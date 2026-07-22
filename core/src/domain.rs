//! Свести адрес сайта к registrable domain (eTLD+1) по встроенному PSL и
//! сравнить два адреса для автозаполнения: запись `vk.com` матчит и
//! `oauth.vk.com`, и `https://vk.com/login`.
//!
//! Это чисто про «какую запись показать/подставить для текущей вкладки».
//! Пароль от матчинга не зависит - он всегда выводится из канонического `site`,
//! записанного в списке. Матчим строго до registrable domain: `vk.com.evil.com`
//! сведётся к `evil.com` и с `vk.com` не совпадёт.

use crate::psl_data::{EXCEPTIONS, RULES, WILDCARDS};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

fn contains(set: &[&str], key: &str) -> bool {
    set.binary_search_by(|probe| probe.cmp(&key)).is_ok()
}

fn join(labels: &[&str]) -> String {
    labels.join(".")
}

/// Сколько лейблов занимает публичный суффикс (eTLD) для этого хоста.
fn public_suffix_len(labels: &[&str]) -> usize {
    let n = labels.len();
    // исключения (!rule) имеют приоритет над всем: публичный суффикс - это
    // правило без крайнего левого лейбла
    for i in 0..n {
        if contains(EXCEPTIONS, &join(&labels[i..])) {
            return n - i - 1;
        }
    }
    // самое длинное совпавшее правило. Идём от самого длинного суффикса (i=0),
    // первое совпадение и есть самое длинное.
    for i in 0..n {
        let suffix = join(&labels[i..]);
        if contains(RULES, &suffix) {
            return n - i;
        }
        // wildcard `*.X` матчит labels[i] + X, где X = labels[i+1..]
        if i + 1 < n && contains(WILDCARDS, &join(&labels[i + 1..])) {
            return n - i;
        }
    }
    // дефолтное правило "*": неизвестный TLD - публичный суффикс из одного лейбла
    1
}

/// Registrable domain (eTLD+1) для голого хоста (без схемы/порта/пути).
/// `None`, если хост пуст, это сам публичный суффикс или IP.
fn registrable(host: &str) -> Option<String> {
    let host = host.trim().trim_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.parse::<core::net::IpAddr>().is_ok() {
        return None;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.iter().any(|l| l.is_empty()) {
        return None; // пустой лейбл (двойная точка и т.п.)
    }
    let n = labels.len();
    let suffix = public_suffix_len(&labels);
    // registrable = публичный суффикс плюс один лейбл слева
    let take = suffix + 1;
    if take > n {
        return None; // хост сам является публичным суффиксом - регистрировать нечего
    }
    Some(labels[n - take..].join("."))
}

/// Вытащить хост из того, что мог записать пользователь: `vk.com`,
/// `https://vk.com/login`, `oauth.vk.com`, `user@host:443`.
fn extract_host(input: &str) -> Option<String> {
    let s = input.trim();
    // отбросить схему
    let s = match s.find("://") {
        Some(i) => &s[i + 3..],
        None => s,
    };
    // отрезать всё от начала пути/запроса/фрагмента - остаётся authority
    let end = s.find(['/', '?', '#']).unwrap_or(s.len());
    let s = &s[..end];
    // отбросить userinfo
    let s = match s.find('@') {
        Some(i) => &s[i + 1..],
        None => s,
    };
    // отбросить порт (для IPv6 в скобках оставим как есть - registrable его отсеет)
    let s = if s.starts_with('[') {
        s
    } else {
        match s.rfind(':') {
            Some(i) => &s[..i],
            None => s,
        }
    };
    let host = s.trim().trim_matches('.');
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Канонический ключ сайта: registrable domain для того, что ввёл пользователь.
pub fn canonical(input: &str) -> Option<String> {
    registrable(&extract_host(input)?)
}

/// Матчит ли адрес страницы сохранённую запись. Оба сводятся к registrable
/// domain и сравниваются. Если хоть один не сводится - не матч.
pub fn matches(saved_site: &str, page_url: &str) -> bool {
    match (canonical(saved_site), canonical(page_url)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_basics() {
        assert_eq!(canonical("vk.com").as_deref(), Some("vk.com"));
        assert_eq!(canonical("https://vk.com/login").as_deref(), Some("vk.com"));
        assert_eq!(canonical("oauth.vk.com").as_deref(), Some("vk.com"));
        assert_eq!(canonical("HTTPS://Login.VK.com:443/x?y#z").as_deref(), Some("vk.com"));
        assert_eq!(canonical("user@mail.vk.com").as_deref(), Some("vk.com"));
    }

    #[test]
    fn multi_level_tlds() {
        assert_eq!(canonical("foo.co.uk").as_deref(), Some("foo.co.uk"));
        assert_eq!(canonical("a.b.foo.co.uk").as_deref(), Some("foo.co.uk"));
        assert_eq!(canonical("bbc.co.uk").as_deref(), Some("bbc.co.uk"));
        assert_eq!(canonical("shop.example.com.au").as_deref(), Some("example.com.au"));
    }

    #[test]
    fn matching() {
        assert!(matches("vk.com", "https://oauth.vk.com/authorize"));
        assert!(matches("https://vk.com", "login.vk.com"));
        assert!(matches("github.com", "https://gist.github.com"));
        // фишинг: vk.com.evil.com сводится к evil.com, не матч
        assert!(!matches("vk.com", "https://vk.com.evil.com"));
        assert!(!matches("vk.com", "vkontakte.com"));
        assert!(!matches("google.com", "google.co.uk"));
    }

    #[test]
    fn rejects_non_domains() {
        assert_eq!(canonical(""), None);
        assert_eq!(canonical("com"), None); // сам публичный суффикс
        assert_eq!(canonical("co.uk"), None);
        assert_eq!(canonical("127.0.0.1"), None); // IPv4
        assert_eq!(canonical("localhost"), None); // один лейбл, дефолтное правило "*" -> нет eTLD+1
    }

    #[test]
    fn wildcard_and_exception() {
        // *.ck: всё под ck - публичный суффикс, кроме исключения www.ck
        assert_eq!(canonical("a.b.ck").as_deref(), Some("a.b.ck"));
        assert_eq!(canonical("www.ck").as_deref(), Some("www.ck")); // !www.ck -> registrable
    }
}
