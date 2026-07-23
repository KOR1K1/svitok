//! Файлы «Свитка» на диске. Секретов здесь нет:
//!   sites.txt - список сайтов и политик (метаданные, копия бумажного списка)
//!   vault.b32 - шифрованный сейф в бумажном виде (те же строки, что на листке)

use std::path::{Path, PathBuf};
use svitok_core::derive::Policy;
use svitok_core::kdf::KdfParams;

#[derive(Clone, Debug)]
pub struct Site {
    /// Стабильный ключ записи для UI, правок и синхронизации. Метаданные:
    /// в деривацию не входит, менять его безопасно для паролей.
    pub id: String,
    /// Домен деривации. Заморожен: он входит в формулу пароля.
    pub name: String,
    pub login: String,
    pub counter: u32,
    pub policy: Policy,
    /// Дополнительные домены для матчинга автозаполнения (lolz.live к lolz.guru).
    /// Только матчинг, в деривацию не входят - пароль всегда выводится из name.
    pub aliases: Vec<String>,
    /// Отображаемое имя. Пустое - показываем name.
    pub label: String,
}

pub struct Store {
    pub dir: PathBuf,
    pub kdf: KdfParams,
    pub sites: Vec<Site>,
    /// Контрольная метка мастер-ключа (hex): по ней проверяем фразу при входе.
    /// Не секрет - без сида (он в Keystore под биометрией) её не подобрать.
    pub check: Option<String>,
}

impl Store {
    pub fn sites_path(dir: &Path) -> PathBuf {
        dir.join("sites.txt")
    }
    pub fn vault_path(dir: &Path) -> PathBuf {
        dir.join("vault.b32")
    }
    pub fn exists(dir: &Path) -> bool {
        Self::sites_path(dir).exists()
    }
    pub fn empty(dir: &Path) -> Store {
        Store { dir: dir.to_path_buf(), kdf: KdfParams::DEFAULT, sites: Vec::new(), check: None }
    }

    pub fn load(dir: &Path) -> Result<Store, String> {
        let path = Self::sites_path(dir);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("не могу прочитать {}: {e}", path.display()))?;
        let mut kdf = KdfParams::DEFAULT;
        let mut check = None;
        let mut sites = Vec::new();
        for (ln, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('#') {
                // "# kdf M17 T21"  или  "# check <hex>"
                let toks: Vec<&str> = rest.split_whitespace().collect();
                if toks.first() == Some(&"kdf") && toks.len() == 3 {
                    let m = toks[1].strip_prefix('M').and_then(|s| s.parse().ok());
                    let t = toks[2].strip_prefix('T').and_then(|s| s.parse().ok());
                    if let (Some(m), Some(t)) = (m, t) {
                        kdf = KdfParams::parse(m, t)
                            .ok_or_else(|| format!("sites.txt:{}: недопустимые параметры KDF", ln + 1))?;
                    }
                } else if toks.first() == Some(&"check") && toks.len() == 2 {
                    check = Some(toks[1].to_string());
                }
                continue;
            }
            let site = Site::from_line(line).map_err(|e| format!("sites.txt:{}: {e}", ln + 1))?;
            sites.push(site);
        }
        let mut store = Store { dir: dir.to_path_buf(), kdf, sites, check };
        // Списки из старых версий (и записанные с листка руками) идут без id.
        // Раздаём id прямо при загрузке и сразу сохраняем: команды перечитывают
        // файл на каждый вызов, и не записанный на диск id жил бы один вызов.
        // Не сохранилось (файловая система только на чтение) - работаем как есть.
        if store.assign_missing_ids() {
            let _ = store.save();
        }
        Ok(store)
    }

    /// Новый id, которого ещё нет в списке: 4 случайных байта в hex.
    pub fn new_id(&self) -> Result<String, String> {
        loop {
            let mut raw = [0u8; 4];
            crate::osrng::os_random(&mut raw).map_err(|e| e.to_string())?;
            let id: String = raw.iter().map(|b| format!("{b:02x}")).collect();
            if !self.sites.iter().any(|s| s.id == id) {
                return Ok(id);
            }
        }
    }

    fn assign_missing_ids(&mut self) -> bool {
        let mut changed = false;
        for i in 0..self.sites.len() {
            if self.sites[i].id.is_empty() {
                match self.new_id() {
                    Ok(id) => {
                        self.sites[i].id = id;
                        changed = true;
                    }
                    Err(_) => break, // без ГСЧ оставляем как есть - добавление сайта откажет громко
                }
            }
        }
        changed
    }

    pub fn save(&self) -> Result<(), String> {
        let mut out = String::from("# svitok v1 — список сайтов (метаданные, не секрет)\n");
        out.push_str(&format!("# kdf {}\n", self.kdf.to_paper()));
        if let Some(c) = &self.check {
            out.push_str(&format!("# check {}\n", c));
        }
        for s in &self.sites {
            out.push_str(&s.to_line());
            out.push('\n');
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        atomic_write(&Self::sites_path(&self.dir), out.as_bytes())
    }

    pub fn find<'a>(&'a self, query: &str) -> Vec<&'a Site> {
        let q = query.to_lowercase();
        let exact: Vec<&Site> = self.sites.iter().filter(|s| s.name.to_lowercase() == q).collect();
        if !exact.is_empty() {
            return exact;
        }
        self.sites
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&q))
            .collect()
    }

    pub fn read_vault_blob(&self) -> Result<Option<Vec<u8>>, String> {
        let path = Self::vault_path(&self.dir);
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let lines: Vec<&str> = text.lines().collect();
        svitok_core::base32::from_paper(&lines)
            .map(Some)
            .map_err(|e| format!("vault.b32 повреждён: {e:?}"))
    }

    /// Записывает сейф в бумажном виде и отдаёт те же строки, чтобы их показать.
    pub fn write_vault_blob(&self, blob: &[u8]) -> Result<Vec<String>, String> {
        let lines = svitok_core::base32::to_paper(blob);
        let mut text = String::from("# svitok v1 — шифрованный сейф (копия бумажной записи)\n");
        for l in &lines {
            text.push_str(l);
            text.push('\n');
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        atomic_write(&Self::vault_path(&self.dir), text.as_bytes())?;
        Ok(lines)
    }
}

static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Пишем через временный файл рядом и rename поверх. Обрыв питания или падение
/// в середине оставит нетронутым старый файл, а не обрезанный - иначе можно
/// потерять сейф или строку # check, и вход перестанет проверять фразу.
///
/// Имя tmp уникально (pid + счётчик), чтобы две одновременные записи не топтали
/// один файл; create_new (O_EXCL) не даёт пойти по подложенному симлинку;
/// на unix ставим 0600 и досинхиваем каталог, чтобы rename пережил обрыв питания.
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let pid = std::process::id();
    let n = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("store");
    let tmp = path.with_file_name(format!(".{fname}.{pid}.{n}.tmp"));

    let res = (|| -> Result<(), String> {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
        f.write_all(contents).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(d) = std::fs::File::open(parent) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

impl Site {
    /// Разбирает строку списка без ведущего «#»: «имя login=… v=… len=… cls=…
    /// sym=… alias=… label=… id=…». Умолчания: login пустой, v=1, len=DEFAULT,
    /// cls=luds. Имя - первый токен, без пробелов. alias - домены через запятую,
    /// label - с пробелами в виде %20, id может отсутствовать (старые списки).
    pub fn from_line(line: &str) -> Result<Site, String> {
        let mut toks = line.split_whitespace();
        let name = toks.next().ok_or("пустая строка сайта")?.to_string();
        let mut id = String::new();
        let mut login = String::new();
        let mut counter = 1u32;
        let mut len = Policy::DEFAULT_LEN;
        let mut cls = "luds".to_string();
        let mut sym: Option<String> = None;
        let mut aliases: Vec<String> = Vec::new();
        let mut label = String::new();
        for t in toks {
            if let Some(v) = t.strip_prefix("login=") {
                login = v.to_string();
            } else if let Some(v) = t.strip_prefix("v=") {
                counter = v.parse().map_err(|_| format!("плохой счётчик {t}"))?;
            } else if let Some(v) = t.strip_prefix("len=") {
                len = v.parse().map_err(|_| format!("плохая длина {t}"))?;
            } else if let Some(v) = t.strip_prefix("cls=") {
                cls = v.to_string();
            } else if let Some(v) = t.strip_prefix("sym=") {
                sym = Some(v.to_string());
            } else if let Some(v) = t.strip_prefix("alias=") {
                aliases = v.split(',').filter(|a| !a.is_empty()).map(str::to_string).collect();
            } else if let Some(v) = t.strip_prefix("label=") {
                label = decode_label(v);
            } else if let Some(v) = t.strip_prefix("id=") {
                id = v.to_string();
            } else {
                return Err(format!("непонятный параметр {t}"));
            }
        }
        let policy =
            Policy::from_classes(len, &cls, sym.as_deref()).ok_or("недопустимая политика")?;
        Ok(Site { id, name, login, counter, policy, aliases, label })
    }

    pub fn to_line(&self) -> String {
        let mut s = self.name.clone();
        if !self.login.is_empty() {
            s.push_str(&format!(" login={}", self.login));
        }
        if self.counter != 1 {
            s.push_str(&format!(" v={}", self.counter));
        }
        if self.policy.length != Policy::DEFAULT_LEN {
            s.push_str(&format!(" len={}", self.policy.length));
        }
        let mut cls = String::new();
        if self.policy.lower {
            cls.push('l');
        }
        if self.policy.upper {
            cls.push('u');
        }
        if self.policy.digits {
            cls.push('d');
        }
        if self.policy.symbols {
            cls.push('s');
        }
        if cls != "luds" {
            s.push_str(&format!(" cls={cls}"));
        }
        if let Some(cs) = &self.policy.custom_symbols {
            s.push_str(&format!(" sym={}", String::from_utf8_lossy(cs)));
        }
        if !self.aliases.is_empty() {
            s.push_str(&format!(" alias={}", self.aliases.join(",")));
        }
        if !self.label.is_empty() {
            s.push_str(&format!(" label={}", encode_label(&self.label)));
        }
        if !self.id.is_empty() {
            s.push_str(&format!(" id={}", self.id));
        }
        s
    }

    /// Все домены, по которым запись матчится: name плюс aliases.
    pub fn domains(&self) -> impl Iterator<Item = &str> {
        core::iter::once(self.name.as_str()).chain(self.aliases.iter().map(String::as_str))
    }

    /// Матчится ли запись на канонический (registrable) домен страницы.
    pub fn matches_domain(&self, canon: &str) -> bool {
        self.domains()
            .any(|d| svitok_core::domain::canonical(d).as_deref() == Some(canon))
    }
}

/// Строка списка режется по пробелам, поэтому пробелы внутри label кодируем:
/// «%» -> %25, пробельный символ -> %20. Остальное как есть - читаемо на бумаге.
fn encode_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '%' => out.push_str("%25"),
            c if c.is_whitespace() => out.push_str("%20"),
            c => out.push(c),
        }
    }
    out
}

fn decode_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if rest.starts_with("%20") {
            out.push(' ');
            rest = &rest[3..];
        } else if rest.starts_with("%25") {
            out.push('%');
            rest = &rest[3..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(line: &str) {
        let site = Site::from_line(line).expect("parse");
        assert_eq!(site.to_line(), line, "формат строки сайта не совпал");
    }

    #[test]
    fn site_line_roundtrip() {
        // На таком round-trip держится синхронизация списка по QR.
        roundtrip("mega.nz");
        roundtrip("github.com login=me@example.org");
        roundtrip("bank login=user v=3 len=16 cls=lud");
        roundtrip("pin len=6 cls=d");
        // cls=luds - умолчание, в каноничном виде опускаем; символы задаёт sym=.
        roundtrip("wifi sym=!@#$%");
        roundtrip("lolz.guru login=me alias=lolz.live,zelenka.guru id=a1b2c3d4");
        roundtrip("gmail.com login=work@gmail.com label=Рабочая%20почта id=00ff00ff");
    }

    #[test]
    fn defaults_omitted() {
        let s = Site::from_line("site login=x").unwrap();
        assert_eq!(s.counter, 1);
        assert!(!s.to_line().contains("v="));
        assert!(!s.to_line().contains("cls="));
        // строка без новых токенов - как из старых версий - обходится без них
        assert!(s.id.is_empty() && s.aliases.is_empty() && s.label.is_empty());
    }

    #[test]
    fn label_percent_coding() {
        assert_eq!(encode_label("две части"), "две%20части");
        assert_eq!(decode_label("две%20части"), "две части");
        // литеральный процент переживает round-trip
        assert_eq!(decode_label(&encode_label("скидка 100%20")), "скидка 100%20");
    }

    #[test]
    fn load_assigns_ids_to_legacy_lists() {
        let dir = std::env::temp_dir().join(format!("svitok-idmig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(Store::sites_path(&dir), "# kdf M17 T21\nmega.nz\ngithub.com login=me\n").unwrap();

        let store = Store::load(&dir).expect("load");
        assert!(store.sites.iter().all(|s| s.id.len() == 8), "id раздаются при загрузке");
        assert_ne!(store.sites[0].id, store.sites[1].id);

        // id записались на диск: повторная загрузка видит те же самые
        let again = Store::load(&dir).expect("reload");
        assert_eq!(again.sites[0].id, store.sites[0].id);
        assert_eq!(again.sites[1].id, store.sites[1].id);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn alias_matching() {
        let s = Site::from_line("lolz.guru alias=lolz.live,lzt.market").unwrap();
        assert!(s.matches_domain("lolz.guru"));
        assert!(s.matches_domain("lolz.live"));
        assert!(s.matches_domain("lzt.market"));
        assert!(!s.matches_domain("lolz.market"));
        // канонизация: поддомен алиаса сводится к тому же registrable domain
        assert_eq!(svitok_core::domain::canonical("forum.lolz.live").as_deref(), Some("lolz.live"));
    }
}
