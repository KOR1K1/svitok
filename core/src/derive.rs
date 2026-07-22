//! Пароли выводятся детерминированно:
//!
//!   pk     = subkey(mk, "password")
//!   sk     = B2S(key=pk, "PW:" ‖ site ‖ 0x1F ‖ login ‖ 0x1F ‖ le32(counter))
//!   поток  = ChaCha20(key=sk, nonce=0)
//!   пароль = символы из разрешённого алфавита, взятые через rejection-sampling
//!            (иначе получили бы перекос от modulo). Потом добиваем недостающие
//!            обязательные классы, тоже детерминированно.
//!
//! Одинаковый набор (сид, фраза, сайт, логин, счётчик, политика) всегда даёт
//! один и тот же пароль - на любом устройстве и в любой момент.

use crate::blake2s::b2s;
use crate::chacha20::ByteStream;
use crate::kdf::subkey;
use alloc::string::String;
use alloc::vec::Vec;

/// Классы символов. Порядок и состав менять нельзя: они зашиты в алгоритм,
/// сдвинешь - и старые пароли перестанут воспроизводиться.
pub const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
pub const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const DIGITS: &[u8] = b"0123456789";
pub const SYMBOLS: &[u8] = b"!@#$%^&*()-_=+[]{};:,.?/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub length: usize,
    pub lower: bool,
    pub upper: bool,
    pub digits: bool,
    pub symbols: bool,
    /// Когда сайт принимает не все спецсимволы - здесь лежит разрешённый поднабор SYMBOLS.
    pub custom_symbols: Option<Vec<u8>>,
}

impl Policy {
    pub const DEFAULT_LEN: usize = 20;

    pub fn default_luds() -> Policy {
        Policy { length: Self::DEFAULT_LEN, lower: true, upper: true, digits: true, symbols: true, custom_symbols: None }
    }

    /// Читает строку вида "luds": l - строчные, u - заглавные, d - цифры, s - спецсимволы.
    pub fn from_classes(length: usize, cls: &str, custom_symbols: Option<&str>) -> Option<Policy> {
        if length == 0 || length > 128 {
            return None;
        }
        let mut p = Policy { length, lower: false, upper: false, digits: false, symbols: false, custom_symbols: None };
        for c in cls.chars() {
            match c {
                'l' => p.lower = true,
                'u' => p.upper = true,
                'd' => p.digits = true,
                's' => p.symbols = true,
                _ => return None,
            }
        }
        if !(p.lower || p.upper || p.digits || p.symbols) {
            return None;
        }
        if let Some(cs) = custom_symbols {
            let bytes: Vec<u8> = cs.bytes().collect();
            // повтор символа перекосил бы распределение, поэтому дубли не пускаем
            let has_dup = bytes.iter().enumerate().any(|(i, b)| bytes[..i].contains(b));
            if bytes.is_empty() || has_dup || !bytes.iter().all(|b| SYMBOLS.contains(b)) {
                return None;
            }
            p.symbols = true;
            p.custom_symbols = Some(bytes);
        }
        Some(p)
    }

    fn classes(&self) -> Vec<&[u8]> {
        let mut v: Vec<&[u8]> = Vec::new();
        if self.lower {
            v.push(LOWER);
        }
        if self.upper {
            v.push(UPPER);
        }
        if self.digits {
            v.push(DIGITS);
        }
        if self.symbols {
            let sym = self.custom_symbols.as_deref().unwrap_or(SYMBOLS);
            // пустой custom_symbols не должен попасть в набор: иначе в добивке
            // классов словим next_mod(0) - деление на ноль в release
            if !sym.is_empty() {
                v.push(sym);
            }
        }
        v
    }
}

/// Пароль для сайта. `site` и `login` должны быть точь-в-точь как в списке сайтов.
/// `None`, если политика негодная (пустой алфавит или длина вне 1..=128) - поля
/// Policy публичны, так что структуру можно собрать напрямую в обход from_classes.
pub fn site_password(mk: &[u8; 32], site: &str, login: &str, counter: u32, policy: &Policy) -> Option<String> {
    let classes = policy.classes();
    let mut alphabet: Vec<u8> = Vec::new();
    for c in &classes {
        alphabet.extend_from_slice(c);
    }
    if alphabet.is_empty() || policy.length == 0 || policy.length > 128 {
        return None;
    }

    let mut pw_key = subkey(mk, b"password");
    let sk = b2s(
        &pw_key,
        &[
            b"PW:",
            site.as_bytes(),
            &[0x1F],
            login.as_bytes(),
            &[0x1F],
            &counter.to_le_bytes(),
        ],
    );
    crate::wipe::wipe(&mut pw_key);
    let mut stream = ByteStream::new(sk);

    let mut pw: Vec<u8> = (0..policy.length)
        .map(|_| alphabet[stream.next_mod(alphabet.len())])
        .collect();

    // Добиваем обязательные классы. Если длина меньше их числа, все классы
    // впихнуть нельзя - берём сколько поместится.
    if policy.length >= classes.len() {
        // не больше 32 проходов; каждая подмена тянет свежие байты потока.
        // обычно хватает первого прохода.
        for _ in 0..32 {
            let missing: Vec<&[u8]> = classes
                .iter()
                .filter(|cl| !pw.iter().any(|ch| cl.contains(ch)))
                .copied()
                .collect();
            if missing.is_empty() {
                break;
            }
            for cl in missing {
                let pos = stream.next_mod(pw.len());
                pw[pos] = cl[stream.next_mod(cl.len())];
            }
        }
    }

    let s = String::from_utf8(pw).expect("алфавит - чистый ASCII");
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk() -> [u8; 32] {
        [7u8; 32]
    }

    #[test]
    fn deterministic() {
        let p = Policy::default_luds();
        let a = site_password(&mk(), "mega.nz", "me", 1, &p).unwrap();
        let b = site_password(&mk(), "mega.nz", "me", 1, &p).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 20);
    }

    #[test]
    fn inputs_change_password() {
        let p = Policy::default_luds();
        let base = site_password(&mk(), "mega.nz", "me", 1, &p).unwrap();
        assert_ne!(base, site_password(&mk(), "mega.nz", "me", 2, &p).unwrap());
        assert_ne!(base, site_password(&mk(), "mega.nz", "other", 1, &p).unwrap());
        assert_ne!(base, site_password(&mk(), "mega.n", "zme", 1, &p).unwrap()); // сдвиг границы site|login
        assert_ne!(base, site_password(&[8u8; 32], "mega.nz", "me", 1, &p).unwrap());
    }

    #[test]
    fn invalid_policy_returns_none() {
        // пустой custom-класс при symbols=true не должен паниковать
        let p = Policy { length: 20, lower: false, upper: false, digits: false, symbols: true, custom_symbols: Some(alloc::vec![]) };
        assert_eq!(site_password(&mk(), "x", "y", 1, &p), None);
        // длина вне диапазона
        let p2 = Policy { length: 0, ..Policy::default_luds() };
        assert_eq!(site_password(&mk(), "x", "y", 1, &p2), None);
    }

    #[test]
    fn required_classes_present() {
        let p = Policy::default_luds();
        for i in 0..50u32 {
            let pw = site_password(&mk(), "test.com", "u", i, &p).unwrap();
            let b = pw.as_bytes();
            assert!(b.iter().any(|c| LOWER.contains(c)), "{pw}");
            assert!(b.iter().any(|c| UPPER.contains(c)), "{pw}");
            assert!(b.iter().any(|c| DIGITS.contains(c)), "{pw}");
            assert!(b.iter().any(|c| SYMBOLS.contains(c)), "{pw}");
        }
    }

    #[test]
    fn digits_only_pin() {
        let p = Policy::from_classes(6, "d", None).unwrap();
        let pw = site_password(&mk(), "bank", "card", 1, &p).unwrap();
        assert_eq!(pw.len(), 6);
        assert!(pw.bytes().all(|c| DIGITS.contains(&c)));
    }

    #[test]
    fn custom_symbols_respected() {
        let p = Policy::from_classes(24, "lds", Some("._-")).unwrap();
        for i in 0..20u32 {
            let pw = site_password(&mk(), "x", "", i, &p).unwrap();
            for c in pw.bytes() {
                assert!(
                    LOWER.contains(&c) || DIGITS.contains(&c) || b"._-".contains(&c),
                    "недопустимый символ в {pw}"
                );
            }
            assert!(pw.bytes().any(|c| b"._-".contains(&c)));
        }
    }

    #[test]
    fn short_password_no_infinite_loop() {
        // Длина 2, а классов 4 - тут все классы и не обещаем, лишь бы не зациклиться.
        let p = Policy::from_classes(2, "luds", None).unwrap();
        let pw = site_password(&mk(), "s", "", 1, &p).unwrap();
        assert_eq!(pw.len(), 2);
    }
}
