//! Импорт списка сайтов из экспортов других менеджеров: CSV (Chrome, Bitwarden,
//! LastPass, KeePassXC и вообще любой с узнаваемыми колонками) и JSON Bitwarden.
//!
//! Берём только метаданные - домен и логин. Пароли из файла в записи не
//! попадают принципиально: у Свитка пароль выводится из сида и фразы, а не
//! хранится. Поэтому чужие пароли здесь - мусор, который мы стараемся ещё и
//! затереть (CSV-поля занулям после разбора; для JSON затирается исходный
//! текст у вызывающего, распарсенное дерево - остаточный риск кучи serde).

use svitok_core::domain::canonical;

pub struct ImportedSite {
    pub name: String,
    pub login: String,
}

pub struct Parsed {
    pub sites: Vec<ImportedSite>,
    /// Строки, из которых не вышло достать ни домена, ни имени.
    pub skipped: usize,
}

pub fn parse(text: &str) -> Result<Parsed, String> {
    let t = text.trim_start_matches('\u{feff}').trim();
    if t.is_empty() {
        return Err("файл пуст".into());
    }
    if t.starts_with('{') || t.starts_with('[') {
        parse_bitwarden_json(t)
    } else {
        parse_csv(t)
    }
}

/// Имя записи из URL или, если он не сводится к домену, из заголовка:
/// «My WiFi» -> «my-wifi» (в строке списка пробелам нельзя).
fn make_name(url: &str, title: &str) -> Option<String> {
    if let Some(c) = canonical(url) {
        return Some(c);
    }
    if let Some(c) = canonical(title) {
        return Some(c); // заголовком часто пишут сам домен
    }
    let slug = title.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("-");
    if slug.is_empty() {
        None
    } else {
        Some(slug)
    }
}

fn push_site(out: &mut Vec<ImportedSite>, seen: &mut std::collections::HashSet<(String, String)>, name: String, login: String) {
    // логин без пробелов - иначе строка списка не соберётся
    let login: String = login.split_whitespace().collect();
    if seen.insert((name.clone(), login.clone())) {
        out.push(ImportedSite { name, login });
    }
}

// ---------- Bitwarden JSON ----------

/// Незашифрованный экспорт Bitwarden: {"items":[{"type":1,"name":…,
/// "login":{"username":…,"uris":[{"uri":…}]}}, …]}. type 1 - логины,
/// остальное (карты, заметки) пропускаем: Свитку из них брать нечего.
fn parse_bitwarden_json(text: &str) -> Result<Parsed, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "не разобрал JSON - это точно экспорт Bitwarden?".to_string())?;
    let items = v
        .get("items")
        .and_then(|x| x.as_array())
        .ok_or("в JSON нет items - нужен незашифрованный экспорт Bitwarden")?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut skipped = 0usize;
    for item in items {
        if item.get("type").and_then(|x| x.as_u64()) != Some(1) {
            continue; // не логин - не пропуск, просто не наш тип
        }
        let title = item.get("name").and_then(|x| x.as_str()).unwrap_or("");
        let login_obj = item.get("login");
        let user = login_obj
            .and_then(|l| l.get("username"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let uri = login_obj
            .and_then(|l| l.get("uris"))
            .and_then(|u| u.as_array())
            .and_then(|a| a.first())
            .and_then(|f| f.get("uri"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        match make_name(uri, title) {
            Some(name) => push_site(&mut out, &mut seen, name, user.to_string()),
            None => skipped += 1,
        }
    }
    if out.is_empty() && skipped == 0 {
        return Err("в экспорте нет логинов".into());
    }
    Ok(Parsed { sites: out, skipped })
}

// ---------- CSV ----------

/// Колонки ищем по заголовку, а не по позиции - так один разбор кроет Chrome
/// (name,url,username,…), Bitwarden (…,login_uri,login_username,…), LastPass
/// (url,username,…,name,…) и KeePassXC ("Title","Username","URL",…).
fn parse_csv(text: &str) -> Result<Parsed, String> {
    let mut records = csv_records(text);
    if records.len() < 2 {
        wipe_records(&mut records);
        return Err("в CSV нет данных под заголовком".into());
    }
    let header: Vec<String> = records[0].iter().map(|h| h.trim().to_lowercase()).collect();
    let col = |names: &[&str]| -> Option<usize> {
        names.iter().find_map(|n| header.iter().position(|h| h == n))
    };
    let url_col = col(&["url", "login_uri", "uri", "web site", "website"]);
    let user_col = col(&["username", "login_username", "user name", "login name", "user"]);
    let title_col = col(&["name", "title", "account"]);
    let type_col = col(&["type"]); // у Bitwarden CSV в одном файле и заметки
    if url_col.is_none() && title_col.is_none() {
        wipe_records(&mut records);
        return Err("не узнал колонки CSV: нужен заголовок с url/login_uri и username".into());
    }

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut skipped = 0usize;
    for rec in records.iter().skip(1) {
        let get = |i: Option<usize>| i.and_then(|i| rec.get(i)).map(String::as_str).unwrap_or("");
        if let Some(tc) = type_col {
            let ty = get(Some(tc));
            if !ty.is_empty() && ty != "login" {
                continue;
            }
        }
        match make_name(get(url_col), get(title_col)) {
            Some(name) => push_site(&mut out, &mut seen, name, get(user_col).to_string()),
            None => skipped += 1,
        }
    }
    // в записях лежат и чужие пароли - зануляем всё поле за полем
    wipe_records(&mut records);
    if out.is_empty() && skipped == 0 {
        return Err("в CSV нет строк с сайтами".into());
    }
    Ok(Parsed { sites: out, skipped })
}

fn wipe_records(records: &mut [Vec<String>]) {
    for rec in records {
        for f in rec {
            svitok_core::wipe::wipe_str(f);
        }
    }
}

/// RFC 4180: поле в кавычках может нести запятые, переводы строк и "" как кавычку.
fn csv_records(text: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut rec: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => in_quotes = true,
            ',' => rec.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                rec.push(std::mem::take(&mut field));
                if rec.len() > 1 || !rec[0].is_empty() {
                    records.push(std::mem::take(&mut rec));
                } else {
                    rec.clear();
                }
            }
            _ => field.push(c),
        }
    }
    rec.push(field);
    if rec.len() > 1 || !rec[0].is_empty() {
        records.push(rec);
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_csv() {
        let text = "name,url,username,password,note\n\
                    GitHub,https://github.com/login,me@x.org,hunter2,\n\
                    Мой WiFi,,admin,pass,заметка\n";
        let p = parse(text).unwrap();
        assert_eq!(p.sites.len(), 2);
        assert_eq!(p.sites[0].name, "github.com");
        assert_eq!(p.sites[0].login, "me@x.org");
        assert_eq!(p.sites[1].name, "мой-wifi");
        assert_eq!(p.skipped, 0);
    }

    #[test]
    fn bitwarden_csv_filters_notes() {
        let text = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                    ,,login,VK,,,0,https://vk.com,ivan,secret,\n\
                    ,,note,Просто заметка,текст,,0,,,,\n";
        let p = parse(text).unwrap();
        assert_eq!(p.sites.len(), 1);
        assert_eq!(p.sites[0].name, "vk.com");
        assert_eq!(p.sites[0].login, "ivan");
    }

    #[test]
    fn lastpass_csv() {
        let text = "url,username,password,totp,extra,name,grouping,fav\n\
                    https://accounts.google.com,me@gmail.com,pw,,,Google,,0\n";
        let p = parse(text).unwrap();
        assert_eq!(p.sites[0].name, "google.com");
    }

    #[test]
    fn keepassxc_csv_quoted() {
        let text = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\"\n\
                    \"Root\",\"Say, hi\",\"user\",\"p,w\"\"x\",\"https://example.org\",\"multi\nline\"\n";
        let p = parse(text).unwrap();
        assert_eq!(p.sites.len(), 1);
        assert_eq!(p.sites[0].name, "example.org");
        assert_eq!(p.sites[0].login, "user");
    }

    #[test]
    fn bitwarden_json() {
        let text = r#"{"items":[
            {"type":1,"name":"GitHub","login":{"username":"me","uris":[{"uri":"https://github.com"}]}},
            {"type":1,"name":"gitlab.com","login":{"username":"me","uris":[]}},
            {"type":2,"name":"Secure note"}
        ]}"#;
        let p = parse(text).unwrap();
        assert_eq!(p.sites.len(), 2);
        assert_eq!(p.sites[0].name, "github.com");
        assert_eq!(p.sites[1].name, "gitlab.com"); // домен из заголовка
    }

    #[test]
    fn dedupes_and_counts_skips() {
        let text = "name,url,username,password\n\
                    A,https://a.com,me,x\n\
                    A copy,https://www.a.com,me,y\n\
                    ,,user-without-anything,z\n";
        let p = parse(text).unwrap();
        assert_eq!(p.sites.len(), 1, "тот же домен и логин - одна запись");
        assert_eq!(p.skipped, 1);
    }

    #[test]
    fn multiple_accounts_survive() {
        let text = "name,url,username,password\n\
                    Gmail,https://gmail.com,work@gmail.com,x\n\
                    Gmail,https://gmail.com,home@gmail.com,y\n";
        let p = parse(text).unwrap();
        assert_eq!(p.sites.len(), 2, "разные логины на одном домене - две записи");
    }
}
