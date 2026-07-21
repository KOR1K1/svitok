//! svitok - бумажный менеджер паролей.
//! Сид живёт только на листке, фраза - только в голове.
//! На диске лишь метаданные (sites.txt) и шифрованный сейф (vault.b32).

mod term;

use std::path::PathBuf;
use svitok_common::store::{Site, Store};
use svitok_core::base32;
use svitok_core::derive::{site_password, Policy};
use svitok_core::kdf::{fingerprint, master_key, KdfParams};
use svitok_core::totp;
use svitok_core::vault::{decrypt, encrypt, Entry};
use svitok_core::wipe::Secret;

fn main() {
    term::enable_ansi();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut dir = PathBuf::from(".");
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--dir" {
            if let Some(d) = it.next() {
                dir = PathBuf::from(d);
            }
        } else {
            rest.push(a);
        }
    }
    let code = match run(&dir, &rest) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("ошибка: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn run(dir: &PathBuf, args: &[String]) -> Result<(), String> {
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    match cmd {
        "new" => cmd_new(dir),
        "add" => cmd_add(dir, &args[1..]),
        "bump" => cmd_bump(dir, &args[1..]),
        "ls" => cmd_ls(dir),
        "pw" => {
            let store = Store::load(dir)?;
            let mk = unlock(store.kdf)?;
            cmd_pw(&store, &mk.0, &args[1..])
        }
        "totp" => {
            let store = Store::load(dir)?;
            let mk = unlock(store.kdf)?;
            cmd_totp(&store, &mk.0, &args[1..])
        }
        "vault" => {
            let store = Store::load(dir)?;
            let mk = unlock(store.kdf)?;
            cmd_vault(&store, &mk.0, &args[1..])
        }
        "paper" => cmd_paper(dir),
        "check" => cmd_check(),
        "shell" => cmd_shell(dir),
        _ => {
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!(
        "svitok — бумажный менеджер паролей

  svitok new                      создать сид (показывается ОДИН раз — на листок!)
  svitok add <сайт> [параметры]   добавить сайт: len=20 cls=luds login=я v=1 sym=._-
  svitok bump <сайт>              сменить пароль сайта после утечки (v+1)
  svitok ls                       список сайтов
  svitok pw <сайт>                показать пароль сайта
  svitok totp <метка>             одноразовый код 2FA
  svitok vault <подкоманда>       сейф: ls | show <м> | rm <м> |
                                  add-pw <м> | add-totp <м> | add-codes <м> | add-note <м>
  svitok paper                    всё, что должно быть переписано на листок
  svitok check                    проверить переписанные строки
  svitok shell                    сессия: один ввод сида на много команд
  --dir <папка>                   где лежат sites.txt и vault.b32 (по умолчанию текущая)"
    );
}

// ---------- Разблокировка ----------

/// Мастер-ключ, который зануляется при уничтожении.
struct Mk([u8; 32]);
impl Drop for Mk {
    fn drop(&mut self) {
        svitok_core::wipe::wipe(&mut self.0);
    }
}

fn read_seed() -> Result<Secret, String> {
    println!("Сид с листка (строки «01 XXXX ...», пустая строка — конец;");
    println!("можно одной строкой без номеров и чек-символов):");
    let lines = term::read_multiline("").map_err(|e| e.to_string())?;
    if lines.is_empty() {
        return Err("сид не введён".into());
    }
    let looks_numbered = lines.iter().any(|l| {
        let t = l.trim();
        t.starts_with("==") || t.split_whitespace().next().is_some_and(|w| w.parse::<u32>().is_ok())
    });
    let seed = if looks_numbered {
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        base32::from_paper(&refs).map_err(|e| match e {
            base32::PaperError::LineCheck(n) => format!("опечатка в строке {n:02} — сверьтесь с листком"),
            base32::PaperError::LineNumber(n) => format!("не хватает строки {n:02}"),
            base32::PaperError::BlobCheck => "итоговая сумма не сошлась".to_string(),
            other => format!("не разобрал ввод: {other:?}"),
        })?
    } else {
        let chars: Vec<u8> = lines
            .join("")
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c as u8)
            .collect();
        base32::decode(&chars).ok_or("не разобрал сид — проверьте символы")?
    };
    if seed.len() != svitok_core::SEED_LEN {
        return Err(format!("сид должен быть {} байт, получено {}", svitok_core::SEED_LEN, seed.len()));
    }
    // Убираем набранный сид с экрана: строки ввода плюс две подсказки.
    term::erase_lines(lines.len() + 3);
    println!("сид принят ({} строк ввода стёрто с экрана)", lines.len());
    Ok(Secret::new(seed))
}

fn unlock(kdf: KdfParams) -> Result<Mk, String> {
    let seed = read_seed()?;
    let phrase = term::read_secret("Мастер-фраза: ").map_err(|e| e.to_string())?;
    println!("вычисляю ключ (KDF {}) ...", kdf.to_paper());
    let mk = master_key(seed.as_slice(), phrase.as_slice(), kdf);
    let fp = fingerprint(&mk);
    println!("отпечаток ключа: {}{}  (должен совпадать с листком)", fp[0] as char, fp[1] as char);
    Ok(Mk(mk))
}

// ---------- Команды ----------

fn cmd_new(dir: &PathBuf) -> Result<(), String> {
    let sites_path = Store::sites_path(dir);
    if sites_path.exists() {
        return Err(format!("{} уже существует — не перезаписываю", sites_path.display()));
    }
    println!("Создание нового «Свитка».");
    let noise = term::read_secret("Помашите руками по клавиатуре и нажмите Enter: ")
        .map_err(|e| e.to_string())?;
    let mut seed = term::generate_seed(noise.as_slice()).map_err(|e| e.to_string())?;

    let kdf = KdfParams::DEFAULT;
    println!("\n=== ЗАПИШИТЕ НА ЛИСТОК — показывается ЕДИНСТВЕННЫЙ раз ===\n");
    println!("SVITOK v1   KDF {}", kdf.to_paper());
    for l in base32::to_paper(&seed) {
        println!("  {l}");
    }
    println!();

    let phrase = term::read_secret("Придумайте мастер-фразу (останется только в голове): ")
        .map_err(|e| e.to_string())?;
    let phrase2 = term::read_secret("Повторите фразу: ").map_err(|e| e.to_string())?;
    if phrase.as_slice() != phrase2.as_slice() {
        svitok_core::wipe::wipe(&mut seed);
        return Err("фразы не совпали — начните заново".into());
    }
    println!("вычисляю отпечаток (KDF {}) ...", kdf.to_paper());
    let mut mk = master_key(&seed, phrase.as_slice(), kdf);
    let fp = fingerprint(&mk);
    println!("отпечаток ключа: {}{}   ← допишите на листок рядом с сидом", fp[0] as char, fp[1] as char);
    svitok_core::wipe::wipe(&mut mk);
    svitok_core::wipe::wipe(&mut seed);

    let store = Store { dir: dir.clone(), kdf, sites: Vec::new(), check: None };
    store.save()?;
    println!(
        "\nсоздан {}\nДальше: svitok add <сайт>, пароль — svitok pw <сайт>.\n\
         Перепишите сид на листок сейчас: после Enter он исчезнет с экрана.",
        sites_path.display()
    );
    let _ = term::read_line("Enter — стереть сид с экрана... ");
    term::erase_lines(14);
    println!("сид стёрт с экрана");
    Ok(())
}

fn cmd_add(dir: &PathBuf, args: &[String]) -> Result<(), String> {
    let name = args.first().ok_or("укажите сайт: svitok add <сайт>")?.clone();
    let mut store = Store::load(dir)?;
    if store.sites.iter().any(|s| s.name == name) {
        return Err(format!("{name} уже есть (сменить пароль: svitok bump {name})"));
    }
    let mut login = String::new();
    let mut counter = 1u32;
    let mut len = Policy::DEFAULT_LEN;
    let mut cls = "luds".to_string();
    let mut sym: Option<String> = None;
    for t in &args[1..] {
        if let Some(v) = t.strip_prefix("login=") {
            login = v.to_string();
        } else if let Some(v) = t.strip_prefix("v=") {
            counter = v.parse().map_err(|_| "плохой v=")?;
        } else if let Some(v) = t.strip_prefix("len=") {
            len = v.parse().map_err(|_| "плохая len=")?;
        } else if let Some(v) = t.strip_prefix("cls=") {
            cls = v.to_string();
        } else if let Some(v) = t.strip_prefix("sym=") {
            sym = Some(v.to_string());
        } else {
            return Err(format!("непонятный параметр {t}"));
        }
    }
    let policy = Policy::from_classes(len, &cls, sym.as_deref()).ok_or("недопустимая политика")?;
    let site = Site { name, login, counter, policy };
    println!("добавлено: {}", site.to_line());
    store.sites.push(site);
    store.save()?;
    println!("не забудьте дописать строку в бумажный список сайтов");
    Ok(())
}

fn cmd_bump(dir: &PathBuf, args: &[String]) -> Result<(), String> {
    let name = args.first().ok_or("укажите сайт")?;
    let mut store = Store::load(dir)?;
    let site = store
        .sites
        .iter_mut()
        .find(|s| &s.name == name)
        .ok_or(format!("{name} не найден"))?;
    site.counter += 1;
    println!("{} → v={}  (пароль изменился; поменяйте его на сайте)", site.name, site.counter);
    let line = site.to_line();
    store.save()?;
    println!("обновите строку на листке: {line}");
    Ok(())
}

fn cmd_ls(dir: &PathBuf) -> Result<(), String> {
    let store = Store::load(dir)?;
    if store.sites.is_empty() {
        println!("сайтов нет — svitok add <сайт>");
    }
    for s in &store.sites {
        println!("{}", s.to_line());
    }
    Ok(())
}

fn cmd_pw(store: &Store, mk: &[u8; 32], args: &[String]) -> Result<(), String> {
    let query = args.first().ok_or("укажите сайт")?;
    let found = store.find(query);
    match found.len() {
        0 => Err(format!("{query} не найден (svitok ls)")),
        1 => {
            let s = found[0];
            let pw = site_password(mk, &s.name, &s.login, s.counter, &s.policy);
            println!("{}  v={}  {}", s.name, s.counter, if s.login.is_empty() { "" } else { &s.login });
            println!("{pw}");
            let _ = term::read_line("Enter — стереть с экрана... ");
            term::erase_lines(3);
            println!("стёрто");
            Ok(())
        }
        _ => {
            println!("уточните:");
            for s in found {
                println!("  {}", s.name);
            }
            Ok(())
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load_entries(store: &Store, mk: &[u8; 32]) -> Result<Vec<Entry>, String> {
    match store.read_vault_blob()? {
        None => Ok(Vec::new()),
        Some(blob) => decrypt(mk, &blob).map_err(|e| match e {
            svitok_core::vault::VaultError::BadMac => {
                "сейф не открывается: неверная фраза/сид или повреждён vault.b32".to_string()
            }
            other => format!("сейф повреждён: {other:?}"),
        }),
    }
}

fn save_entries(store: &Store, mk: &[u8; 32], entries: &[Entry]) -> Result<(), String> {
    let mut nonce = [0u8; 12];
    term::os_random(&mut nonce).map_err(|e| e.to_string())?;
    let blob = encrypt(mk, entries, nonce);
    let lines = store.write_vault_blob(&blob)?;
    println!("\nсейф изменён — перепишите его на листок заново ({} строк):", lines.len());
    for l in &lines {
        println!("  {l}");
    }
    Ok(())
}

fn cmd_totp(store: &Store, mk: &[u8; 32], args: &[String]) -> Result<(), String> {
    let label = args.first().ok_or("укажите метку: svitok totp <метка>")?;
    let entries = load_entries(store, mk)?;
    let e = entries
        .iter()
        .find(|e| matches!(e, Entry::Totp { .. }) && e.label().contains(label.as_str()))
        .ok_or(format!("TOTP «{label}» не найден (svitok vault ls)"))?;
    if let Entry::Totp { label, secret, digits8, period } = e {
        let digits = if *digits8 { 8 } else { 6 };
        let now = unix_now();
        let code = totp::totp(secret, now, *period, digits);
        let left = totp::seconds_left(now, *period);
        println!("{label}: {:0width$}   (ещё {left} c)", code, width = digits as usize);
    }
    Ok(())
}

fn cmd_vault(store: &Store, mk: &[u8; 32], args: &[String]) -> Result<(), String> {
    let sub = args.first().map(String::as_str).unwrap_or("ls");
    let mut entries = load_entries(store, mk)?;
    let label_arg = || -> Result<String, String> {
        args.get(1).cloned().ok_or("укажите метку".to_string())
    };
    match sub {
        "ls" => {
            if entries.is_empty() {
                println!("сейф пуст");
            }
            for e in &entries {
                println!("{:6} {}", e.kind(), e.label());
            }
            Ok(())
        }
        "show" => {
            let label = label_arg()?;
            let e = entries
                .iter()
                .find(|e| e.label().contains(&label))
                .ok_or(format!("«{label}» не найдено"))?;
            let shown = match e {
                Entry::Password { label, secret } => {
                    println!("{label}: {}", String::from_utf8_lossy(secret));
                    2
                }
                Entry::Totp { label, secret, digits8, period } => {
                    println!("{label}: totp, {} цифр, период {period} c", if *digits8 { 8 } else { 6 });
                    println!("секрет (RFC4648): {}", rfc4648_encode(secret));
                    3
                }
                Entry::Codes { label, codes } => {
                    println!("{label}:");
                    for c in codes {
                        println!("  {c}");
                    }
                    codes.len() + 2
                }
                Entry::Note { label, text } => {
                    println!("{label}: {text}");
                    2
                }
            };
            let _ = term::read_line("Enter — стереть с экрана... ");
            term::erase_lines(shown);
            println!("стёрто");
            Ok(())
        }
        "rm" => {
            let label = label_arg()?;
            let before = entries.len();
            entries.retain(|e| e.label() != label);
            if entries.len() == before {
                return Err(format!("«{label}» не найдено (нужно точное имя)"));
            }
            save_entries(store, mk, &entries)
        }
        "add-pw" => {
            let label = label_arg()?;
            let secret = term::read_secret("Пароль: ").map_err(|e| e.to_string())?;
            entries.push(Entry::Password { label, secret: secret.as_slice().to_vec() });
            save_entries(store, mk, &entries)
        }
        "add-totp" => {
            let label = label_arg()?;
            let s = term::read_secret("TOTP-секрет (Base32 c сайта, A-Z2-7): ")
                .map_err(|e| e.to_string())?;
            let raw = totp::decode_rfc4648(core::str::from_utf8(s.as_slice()).map_err(|_| "не UTF-8")?)
                .ok_or("не похоже на Base32-секрет")?;
            if raw.is_empty() {
                return Err("пустой секрет".into());
            }
            let digits8 = args.iter().any(|a| a == "digits=8");
            let period = args
                .iter()
                .find_map(|a| a.strip_prefix("period=").and_then(|v| v.parse().ok()))
                .unwrap_or(30);
            // Сразу показываем текущий код - сверьте его с сайтом.
            let code = totp::totp(&raw, unix_now(), period, if digits8 { 8 } else { 6 });
            println!("текущий код: {:0w$} — сверьте с сайтом до сохранения", code, w = if digits8 { 8 } else { 6 });
            let ok = term::read_line("Совпадает? [y/N]: ").map_err(|e| e.to_string())?;
            if !ok.eq_ignore_ascii_case("y") {
                return Err("отменено".into());
            }
            entries.push(Entry::Totp { label, secret: raw, digits8, period });
            save_entries(store, mk, &entries)
        }
        "add-codes" => {
            let label = label_arg()?;
            let codes = term::read_multiline("Recovery-коды, по одному в строке (пустая — конец):")
                .map_err(|e| e.to_string())?;
            if codes.is_empty() {
                return Err("кодов нет".into());
            }
            entries.push(Entry::Codes { label, codes });
            save_entries(store, mk, &entries)
        }
        "add-note" => {
            let label = label_arg()?;
            let lines = term::read_multiline("Текст заметки (пустая строка — конец):")
                .map_err(|e| e.to_string())?;
            entries.push(Entry::Note { label, text: lines.join("\n") });
            save_entries(store, mk, &entries)
        }
        other => Err(format!("неизвестная подкоманда vault {other}")),
    }
}

fn rfc4648_encode(data: &[u8]) -> String {
    const A: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &b in data {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(A[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(A[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn cmd_paper(dir: &PathBuf) -> Result<(), String> {
    let store = Store::load(dir)?;
    println!("=== ЛИСТОК «СВИТОК» (плюс сид, записанный при svitok new) ===\n");
    println!("SVITOK v1   KDF {}\n", store.kdf.to_paper());
    println!("--- сайты ---");
    for s in &store.sites {
        println!("{}", s.to_line());
    }
    match store.read_vault_blob()? {
        None => println!("\n--- сейф пуст ---"),
        Some(blob) => {
            println!("\n--- сейф ---");
            for l in base32::to_paper(&blob) {
                println!("{l}");
            }
        }
    }
    Ok(())
}

fn cmd_check() -> Result<(), String> {
    println!("Вставьте строки с листка (сейф или сид), пустая строка — конец:");
    let lines = term::read_multiline("").map_err(|e| e.to_string())?;
    let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    match base32::from_paper(&refs) {
        Ok(data) => {
            println!("✓ всё сходится: {} байт, сумма {}", data.len(),
                String::from_utf8_lossy(&base32::blob_check(&data)));
            Ok(())
        }
        Err(base32::PaperError::LineCheck(n)) => Err(format!("✗ опечатка в строке {n:02}")),
        Err(base32::PaperError::LineNumber(n)) => Err(format!("✗ пропущена строка {n:02}")),
        Err(base32::PaperError::BlobCheck) => Err("✗ построчно верно, но общая сумма не сошлась".into()),
        Err(e) => Err(format!("✗ не разобрано: {e:?}")),
    }
}

fn cmd_shell(dir: &PathBuf) -> Result<(), String> {
    let store = Store::load(dir)?;
    let mk = unlock(store.kdf)?;
    println!("сессия открыта. Команды: pw <сайт> | totp <м> | vault ... | ls | exit");
    loop {
        let line = term::read_line("svitok> ").map_err(|e| e.to_string())?;
        let toks: Vec<String> = line.split_whitespace().map(String::from).collect();
        let Some(cmd) = toks.first() else { continue };
        // Перечитываем store - вдруг add/bump правил файлы из другого окна.
        let store = Store::load(dir)?;
        let res = match cmd.as_str() {
            "exit" | "quit" | "q" => break,
            "ls" => cmd_ls(dir),
            "pw" => cmd_pw(&store, &mk.0, &toks[1..]),
            "totp" => cmd_totp(&store, &mk.0, &toks[1..]),
            "vault" => cmd_vault(&store, &mk.0, &toks[1..]),
            "add" => cmd_add(dir, &toks[1..]),
            "bump" => cmd_bump(dir, &toks[1..]),
            other => Err(format!("не знаю команду {other}")),
        };
        if let Err(e) = res {
            eprintln!("ошибка: {e}");
        }
    }
    println!("сессия закрыта, ключ стёрт из памяти");
    Ok(())
}
