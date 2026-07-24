//! Мини-сейф. Сюда кладём то, что формулой не выведешь: TOTP-секреты,
//! чужие пароли, recovery-коды, заметки. Всё это шифруется в один компактный блоб.
//!
//! Формат открытого текста (числа - varint LEB128):
//!   ver(0x01) ‖ count ‖ записи
//!   запись = тег(u8) ‖ len(label) ‖ label ‖ полезная нагрузка
//!     1 пароль:   len ‖ bytes(utf8)
//!     2 TOTP:     flags(u8) ‖ len ‖ секрет (сырые байты, не Base32!)
//!                 [если бит 5: len ‖ login ‖ count ‖ (len ‖ domain)*]
//!                 flags: биты 0-1 алгоритм (0=SHA1), бит 2: 8 цифр, биты 3-4 период (0=30с,1=60с,2=15с),
//!                 бит 5: есть привязка к аккаунту (login + домены для автозаполнения кода).
//!                 Старые записи бит 5 не ставят и читаются как прежде; запись без привязки
//!                 сериализуется байт-в-байт как в v1, так что бумажные векторы целы.
//!     3 коды:     enc(u8: 0=utf8, 1=BCD-цифры) ‖ len ‖ данные
//!                 (коды склеены '\n'; BCD: цифра в ниббл, 0xA=разделитель, 0xF=паддинг)
//!     4 заметка:  len ‖ bytes(utf8)
//!
//! Конверт: nonce(12) ‖ ciphertext(ChaCha20) ‖ mac(8)
//!   enc_key = subkey(mk, "vault-enc"), mac_key = subkey(mk, "vault-mac")
//!   mac = B2S(key=mac_key, nonce ‖ ct)[0..8]. Сначала шифруем, потом считаем MAC.

use crate::blake2s::b2s;
use crate::chacha20::xor_stream;
use crate::kdf::subkey;
use crate::wipe::{ct_eq, wipe, wipe_str};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Password { label: String, secret: Vec<u8> },
    Totp {
        label: String,
        secret: Vec<u8>,
        digits8: bool,
        period: u32,
        /// Аккаунт, к которому привязан код (для автозаполнения). Пусто - не привязан.
        /// Метаданные матчинга, в генерацию кода не входят; логин не секрет (он и в
        /// списке сайтов лежит открытым).
        login: String,
        /// Домены (name + алиасы аккаунта), на которых код предлагается автозаполнением.
        /// Снимок, а не ссылка: переживает удаление парольной записи. Пусто - не привязан.
        domains: Vec<String>,
    },
    Codes { label: String, codes: Vec<String> },
    Note { label: String, text: String },
}

// Расшифрованные записи держат в открытом виде TOTP-секреты, чужие пароли,
// recovery-коды и заметки. По умолчанию Vec/String просто освободили бы память,
// оставив эти байты в куче до переиспользования аллокатором - затираем сами.
impl Drop for Entry {
    fn drop(&mut self) {
        match self {
            Entry::Password { label, secret } => {
                wipe_str(label);
                wipe(secret);
            }
            Entry::Totp { label, secret, .. } => {
                wipe_str(label);
                wipe(secret);
            }
            Entry::Codes { label, codes } => {
                wipe_str(label);
                for c in codes {
                    wipe_str(c);
                }
            }
            Entry::Note { label, text } => {
                wipe_str(label);
                wipe_str(text);
            }
        }
    }
}

impl Entry {
    pub fn label(&self) -> &str {
        match self {
            Entry::Password { label, .. }
            | Entry::Totp { label, .. }
            | Entry::Codes { label, .. }
            | Entry::Note { label, .. } => label,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Entry::Password { .. } => "pw",
            Entry::Totp { .. } => "totp",
            Entry::Codes { .. } => "codes",
            Entry::Note { .. } => "note",
        }
    }
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn get_varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v: u64 = 0;
    let mut shift = 0u32;
    loop {
        let b = *data.get(*pos)?;
        *pos += 1;
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
        if shift > 56 {
            return None;
        }
    }
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    put_varint(out, b.len() as u64);
    out.extend_from_slice(b);
}

fn get_bytes<'a>(data: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    // try_from + checked_add: на 32-битных длина из varint могла бы усечься
    // или дать переполнение при сложении, поэтому режем это на корню
    let len = usize::try_from(get_varint(data, pos)?).ok()?;
    let end = pos.checked_add(len)?;
    let s = data.get(*pos..end)?;
    *pos = end;
    Some(s)
}

/// Упаковка чисто цифровых кодов в BCD: по две цифры на байт.
fn pack_bcd(codes: &[String]) -> Option<Vec<u8>> {
    let mut nibbles: Vec<u8> = Vec::new();
    for (i, c) in codes.iter().enumerate() {
        if i > 0 {
            nibbles.push(0xA);
        }
        for ch in c.chars() {
            let d = ch.to_digit(10)?;
            nibbles.push(d as u8);
        }
    }
    if nibbles.len() % 2 == 1 {
        nibbles.push(0xF);
    }
    let out: Vec<u8> = nibbles.chunks(2).map(|p| (p[0] << 4) | p[1]).collect();
    wipe(&mut nibbles); // ниблы - это цифры recovery-кодов
    Some(out)
}

fn unpack_bcd(data: &[u8]) -> Option<Vec<String>> {
    let mut codes = Vec::new();
    let mut cur = String::new();
    for byte in data {
        for nib in [byte >> 4, byte & 0xF] {
            match nib {
                0..=9 => cur.push(char::from(b'0' + nib)),
                0xA => {
                    codes.push(core::mem::take(&mut cur));
                }
                0xF => {}
                _ => return None,
            }
        }
    }
    codes.push(cur);
    Some(codes)
}

pub fn serialize(entries: &[Entry]) -> Vec<u8> {
    // Резервируем с запасом, чтобы буфер не реаллоцировался в процессе: иначе
    // старые блоки с уже записанным открытым текстом освобождались бы без затирания.
    // BCD-упаковка кодов меньше этой оценки, так что это верхняя граница.
    let cap = 8 + entries
        .iter()
        .map(|e| {
            8 + e.label().len()
                + match e {
                    Entry::Password { secret, .. } | Entry::Totp { secret, .. } => secret.len() + 8,
                    Entry::Codes { codes, .. } => codes.iter().map(|c| c.len() + 1).sum::<usize>() + 8,
                    Entry::Note { text, .. } => text.len() + 8,
                }
        })
        .sum::<usize>();
    let mut out = Vec::with_capacity(cap);
    out.push(0x01);
    put_varint(&mut out, entries.len() as u64);
    for e in entries {
        match e {
            Entry::Password { label, secret } => {
                out.push(1);
                put_bytes(&mut out, label.as_bytes());
                put_bytes(&mut out, secret);
            }
            Entry::Totp { label, secret, digits8, period, login, domains } => {
                out.push(2);
                put_bytes(&mut out, label.as_bytes());
                let pbits: u8 = match period {
                    60 => 1,
                    15 => 2,
                    _ => 0,
                };
                let bound = !login.is_empty() || !domains.is_empty();
                let flags: u8 = ((*digits8 as u8) << 2) | (pbits << 3) | ((bound as u8) << 5);
                out.push(flags);
                put_bytes(&mut out, secret);
                // без привязки байты те же, что в v1 - бумажные векторы не трогаем
                if bound {
                    put_bytes(&mut out, login.as_bytes());
                    put_varint(&mut out, domains.len() as u64);
                    for d in domains {
                        put_bytes(&mut out, d.as_bytes());
                    }
                }
            }
            Entry::Codes { label, codes } => {
                out.push(3);
                put_bytes(&mut out, label.as_bytes());
                if let Some(bcd) = pack_bcd(codes) {
                    out.push(1);
                    put_bytes(&mut out, &bcd);
                } else {
                    out.push(0);
                    let mut joined = codes.join("\n");
                    put_bytes(&mut out, joined.as_bytes());
                    wipe_str(&mut joined); // склейка кодов - это секрет
                }
            }
            Entry::Note { label, text } => {
                out.push(4);
                put_bytes(&mut out, label.as_bytes());
                put_bytes(&mut out, text.as_bytes());
            }
        }
    }
    out
}

pub fn deserialize(data: &[u8]) -> Option<Vec<Entry>> {
    let mut pos = 0usize;
    if *data.first()? != 0x01 {
        return None;
    }
    pos += 1;
    let count = get_varint(data, &mut pos)? as usize;
    if count > 10_000 {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = *data.get(pos)?;
        pos += 1;
        let label = String::from_utf8(get_bytes(data, &mut pos)?.to_vec()).ok()?;
        let e = match tag {
            1 => Entry::Password { label, secret: get_bytes(data, &mut pos)?.to_vec() },
            2 => {
                let flags = *data.get(pos)?;
                pos += 1;
                if flags & 0b11 != 0 {
                    return None; // в v1 только SHA-1
                }
                let period = match (flags >> 3) & 0b11 {
                    1 => 60,
                    2 => 15,
                    _ => 30,
                };
                let secret = get_bytes(data, &mut pos)?.to_vec();
                let (login, domains) = if flags & 0b10_0000 != 0 {
                    let login = String::from_utf8(get_bytes(data, &mut pos)?.to_vec()).ok()?;
                    let n = usize::try_from(get_varint(data, &mut pos)?).ok()?;
                    if n > 64 {
                        return None; // разумный потолок доменов на запись
                    }
                    let mut domains = Vec::with_capacity(n);
                    for _ in 0..n {
                        domains.push(String::from_utf8(get_bytes(data, &mut pos)?.to_vec()).ok()?);
                    }
                    (login, domains)
                } else {
                    (String::new(), Vec::new())
                };
                Entry::Totp {
                    label,
                    secret,
                    digits8: flags & 0b100 != 0,
                    period,
                    login,
                    domains,
                }
            }
            3 => {
                let enc = *data.get(pos)?;
                pos += 1;
                let raw = get_bytes(data, &mut pos)?;
                let codes = match enc {
                    0 => {
                        let s = core::str::from_utf8(raw).ok()?;
                        s.split('\n').map(String::from).collect()
                    }
                    1 => unpack_bcd(raw)?,
                    _ => return None,
                };
                Entry::Codes { label, codes }
            }
            4 => Entry::Note { label, text: String::from_utf8(get_bytes(data, &mut pos)?.to_vec()).ok()? },
            _ => return None,
        };
        entries.push(e);
    }
    if pos != data.len() {
        return None;
    }
    Some(entries)
}

pub const NONCE_LEN: usize = 12;
pub const MAC_LEN: usize = 8;

/// Шифрует сейф. `nonce` - 12 свежих случайных байт, их даёт вызывающий.
/// Детерминизм тут не нужен: при расшифровке nonce читается из конверта.
pub fn encrypt(mk: &[u8; 32], entries: &[Entry], nonce: [u8; NONCE_LEN]) -> Vec<u8> {
    let mut enc_key = subkey(mk, b"vault-enc");
    let mut mac_key = subkey(mk, b"vault-mac");
    let mut pt = serialize(entries);
    xor_stream(&enc_key, &nonce, 0, &mut pt);
    let mac = b2s(&mac_key, &[&nonce, &pt]);
    let mut out = Vec::with_capacity(NONCE_LEN + pt.len() + MAC_LEN);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&pt);
    out.extend_from_slice(&mac[..MAC_LEN]);
    wipe(&mut enc_key);
    wipe(&mut mac_key);
    out
}

#[derive(Debug, PartialEq, Eq)]
pub enum VaultError {
    TooShort,
    BadMac,
    BadFormat,
}

pub fn decrypt(mk: &[u8; 32], blob: &[u8]) -> Result<Vec<Entry>, VaultError> {
    if blob.len() < NONCE_LEN + MAC_LEN + 1 {
        return Err(VaultError::TooShort);
    }
    let (nonce, rest) = blob.split_at(NONCE_LEN);
    let (ct, mac) = rest.split_at(rest.len() - MAC_LEN);
    let mut mac_key = subkey(mk, b"vault-mac");
    let expected = b2s(&mac_key, &[nonce, ct]);
    wipe(&mut mac_key);
    if !ct_eq(&expected[..MAC_LEN], mac) {
        return Err(VaultError::BadMac);
    }
    let mut enc_key = subkey(mk, b"vault-enc");
    let mut pt = ct.to_vec();
    let nonce_arr: [u8; NONCE_LEN] = nonce.try_into().unwrap();
    xor_stream(&enc_key, &nonce_arr, 0, &mut pt);
    wipe(&mut enc_key);
    let entries = deserialize(&pt).ok_or(VaultError::BadFormat);
    wipe(&mut pt);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn sample() -> Vec<Entry> {
        vec![
            Entry::Password { label: "legacy-email".into(), secret: b"old-p@ss".to_vec() },
            Entry::Totp {
                label: "github".into(),
                secret: b"12345678901234567890".to_vec(),
                digits8: false,
                period: 30,
                login: String::new(),
                domains: Vec::new(),
            },
            Entry::Codes {
                label: "google-rec".into(),
                codes: vec!["12345678".into(), "87654321".into(), "11112222".into()],
            },
            Entry::Codes {
                label: "github-rec".into(),
                codes: vec!["abcde-12345".into(), "fghij-67890".into()],
            },
            Entry::Note { label: "wifi-дом".into(), text: "SSID=home pass=тайна123".into() },
        ]
    }

    #[test]
    fn serialize_roundtrip() {
        let e = sample();
        assert_eq!(deserialize(&serialize(&e)).unwrap(), e);
    }

    #[test]
    fn totp_binding_roundtrip() {
        let e = vec![Entry::Totp {
            label: "Discord".into(),
            secret: b"12345678901234567890".to_vec(),
            digits8: false,
            period: 30,
            login: "user@x".into(),
            domains: vec!["discord.com".into(), "discordapp.com".into()],
        }];
        assert_eq!(deserialize(&serialize(&e)).unwrap(), e);
    }

    #[test]
    fn unbound_totp_is_byte_identical_to_v1() {
        // запись без привязки должна сериализоваться точно как в v1 - иначе
        // золотые бумажные векторы (core/tests/golden.rs) поехали бы
        let unbound = vec![Entry::Totp {
            label: "gh".into(),
            secret: b"12345678901234567890".to_vec(),
            digits8: false,
            period: 30,
            login: String::new(),
            domains: Vec::new(),
        }];
        // v1-байты: ver, count=1, tag=2, len(2)+"gh", flags=0, len(20)+secret
        let mut expect = vec![0x01, 0x01, 0x02, 0x02];
        expect.extend_from_slice(b"gh");
        expect.push(0x00);
        expect.push(20);
        expect.extend_from_slice(b"12345678901234567890");
        assert_eq!(serialize(&unbound), expect);
    }

    #[test]
    fn bcd_saves_space() {
        // 3 кода по 8 цифр: 24 цифры плюс 2 разделителя, итого 13 байт против 26 в utf8.
        let codes: Vec<String> = vec!["12345678".into(), "87654321".into(), "11112222".into()];
        let packed = pack_bcd(&codes).unwrap();
        assert_eq!(packed.len(), 13);
        assert_eq!(unpack_bcd(&packed).unwrap(), codes);
    }

    #[test]
    fn bcd_odd_number_of_digits() {
        let codes: Vec<String> = vec!["123".into(), "45".into()];
        assert_eq!(unpack_bcd(&pack_bcd(&codes).unwrap()).unwrap(), codes);
    }

    #[test]
    fn encrypt_roundtrip() {
        let mk = [9u8; 32];
        let blob = encrypt(&mk, &sample(), [3u8; 12]);
        assert_eq!(decrypt(&mk, &blob).unwrap(), sample());
    }

    #[test]
    fn wrong_key_rejected() {
        let blob = encrypt(&[9u8; 32], &sample(), [3u8; 12]);
        assert_eq!(decrypt(&[8u8; 32], &blob), Err(VaultError::BadMac));
    }

    #[test]
    fn tamper_rejected() {
        let mk = [9u8; 32];
        let mut blob = encrypt(&mk, &sample(), [3u8; 12]);
        let mid = blob.len() / 2;
        blob[mid] ^= 1;
        assert_eq!(decrypt(&mk, &blob), Err(VaultError::BadMac));
    }

    #[test]
    fn compactness_50_entries() {
        // Прикидываем бумажный объём. Никто не держит 10 TOTP и 30 наборов кодов;
        // реалистичный сейф - 5 TOTP по 20 байт, 3 набора кодов, пара заметок.
        let mut entries = Vec::new();
        for i in 0..5 {
            entries.push(Entry::Totp {
                label: alloc::format!("site{i}"),
                secret: vec![0xAB; 20],
                digits8: false,
                period: 30,
                login: String::new(),
                domains: Vec::new(),
            });
        }
        for i in 0..3 {
            entries.push(Entry::Codes {
                label: alloc::format!("rec{i}"),
                codes: (0..10).map(|j| alloc::format!("{:08}", j * 11111111u64 % 99999999)).collect(),
            });
        }
        let blob = encrypt(&[1u8; 32], &entries, [0u8; 12]);
        let lines = crate::base32::to_paper(&blob);
        // Всё должно влезать на один лист, а это меньше 60 строк.
        assert!(lines.len() < 60, "{} строк", lines.len());
    }
}
