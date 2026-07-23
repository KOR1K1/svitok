//! Команды IPC. Мастер-ключ через мост в JS не уходит никогда. Сид уходит
//! только бумажными строками и только когда пользователь сам просит показать
//! его для листка (создание, «показать сид»). Остальное наружу - метаданные
//! и, по явному действию, один конкретный пароль или код.

use crate::AppState;
use serde::Serialize;
use svitok_common::lockmem::LockedKey;
use svitok_common::osrng::{generate_seed, os_random};
use svitok_common::store::{Site, Store};
use svitok_core::derive::{site_password, Policy};
use svitok_core::kdf::{fingerprint, master_key, subkey, KdfParams};
use svitok_core::vault::{decrypt, encrypt, Entry};
use tauri::{Manager, State};

// ---------- ответы наружу ----------

/// Строка-секрет, уходящая наружу через IPC (пароль, бумажный сид). На проводе -
/// обычная строка, но свою Rust-копию она затирает, когда Tauri, отсериализовав
/// ответ, дропает структуру. Внутренний JSON-буфер самого Tauri вне нашего
/// контроля - это остаточный риск webview-моста, он закрывается только уходом от
/// моста (native messaging на десктопе, autofill на Android).
pub struct SecretString(pub String);

impl serde::Serialize for SecretString {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        svitok_core::wipe::wipe_str(&mut self.0);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub has_vault: bool,
    pub has_seed: bool,
    pub unlocked: bool,
}

#[derive(Serialize)]
pub struct Unlocked {
    pub fingerprint: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewVault {
    pub fingerprint: String,
    pub seed_paper: Vec<SecretString>,
}

#[derive(Serialize)]
pub struct SiteView {
    pub id: String,
    pub name: String,
    pub login: String,
    pub counter: u32,
    pub length: usize,
    pub classes: String,
    pub aliases: Vec<String>,
    pub label: String,
}

#[derive(Serialize)]
pub struct PasswordView {
    pub name: String,
    pub login: String,
    pub counter: u32,
    pub password: SecretString,
}

#[derive(Serialize)]
pub struct EntryView {
    pub kind: String,
    pub label: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpView {
    pub label: String,
    pub code: String,
    pub digits: u32,
    pub seconds_left: u32,
    pub period: u32,
}

#[derive(Serialize)]
pub struct Paper {
    pub kdf: String,
    pub sites: Vec<String>,
    pub vault: Vec<String>,
}

#[derive(Serialize)]
pub struct SyncPreview {
    /// Имена сайтов, которых ещё нет - будут добавлены.
    pub added: Vec<String>,
    /// Имена, которые уже есть - при подтверждении будут перезаписаны
    /// (логин/счётчик/политика меняются, а значит и выводимый пароль).
    pub updated: Vec<String>,
}

// ---------- помощники ----------

fn dir_of(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn classes_str(p: &Policy) -> String {
    let mut c = String::new();
    if p.lower { c.push('l') }
    if p.upper { c.push('u') }
    if p.digits { c.push('d') }
    if p.symbols { c.push('s') }
    c
}

/// Рабочая копия мастер-ключа на время команды. Сам мастер-ключ живёт в
/// AppState; сюда берётся копия, которая затирается, когда команда отработала,
/// а не остаётся болтаться в стеке/куче потока tokio.
pub struct MkGuard([u8; 32]);

impl core::ops::Deref for MkGuard {
    type Target = [u8; 32];
    fn deref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for MkGuard {
    fn drop(&mut self) {
        svitok_core::wipe::wipe(&mut self.0);
    }
}

fn require_key(state: &AppState) -> Result<MkGuard, String> {
    // не паникуем, если мьютекс отравлен предыдущей паникой под замком.
    // Берём копию запертого ключа в guard - она короткоживущая и затрётся при
    // выходе из команды; сам запертый оригинал остаётся в состоянии.
    let k = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .master_key
        .as_ref()
        .map(|lk| *lk.get())
        .ok_or_else(|| "заблокировано".to_string())?;
    Ok(MkGuard(k))
}

/// Контрольная метка мастер-ключа (8 hex). По ней при входе понимаем, верна ли фраза.
fn verifier(mk: &[u8; 32]) -> String {
    subkey(mk, b"unlock-verifier")[..4]
        .iter()
        .map(|x| format!("{:02x}", x))
        .collect()
}

/// Имя и логин пишутся в строку списка через пробел как разделитель токенов,
/// поэтому пробел (или иной whitespace) внутри поля подменил бы соседние
/// поля при перечитывании. Режем это на входе.
fn check_field(s: &str, what: &str) -> Result<(), String> {
    if s.chars().any(|c| c.is_whitespace()) {
        return Err(format!("{what}: пробелы недопустимы"));
    }
    Ok(())
}

/// Как запись зовётся в сообщениях: «имя (логин)», чтобы при нескольких
/// аккаунтах на одном домене было понятно, о котором речь.
fn display_site(s: &Site) -> String {
    if s.login.is_empty() {
        s.name.clone()
    } else {
        format!("{} ({})", s.name, s.login)
    }
}

/// Мягкое предупреждение о пересечении доменов: страница совпадёт сразу с
/// несколькими записями с разными доменами деривации - пароли будут разные.
/// Несколько аккаунтов на одном name - штатный случай, о нём не предупреждаем.
fn overlap_warnings(store: &Store, site: &Site) -> Vec<String> {
    let mine: Vec<String> =
        site.domains().filter_map(svitok_core::domain::canonical).collect();
    store
        .sites
        .iter()
        .filter(|o| o.id != site.id && o.name != site.name)
        .filter(|o| {
            o.domains()
                .filter_map(svitok_core::domain::canonical)
                .any(|c| mine.contains(&c))
        })
        .map(display_site)
        .collect()
}

/// Алиасы приходят из формы: режем пустое, пробелы недопустимы (формат строки),
/// запятая - разделитель в токене alias=, внутри домена ей делать нечего.
fn check_aliases(aliases: &[String]) -> Result<(), String> {
    for a in aliases {
        check_field(a, "домен")?;
        if a.contains(',') {
            return Err(format!("{a}: запятая внутри домена"));
        }
    }
    Ok(())
}

/// Табы и переводы строк в отображаемом имени сводим к пробелу.
fn tidy_label(label: &str) -> String {
    label.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------- команды ----------

#[tauri::command]
pub fn status(app: tauri::AppHandle, state: State<AppState>) -> Result<Status, String> {
    let dir = dir_of(&app)?;
    Ok(Status {
        has_vault: Store::exists(&dir),
        has_seed: crate::seed::has_seed(&app, &dir).unwrap_or(false),
        unlocked: state.lock().unwrap_or_else(|p| p.into_inner()).master_key.is_some(),
    })
}

/// Создать новый Свиток: сгенерировать сид, положить его в хранилище сида,
/// вывести мастер-ключ в состояние. Возвращает бумажные строки сида - их
/// пользователь переписывает на листок. Показываем один раз.
#[tauri::command]
pub async fn create_vault(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    phrase: String,
) -> Result<NewVault, String> {
    let dir = dir_of(&app)?;
    // Проверяем и файл списка, и сам сид: иначе при наличии сида в хранилище,
    // но без sites.txt, мы сгенерировали бы новый сид поверх старого и потеряли
    // доступ ко всем уже выведенным паролям.
    if Store::exists(&dir) || crate::seed::has_seed(&app, &dir).unwrap_or(false) {
        return Err("Свиток уже существует".into());
    }
    let mut seed = generate_seed(&[]).map_err(|e| e.to_string())?;
    let seed_paper = svitok_core::base32::to_paper(&seed);

    let mut store = Store::empty(&dir);
    let kdf = store.kdf;
    crate::seed::store_seed(&app, &dir, &seed)?;

    // KDF считаем в фоновом потоке, иначе на Android словим ANR.
    let seed_owned = seed;
    let phrase_bytes = phrase.into_bytes();
    let mk = tauri::async_runtime::spawn_blocking(move || {
        let mut so = seed_owned;
        let mut pb = phrase_bytes;
        let k = master_key(&so, &pb, kdf);
        svitok_core::wipe::wipe(&mut so);
        svitok_core::wipe::wipe(&mut pb);
        k
    })
    .await
    .map_err(|e| e.to_string())?;

    svitok_core::wipe::wipe(&mut seed);
    store.check = Some(verifier(&mk));
    store.save()?;
    let fp = fingerprint(&mk);
    let mut g = state.lock().unwrap_or_else(|p| p.into_inner());
    g.master_key = Some(LockedKey::new(mk));
    g.dir = dir;

    Ok(NewVault {
        fingerprint: String::from_utf8_lossy(&fp).to_string(),
        seed_paper: seed_paper.into_iter().map(SecretString).collect(),
    })
}

/// Разбор сида с листка: либо нумерованные строки с чек-символами
/// («01 …», «== …»), либо просто 26 символов Base32 без разметки.
fn parse_seed(input: &str) -> Result<[u8; 16], String> {
    let looks_numbered = input.lines().any(|l| {
        let t = l.trim();
        t.starts_with("==") || t.split_whitespace().next().is_some_and(|w| w.parse::<u32>().is_ok())
    });
    let mut bytes = if looks_numbered {
        let lines: Vec<&str> = input.lines().collect();
        svitok_core::base32::from_paper(&lines).map_err(|e| match e {
            svitok_core::base32::PaperError::LineCheck(n) => format!("опечатка в строке {n:02}"),
            svitok_core::base32::PaperError::BlobCheck => "итоговая сумма не сошлась".into(),
            svitok_core::base32::PaperError::MissingBlobCheck => "нет строки суммы «== …» - допишите её с листка".into(),
            other => format!("не разобрал сид: {other:?}"),
        })?
    } else {
        let mut chars: Vec<u8> = input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c as u8)
            .collect();
        let out = svitok_core::base32::decode(&chars).ok_or("не разобрал сид — проверьте символы");
        svitok_core::wipe::wipe(&mut chars);
        out?
    };
    if bytes.len() != 16 {
        svitok_core::wipe::wipe(&mut bytes);
        return Err(format!("сид должен быть 16 байт, получено {}", bytes.len()));
    }
    let mut seed = [0u8; 16];
    seed.copy_from_slice(&bytes);
    svitok_core::wipe::wipe(&mut bytes); // расшифрованный сид не оставляем в куче
    Ok(seed)
}

/// Восстановить Свиток из существующего сида (второе устройство).
/// Тот же сид плюс та же фраза дают те же пароли, что и на первом устройстве.
#[tauri::command]
pub async fn restore_vault(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    phrase: String,
    seed: String,
) -> Result<Unlocked, String> {
    let dir = dir_of(&app)?;
    if crate::seed::has_seed(&app, &dir).unwrap_or(false) {
        return Err("на этом устройстве Свиток уже есть".into());
    }
    let mut seed_bytes = parse_seed(&seed)?;
    {
        // сам текст сида с листка (полная строка) тоже не оставляем в куче
        let mut s = seed;
        svitok_core::wipe::wipe_str(&mut s);
    }
    // Если список уже лежит (импортирован из бэкапа) - берём его вместе с
    // параметрами, иначе начинаем с пустого. Сид добавляем только после проверки.
    let mut store = if Store::exists(&dir) { Store::load(&dir)? } else { Store::empty(&dir) };
    let kdf = store.kdf;

    let seed_owned = seed_bytes;
    let phrase_bytes = phrase.into_bytes();
    let mk = tauri::async_runtime::spawn_blocking(move || {
        let mut so = seed_owned;
        let mut pb = phrase_bytes;
        let k = master_key(&so, &pb, kdf);
        svitok_core::wipe::wipe(&mut so);
        svitok_core::wipe::wipe(&mut pb);
        k
    })
    .await
    .map_err(|e| e.to_string())?;

    // В существующем списке лежит верификатор фразы. Сверяем, чтобы неверной
    // парой сид+фраза не затереть чужой сид или список.
    match &store.check {
        Some(existing) if *existing != verifier(&mk) => {
            svitok_core::wipe::wipe(&mut seed_bytes);
            return Err("сид или фраза не совпадают с этим списком".into());
        }
        None => store.check = Some(verifier(&mk)),
        _ => {}
    }

    crate::seed::store_seed(&app, &dir, &seed_bytes)?;
    svitok_core::wipe::wipe(&mut seed_bytes);
    store.save()?;
    let fp = fingerprint(&mk);
    let mut g = state.lock().unwrap_or_else(|p| p.into_inner());
    g.master_key = Some(LockedKey::new(mk));
    g.dir = dir;
    Ok(Unlocked { fingerprint: String::from_utf8_lossy(&fp).to_string() })
}

#[tauri::command]
pub async fn unlock(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    phrase: String,
) -> Result<Unlocked, String> {
    let dir = dir_of(&app)?;
    if !crate::seed::has_seed(&app, &dir)? {
        return Err("сид не найден — сначала создайте Свиток".into());
    }
    let store = if Store::exists(&dir) { Some(Store::load(&dir)?) } else { None };
    let kdf = store.as_ref().map(|s| s.kdf).unwrap_or(KdfParams::DEFAULT);

    let mut seed = crate::seed::load_seed(&app, &dir)?;
    let seed_owned = seed;
    let phrase_bytes = phrase.into_bytes();
    let mut mk = tauri::async_runtime::spawn_blocking(move || {
        // копии сида и фразы в фоновом потоке тоже затираем, а не бросаем в куче
        let mut so = seed_owned;
        let mut pb = phrase_bytes;
        let k = master_key(&so, &pb, kdf);
        svitok_core::wipe::wipe(&mut so);
        svitok_core::wipe::wipe(&mut pb);
        k
    })
    .await
    .map_err(|e| e.to_string())?;
    svitok_core::wipe::wipe(&mut seed);

    // Неверная фраза даёт другой мастер-ключ, а значит метка не совпадёт.
    if let Some(expected) = store.as_ref().and_then(|s| s.check.as_ref()) {
        if verifier(&mk) != *expected {
            svitok_core::wipe::wipe(&mut mk);
            return Err("Неверная фраза".into());
        }
    }

    let fp = fingerprint(&mk);
    let mut g = state.lock().unwrap_or_else(|p| p.into_inner());
    g.master_key = Some(LockedKey::new(mk));
    g.dir = dir;
    Ok(Unlocked { fingerprint: String::from_utf8_lossy(&fp).to_string() })
}

#[tauri::command]
pub fn lock(state: State<AppState>) {
    let mut g = state.lock().unwrap_or_else(|p| p.into_inner());
    g.master_key = None; // Drop у LockedKey затирает ключ и снимает блокировку RAM
}

/// Полностью стереть Свиток: сид из хранилища плюс файлы списка и сейфа.
/// Необратимо. Восстановить можно только с бумажного сида и фразы.
#[tauri::command]
pub fn destroy_vault(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    // необратимое стирание доступно только разблокированному владельцу: иначе
    // скомпрометированный JS мог бы снести Свиток прямо с экрана блокировки
    require_key(&state)?;
    let dir = dir_of(&app)?;
    {
        let mut g = state.lock().unwrap_or_else(|p| p.into_inner());
        g.master_key = None; // Drop у LockedKey затирает ключ и снимает блокировку RAM
    }
    crate::seed::clear_seed(&app, &dir)?;
    let _ = std::fs::remove_file(Store::sites_path(&dir));
    let _ = std::fs::remove_file(Store::vault_path(&dir));
    Ok(())
}

#[tauri::command]
pub fn list_sites(app: tauri::AppHandle, state: State<AppState>) -> Result<Vec<SiteView>, String> {
    require_key(&state)?;
    let dir = dir_of(&app)?;
    if !Store::exists(&dir) {
        return Ok(Vec::new());
    }
    let store = Store::load(&dir)?;
    Ok(store
        .sites
        .iter()
        .map(|s| SiteView {
            id: s.id.clone(),
            name: s.name.clone(),
            login: s.login.clone(),
            counter: s.counter,
            length: s.policy.length,
            classes: classes_str(&s.policy),
            aliases: s.aliases.clone(),
            label: s.label.clone(),
        })
        .collect())
}

/// Возвращает мягкие предупреждения о пересечении доменов с другими записями.
/// Несколько аккаунтов на одном домене - штатный случай: различаются логином.
#[tauri::command]
pub fn add_site(
    app: tauri::AppHandle,
    state: State<AppState>,
    name: String,
    login: String,
    counter: u32,
    length: usize,
    classes: String,
    symbols: Option<String>,
    aliases: Vec<String>,
    label: String,
) -> Result<Vec<String>, String> {
    require_key(&state)?;
    check_field(&name, "имя")?;
    check_field(&login, "логин")?;
    check_aliases(&aliases)?;
    let dir = dir_of(&app)?;
    let mut store = if Store::exists(&dir) { Store::load(&dir)? } else { Store::empty(&dir) };
    // одинаковая пара имя+логин дала бы буквально тот же пароль - это дубль;
    // второй аккаунт на том же домене отличается логином и проходит свободно
    if store.sites.iter().any(|s| s.name == name && s.login == login) {
        return Err(format!("{name}: запись с этим логином уже есть"));
    }
    let policy = Policy::from_classes(length, &classes, symbols.as_deref())
        .ok_or("недопустимая политика")?;
    let site = Site {
        id: store.new_id()?,
        name,
        login,
        counter: counter.max(1),
        policy,
        aliases,
        label: tidy_label(&label),
    };
    let warnings = overlap_warnings(&store, &site);
    store.sites.push(site);
    store.save()?;
    Ok(warnings)
}

#[tauri::command]
pub fn bump_site(app: tauri::AppHandle, state: State<AppState>, id: String) -> Result<u32, String> {
    require_key(&state)?;
    let dir = dir_of(&app)?;
    let mut store = Store::load(&dir)?;
    let s = store.sites.iter_mut().find(|s| s.id == id).ok_or("не найден")?;
    s.counter += 1;
    let c = s.counter;
    store.save()?;
    Ok(c)
}

/// Изменить сайт: логин, счётчик, политику, алиасы, отображаемое имя.
/// Ключ записи - id; name заморожен, он входит в деривацию. Смена логина,
/// счётчика или политики меняет выводимый пароль, как и bump. Так и задумано;
/// алиасы и label - метаданные, на пароль не влияют.
#[tauri::command]
pub fn update_site(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
    login: String,
    counter: u32,
    length: usize,
    classes: String,
    symbols: Option<String>,
    aliases: Vec<String>,
    label: String,
) -> Result<Vec<String>, String> {
    require_key(&state)?;
    check_field(&login, "логин")?;
    check_aliases(&aliases)?;
    let dir = dir_of(&app)?;
    let mut store = Store::load(&dir)?;
    let policy = Policy::from_classes(length, &classes, symbols.as_deref())
        .ok_or("недопустимая политика")?;
    let idx = store.sites.iter().position(|s| s.id == id).ok_or("не найден")?;
    if store.sites.iter().any(|s| s.id != id && s.name == store.sites[idx].name && s.login == login) {
        return Err(format!("{}: запись с этим логином уже есть", store.sites[idx].name));
    }
    let s = &mut store.sites[idx];
    s.login = login;
    s.counter = counter.max(1);
    s.policy = policy;
    s.aliases = aliases;
    s.label = tidy_label(&label);
    let warnings = overlap_warnings(&store, &store.sites[idx]);
    store.save()?;
    Ok(warnings)
}

/// Удалить сайт из списка. Секреты сейфа при этом не трогаем.
#[tauri::command]
pub fn remove_site(app: tauri::AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    require_key(&state)?;
    let dir = dir_of(&app)?;
    let mut store = Store::load(&dir)?;
    let before = store.sites.len();
    store.sites.retain(|s| s.id != id);
    if store.sites.len() == before {
        return Err("не найден".into());
    }
    store.save()
}

/// Снова показать сид - переписать на новый листок, если старый потерян или испорчен.
/// Требуем повторный ввод фразы: разблокировки мало, иначе молчаливый вызов из
/// скомпрометированного JS выгрузил бы сид на бумагу без действия пользователя.
/// Фразу сверяем, заново выведя ключ (на Android чтение сида к тому же проходит
/// через биометрию Keystore).
#[tauri::command]
pub async fn show_seed(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    phrase: String,
) -> Result<Vec<SecretString>, String> {
    require_key(&state)?;
    let dir = dir_of(&app)?;
    let store = if Store::exists(&dir) { Some(Store::load(&dir)?) } else { None };
    let kdf = store.as_ref().map(|s| s.kdf).unwrap_or(KdfParams::DEFAULT);
    let expected = store.as_ref().and_then(|s| s.check.clone());

    let mut seed = crate::seed::load_seed(&app, &dir)?;
    let seed_owned = seed;
    let phrase_bytes = phrase.into_bytes();
    let mut mk = tauri::async_runtime::spawn_blocking(move || {
        let mut so = seed_owned;
        let mut pb = phrase_bytes;
        let k = master_key(&so, &pb, kdf);
        svitok_core::wipe::wipe(&mut so);
        svitok_core::wipe::wipe(&mut pb);
        k
    })
    .await
    .map_err(|e| e.to_string())?;
    let ok = expected.as_deref().map(|e| verifier(&mk) == e).unwrap_or(true);
    svitok_core::wipe::wipe(&mut mk);
    if !ok {
        svitok_core::wipe::wipe(&mut seed);
        return Err("Неверная фраза".into());
    }

    let paper = svitok_core::base32::to_paper(&seed);
    svitok_core::wipe::wipe(&mut seed);
    Ok(paper.into_iter().map(SecretString).collect())
}

#[tauri::command]
pub fn derive_password(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
) -> Result<PasswordView, String> {
    let mk = require_key(&state)?;
    let dir = dir_of(&app)?;
    let store = Store::load(&dir)?;
    let s = store.sites.iter().find(|s| s.id == id).ok_or("не найден")?;
    let password = site_password(&mk, &s.name, &s.login, s.counter, &s.policy)
        .ok_or("негодная политика пароля у этого сайта")?;
    Ok(PasswordView {
        name: s.name.clone(),
        login: s.login.clone(),
        counter: s.counter,
        password: SecretString(password),
    })
}

// ---------- сейф ----------

fn load_entries(dir: &std::path::Path, mk: &[u8; 32]) -> Result<Vec<Entry>, String> {
    let store = Store::load(dir).unwrap_or_else(|_| Store::empty(dir));
    match store.read_vault_blob()? {
        None => Ok(Vec::new()),
        Some(blob) => decrypt(mk, &blob).map_err(|e| match e {
            svitok_core::vault::VaultError::BadMac => "сейф не открывается (ключ/повреждение)".into(),
            other => format!("сейф: {other:?}"),
        }),
    }
}

fn save_entries(dir: &std::path::Path, mk: &[u8; 32], entries: &[Entry]) -> Result<(), String> {
    let mut nonce = [0u8; 12];
    os_random(&mut nonce).map_err(|e| e.to_string())?;
    let blob = encrypt(mk, entries, nonce);
    let store = if Store::exists(dir) { Store::load(dir)? } else { Store::empty(dir) };
    store.write_vault_blob(&blob)?;
    Ok(())
}

#[tauri::command]
pub fn vault_list(app: tauri::AppHandle, state: State<AppState>) -> Result<Vec<EntryView>, String> {
    let mk = require_key(&state)?;
    let dir = dir_of(&app)?;
    let entries = load_entries(&dir, &mk)?;
    Ok(entries
        .iter()
        .map(|e| EntryView { kind: e.kind().to_string(), label: e.label().to_string() })
        .collect())
}

#[tauri::command]
pub fn totp_list(app: tauri::AppHandle, state: State<AppState>) -> Result<Vec<String>, String> {
    let mk = require_key(&state)?;
    let dir = dir_of(&app)?;
    let entries = load_entries(&dir, &mk)?;
    Ok(entries
        .iter()
        .filter_map(|e| match e {
            Entry::Totp { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect())
}

#[tauri::command]
pub fn totp_code(app: tauri::AppHandle, state: State<AppState>, label: String) -> Result<TotpView, String> {
    let mk = require_key(&state)?;
    let dir = dir_of(&app)?;
    let entries = load_entries(&dir, &mk)?;
    for e in &entries {
        if let Entry::Totp { label: l, secret, digits8, period } = e {
            if *l == label {
                let digits = if *digits8 { 8 } else { 6 };
                let now = unix_now();
                let code = svitok_core::totp::totp(secret, now, *period, digits);
                return Ok(TotpView {
                    label: l.clone(),
                    code: format!("{:0width$}", code, width = digits as usize),
                    digits,
                    seconds_left: svitok_core::totp::seconds_left(now, *period),
                    period: *period,
                });
            }
        }
    }
    Err(format!("TOTP «{label}» не найден"))
}

#[tauri::command]
pub fn vault_add_totp(
    app: tauri::AppHandle,
    state: State<AppState>,
    label: String,
    secret_b32: String,
    digits8: bool,
    period: u32,
) -> Result<TotpView, String> {
    let mk = require_key(&state)?;
    let dir = dir_of(&app)?;
    let raw = svitok_core::totp::decode_rfc4648(&secret_b32).ok_or("не Base32-секрет")?;
    if raw.is_empty() {
        return Err("пустой секрет".into());
    }
    let mut entries = load_entries(&dir, &mk)?;
    // формат сейфа кодирует только 15/30/60 c. Любой другой период раньше молча
    // схлопывался в 30 и коды генерировались неверно - теперь честно отказываем.
    let per = if period == 0 { 30 } else { period };
    if per != 15 && per != 30 && per != 60 {
        return Err(format!("период {per} c не поддерживается (только 15, 30 или 60)"));
    }
    // Сразу считаем код, чтобы пользователь сверил его с сайтом.
    let digits = if digits8 { 8 } else { 6 };
    let now = unix_now();
    let code = svitok_core::totp::totp(&raw, now, per, digits);
    entries.push(Entry::Totp { label: label.clone(), secret: raw, digits8, period: per });
    save_entries(&dir, &mk, &entries)?;
    Ok(TotpView {
        label,
        code: format!("{:0width$}", code, width = digits as usize),
        digits,
        seconds_left: svitok_core::totp::seconds_left(now, per),
        period: per,
    })
}

#[tauri::command]
pub fn vault_add_password(
    app: tauri::AppHandle,
    state: State<AppState>,
    label: String,
    secret: String,
) -> Result<(), String> {
    let mk = require_key(&state)?;
    let dir = dir_of(&app)?;
    let mut entries = load_entries(&dir, &mk)?;
    entries.push(Entry::Password { label, secret: secret.into_bytes() });
    save_entries(&dir, &mk, &entries)
}

#[tauri::command]
pub fn vault_add_note(
    app: tauri::AppHandle,
    state: State<AppState>,
    label: String,
    text: String,
) -> Result<(), String> {
    let mk = require_key(&state)?;
    let dir = dir_of(&app)?;
    let mut entries = load_entries(&dir, &mk)?;
    entries.push(Entry::Note { label, text });
    save_entries(&dir, &mk, &entries)
}

#[tauri::command]
pub fn vault_add_codes(
    app: tauri::AppHandle,
    state: State<AppState>,
    label: String,
    codes: Vec<String>,
) -> Result<(), String> {
    let mk = require_key(&state)?;
    let dir = dir_of(&app)?;
    let mut entries = load_entries(&dir, &mk)?;
    entries.push(Entry::Codes { label, codes });
    save_entries(&dir, &mk, &entries)
}

#[tauri::command]
pub fn vault_remove(app: tauri::AppHandle, state: State<AppState>, label: String) -> Result<(), String> {
    let mk = require_key(&state)?;
    let dir = dir_of(&app)?;
    let mut entries = load_entries(&dir, &mk)?;
    let before = entries.len();
    entries.retain(|e| e.label() != label);
    if entries.len() == before {
        return Err(format!("«{label}» не найдено"));
    }
    save_entries(&dir, &mk, &entries)
}

/// SVG QR-кода: перенести секрет на другое устройство камерой.
/// QR несёт секрет, поэтому нужна разблокировка.
#[tauri::command]
pub fn qr_svg(state: State<AppState>, data: String) -> Result<String, String> {
    require_key(&state)?;
    svitok_common::qr::to_svg(&data)
}

/// Включить или выключить защиту от захвата экрана на лету, без перезапуска.
/// На Android это FLAG_SECURE через плагин, на десктопе - set_content_protected.
/// Снять защиту можно только после разблокировки - иначе скомпрометированный JS
/// открыл бы окно для записи ещё на экране ввода фразы.
#[tauri::command]
pub fn set_screen_protection(app: tauri::AppHandle, state: State<AppState>, on: bool) -> Result<(), String> {
    if !on {
        require_key(&state)?;
    }
    #[cfg(target_os = "android")]
    {
        #[derive(Serialize)]
        struct A {
            on: bool,
        }
        let p = app.state::<crate::SeedPlugin>();
        let _: serde_json::Value = p.0.run_mobile_plugin("setSecure", A { on }).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let win = app.get_webview_window("main").ok_or("нет окна")?;
        win.set_content_protected(on).map_err(|e| e.to_string())
    }
}

/// Копировать в буфер. На Android идём через плагин, который метит содержимое
/// как чувствительное (не светится в превью буфера, не уходит в облако клавиатур).
/// На десктопе - обычная запись через clipboard-manager.
#[tauri::command]
pub fn clip_copy(app: tauri::AppHandle, state: State<AppState>, text: String) -> Result<(), String> {
    require_key(&state)?;
    #[cfg(target_os = "android")]
    {
        #[derive(Serialize)]
        struct A {
            text: String,
        }
        let p = app.state::<crate::SeedPlugin>();
        let _: serde_json::Value = p.0.run_mobile_plugin("copyClip", A { text }).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        // на Windows исключаем пароль из истории буфера и облачного буфера
        let _ = app;
        crate::winclip::copy_excluded(&text)
    }
    #[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
    {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        app.clipboard().write_text(text).map_err(|e| e.to_string())
    }
}

/// Очистить буфер (после показа пароля и при блокировке).
#[tauri::command]
pub fn clip_clear(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let p = app.state::<crate::SeedPlugin>();
        let _: serde_json::Value = p.0.run_mobile_plugin("clearClip", ()).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        app.clipboard().write_text(String::new()).map_err(|e| e.to_string())
    }
}

const BACKUP_HEADER: &str = "SVITOK-BACKUP v1";
const BACKUP_SITES: &str = "--- SITES ---";
const BACKUP_VAULT: &str = "--- VAULT ---";

/// Экспорт списка сайтов и сейфа одним текстом для внешнего бэкапа.
/// Секрета тут нет: список - метаданные, сейф - шифртекст, без сида и фразы бесполезен.
#[tauri::command]
pub fn backup_export(app: tauri::AppHandle, state: State<AppState>) -> Result<String, String> {
    require_key(&state)?;
    let dir = dir_of(&app)?;
    let sites = std::fs::read_to_string(Store::sites_path(&dir)).unwrap_or_default();
    let vault = std::fs::read_to_string(Store::vault_path(&dir)).unwrap_or_default();
    Ok(format!(
        "{}\n{}\n{}\n{}\n{}",
        BACKUP_HEADER,
        BACKUP_SITES,
        sites.trim_end(),
        BACKUP_VAULT,
        vault.trim_end()
    ))
}

/// Импорт бэкапа: восстанавливает sites.txt и vault.b32 из текста.
/// Применять после ввода того же сида и фразы - тогда сейф расшифруется тем же ключом.
#[tauri::command]
pub fn backup_import(app: tauri::AppHandle, state: State<AppState>, data: String) -> Result<usize, String> {
    let mk = require_key(&state)?;
    if !data.trim_start().starts_with(BACKUP_HEADER) {
        return Err("это не резервная копия Свитка".into());
    }
    let sites_start = data.find(BACKUP_SITES).ok_or("нет секции сайтов")? + BACKUP_SITES.len();
    let vault_pos = data.find(BACKUP_VAULT).ok_or("нет секции сейфа")?;
    let sites = data[sites_start..vault_pos].trim();
    let vault = data[vault_pos + BACKUP_VAULT.len()..].trim();

    let dir = dir_of(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // параметры, под которыми выведен текущий ключ. Импорт не должен их менять.
    let current_kdf = if Store::exists(&dir) { Store::load(&dir)?.kdf } else { KdfParams::DEFAULT };

    // всё проверяем во временной папке: список парсится, а сейф обязан
    // расшифроваться текущим ключом. Иначе кривой импорт затёр бы рабочий
    // vault.b32 и утащил бы за собой все TOTP-секреты.
    let tmpdir = dir.join(".import");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&tmpdir).map_err(|e| e.to_string())?;
    let checked = (|| -> Result<usize, String> {
        svitok_common::store::atomic_write(&Store::sites_path(&tmpdir), format!("{sites}\n").as_bytes())?;
        let store = Store::load(&tmpdir).map_err(|e| format!("список сайтов повреждён: {e}"))?;
        // Метаданные из копии обязаны сойтись с текущим ключом. Иначе импорт с
        // чужими «# kdf»/«# check» (даже при пустой секции сейфа) сменил бы
        // параметры вывода, следующий unlock дал бы другой mk, и рабочий
        // vault.b32 больше не расшифровался бы никогда.
        if store.kdf != current_kdf {
            return Err("копия сделана с другими параметрами KDF - импорт закрыл бы доступ к сейфу".into());
        }
        if let Some(check) = &store.check {
            if *check != verifier(&mk) {
                return Err("копия сделана из другого сида или фразы".into());
            }
        }
        if !vault.is_empty() {
            svitok_common::store::atomic_write(&Store::vault_path(&tmpdir), format!("{vault}\n").as_bytes())?;
            let blob = store.read_vault_blob()?.ok_or("сейф в копии не читается")?;
            decrypt(&mk, &blob).map_err(|_| "сейф в копии не расшифровывается этим сидом и фразой".to_string())?;
        }
        Ok(store.sites.len())
    })();
    let _ = std::fs::remove_dir_all(&tmpdir);
    let n = checked?;

    svitok_common::store::atomic_write(&Store::sites_path(&dir), format!("{sites}\n").as_bytes())?;
    if !vault.is_empty() {
        svitok_common::store::atomic_write(&Store::vault_path(&dir), format!("{vault}\n").as_bytes())?;
    }
    Ok(n)
}

const SYNC_HEADER: &str = "SVSYNC1";

/// Экспорт списка сайтов для переноса по QR на другое устройство.
/// Тут только метаданные (имя, логин, счётчик, политика), не секрет; пароли на
/// втором устройстве выводятся из того же сида и фразы. Влезает в один QR
/// (до ~2331 байта, версии QR 1-40).
#[tauri::command]
pub fn sync_export(app: tauri::AppHandle, state: State<AppState>) -> Result<String, String> {
    require_key(&state)?;
    let dir = dir_of(&app)?;
    let store = if Store::exists(&dir) { Store::load(&dir)? } else { Store::empty(&dir) };
    let mut out = String::from(SYNC_HEADER);
    for s in &store.sites {
        out.push('\n');
        out.push_str(&s.to_line());
    }
    if out.len() > svitok_common::qr::MAX_BYTES {
        return Err(format!(
            "список слишком велик для одного QR ({} байт); используйте резервную копию",
            out.len()
        ));
    }
    Ok(out)
}

fn parse_sync(data: &str) -> Result<Vec<Site>, String> {
    let body = data
        .trim()
        .strip_prefix(SYNC_HEADER)
        .ok_or("это не перенос списка Свитка")?;
    let mut incoming: Vec<Site> = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        incoming.push(Site::from_line(line).map_err(|e| format!("строка «{line}»: {e}"))?);
    }
    if incoming.is_empty() {
        return Err("в переносе нет сайтов".into());
    }
    Ok(incoming)
}

/// С кем в списке сливается входящая запись: по id, если оба его знают
/// (устройства с этой версией шлют id в QR), иначе по паре имя+логин -
/// на неё падают переносы из старых версий, где id ещё не было.
fn merge_target(store: &Store, site: &Site) -> Option<usize> {
    if !site.id.is_empty() {
        if let Some(i) = store.sites.iter().position(|s| s.id == site.id) {
            return Some(i);
        }
    }
    store.sites.iter().position(|s| s.name == site.name && s.login == site.login)
}

/// Что даст импорт из этого QR: какие сайты добавятся, какие перезапишутся.
/// Обновление существующего меняет выводимый пароль, поэтому диф показываем
/// пользователю до применения, а не переписываем молча.
#[tauri::command]
pub fn sync_preview(app: tauri::AppHandle, state: State<AppState>, data: String) -> Result<SyncPreview, String> {
    require_key(&state)?;
    let incoming = parse_sync(&data)?;
    let dir = dir_of(&app)?;
    let store = if Store::exists(&dir) { Store::load(&dir)? } else { Store::empty(&dir) };
    let mut added = Vec::new();
    let mut updated = Vec::new();
    for site in &incoming {
        if merge_target(&store, site).is_some() {
            updated.push(display_site(site));
        } else {
            added.push(display_site(site));
        }
    }
    Ok(SyncPreview { added, updated })
}

/// Импорт списка сайтов из QR: новые добавляем всегда. Существующие
/// перезаписываем только при `overwrite=true` - это отдельное подтверждение
/// пользователя после показа дифа (см. sync_preview).
#[tauri::command]
pub fn sync_import(app: tauri::AppHandle, state: State<AppState>, data: String, overwrite: bool) -> Result<usize, String> {
    require_key(&state)?;
    let incoming = parse_sync(&data)?;
    let dir = dir_of(&app)?;
    let mut store = if Store::exists(&dir) { Store::load(&dir)? } else { Store::empty(&dir) };
    let mut changed = 0usize;
    for mut site in incoming {
        match merge_target(&store, &site) {
            Some(i) => {
                if overwrite {
                    if site.id.is_empty() {
                        // перенос со старой версии без id - не теряем свой
                        site.id = store.sites[i].id.clone();
                    }
                    store.sites[i] = site;
                    changed += 1;
                }
            }
            None => {
                if site.id.is_empty() {
                    site.id = store.new_id()?;
                }
                store.sites.push(site);
                changed += 1;
            }
        }
    }
    store.save()?;
    Ok(changed)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// Путь выбранного файла - его же фронт передаст в import_apply.
    pub path: String,
    /// Что добавится, в виде «имя (логин)».
    pub added: Vec<String>,
    /// Сколько записей уже есть (та же пара имя+логин) - их не трогаем.
    pub existing: usize,
    /// Строки без домена и имени - не разобрали.
    pub skipped: usize,
}

/// Разобрать файл импорта и свериться со списком. Пароли из файла не читаем
/// в записи вовсе (см. import.rs), текст файла затираем.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn import_parse_diff(dir: &std::path::Path, path: &std::path::Path) -> Result<(Vec<crate::import::ImportedSite>, usize, usize), String> {
    let mut text = std::fs::read_to_string(path).map_err(|e| format!("не прочитал файл: {e}"))?;
    let parsed = crate::import::parse(&text);
    svitok_core::wipe::wipe_str(&mut text);
    let parsed = parsed?;
    let store = if Store::exists(dir) { Store::load(dir)? } else { Store::empty(dir) };
    let (mut fresh, mut existing) = (Vec::new(), 0usize);
    for s in parsed.sites {
        if store.sites.iter().any(|e| e.name == s.name && e.login == s.login) {
            existing += 1;
        } else {
            fresh.push(s);
        }
    }
    Ok((fresh, existing, parsed.skipped))
}

/// Импорт, шаг 1: выбрать файл и показать, что из него выйдет. Диалог зовётся
/// из Rust - содержимое файла (включая чужие пароли) в webview не попадает,
/// наружу уходят только домены и логины. Телефон списки получает по QR.
#[tauri::command]
pub async fn import_pick(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Option<ImportPreview>, String> {
    require_key(&state)?;
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = app;
        Err("импорт файлов - на десктопе; на телефон список переносится по QR".into())
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_dialog::DialogExt;
        let picked = app
            .dialog()
            .file()
            .add_filter("CSV / JSON", &["csv", "json", "txt"])
            .blocking_pick_file();
        let Some(f) = picked else { return Ok(None) };
        let path = f.into_path().map_err(|e| e.to_string())?;
        let dir = dir_of(&app)?;
        let (fresh, existing, skipped) = import_parse_diff(&dir, &path)?;
        Ok(Some(ImportPreview {
            path: path.display().to_string(),
            added: fresh
                .iter()
                .map(|s| if s.login.is_empty() { s.name.clone() } else { format!("{} ({})", s.name, s.login) })
                .collect(),
            existing,
            skipped,
        }))
    }
}

/// Импорт, шаг 2: добавить новые записи из того же файла. Только добавление:
/// у импорта нет ни счётчиков, ни политик, так что существующие пары
/// имя+логин ему менять нечего. Политика у новых - умолчания (20, luds).
#[tauri::command]
pub async fn import_apply(app: tauri::AppHandle, state: State<'_, AppState>, path: String) -> Result<usize, String> {
    require_key(&state)?;
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (app, path);
        Err("импорт файлов - на десктопе".into())
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let dir = dir_of(&app)?;
        let (fresh, _, _) = import_parse_diff(&dir, std::path::Path::new(&path))?;
        let mut store = if Store::exists(&dir) { Store::load(&dir)? } else { Store::empty(&dir) };
        let policy = Policy::from_classes(Policy::DEFAULT_LEN, "luds", None).ok_or("политика по умолчанию")?;
        let n = fresh.len();
        for s in fresh {
            let site = Site {
                id: store.new_id()?,
                name: s.name,
                login: s.login,
                counter: 1,
                policy: policy.clone(),
                aliases: Vec::new(),
                label: String::new(),
            };
            store.sites.push(site);
        }
        store.save()?;
        Ok(n)
    }
}

/// Токен связки для браузерного расширения (десктоп). Показывается в настройках,
/// пользователь один раз копирует его в расширение - это и есть подтверждение.
#[tauri::command]
pub fn autofill_token(app: tauri::AppHandle, state: State<AppState>) -> Result<String, String> {
    require_key(&state)?;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let dir = dir_of(&app)?;
        crate::ipc_server::get_or_create_token(&dir)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = app;
        Err("автозаполнение через расширение - только на десктопе".into())
    }
}

#[tauri::command]
pub fn paper_export(app: tauri::AppHandle, state: State<AppState>) -> Result<Paper, String> {
    require_key(&state)?;
    let dir = dir_of(&app)?;
    let store = if Store::exists(&dir) { Store::load(&dir)? } else { Store::empty(&dir) };
    let sites: Vec<String> = store.sites.iter().map(|s| s.to_line()).collect();
    let vault = match store.read_vault_blob()? {
        Some(blob) => svitok_core::base32::to_paper(&blob),
        None => Vec::new(),
    };
    Ok(Paper { kdf: store.kdf.to_paper(), sites, vault })
}
