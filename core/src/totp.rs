//! TOTP (RFC 6238) поверх HOTP (RFC 4226) на HMAC-SHA1. Именно этот вариант
//! реально принимают сайты: Google Authenticator всё равно молча игнорирует
//! любые другие algorithm/period.

use crate::sha1::hmac_sha1;

/// HOTP: код по секрету и счётчику.
pub fn hotp(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let digits = digits.min(9); // 10^10 не влезет в u32; сайты и так дают 6 или 8
    let mac = hmac_sha1(secret, &counter.to_be_bytes());
    // Динамическая обрезка (RFC 4226 §5.3): смещение берём из младшего ниббла.
    let off = (mac[19] & 0x0f) as usize;
    let code = ((mac[off] & 0x7f) as u32) << 24
        | (mac[off + 1] as u32) << 16
        | (mac[off + 2] as u32) << 8
        | (mac[off + 3] as u32);
    code % 10u32.pow(digits)
}

/// TOTP: код по секрету и Unix-времени (в секундах).
pub fn totp(secret: &[u8], unix_time: u64, period: u32, digits: u32) -> u32 {
    let period = period.max(1); // защита от деления на ноль
    hotp(secret, unix_time / period as u64, digits)
}

/// Сколько секунд текущий код ещё действителен.
pub fn seconds_left(unix_time: u64, period: u32) -> u32 {
    let period = period.max(1);
    period - (unix_time % period as u64) as u32
}

/// Декодер Base32 (RFC 4648, алфавит A-Z2-7): в таком виде сайты отдают
/// TOTP-секреты. Не путать с бумажным Crockford-Base32 «Свитка».
pub fn decode_rfc4648(s: &str) -> Option<alloc::vec::Vec<u8>> {
    let mut out = alloc::vec::Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for ch in s.chars() {
        let c = ch.to_ascii_uppercase();
        if c == ' ' || c == '-' || c == '=' {
            continue;
        }
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            '2'..='7' => c as u32 - '2' as u32 + 26,
            _ => return None,
        };
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_sha1_vectors() {
        // RFC 6238, приложение B: секрет "12345678901234567890", 8 цифр.
        let secret = b"12345678901234567890";
        assert_eq!(totp(secret, 59, 30, 8), 94287082);
        assert_eq!(totp(secret, 1111111109, 30, 8), 7081804);
        assert_eq!(totp(secret, 1234567890, 30, 8), 89005924);
        assert_eq!(totp(secret, 20000000000, 30, 8), 65353130);
    }

    #[test]
    fn base32_decode() {
        // "JBSWY3DPEHPK3PXP" - ходовой пример секрета ("Hello!" плюс 0xDE 0xAD 0xBE 0xEF).
        let d = decode_rfc4648("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(d, b"Hello!\xde\xad\xbe\xef");
        assert!(decode_rfc4648("jbswy3dp ehpk3pxp").is_some());
        assert!(decode_rfc4648("1нет").is_none());
    }
}
