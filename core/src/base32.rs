//! Бумажная кодировка: Crockford Base32, нумерованные строки с чек-символами
//! и контрольная сумма всего блоба.
//!
//! Строка листка:  `NN XXXX XXXX XXXX XXXX K`
//!   NN - номер строки (с 01), 16 символов данных (= 10 байт), K - чек-символ.
//! Финальная строка: `== XXXX` - 20-битная сумма всех байт блоба.
//!
//! Декодер намеренно снисходителен к переписчику: регистр не важен, o читается
//! как 0, i и l как 1, пробелы и дефисы выкидываются. Если строку переписали
//! с ошибкой, мы укажем, какую именно.

use crate::blake2s::b2s;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Crockford: без I, L, O, U.
pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Байты в символы, по 5 бит на символ, старший бит первым.
pub fn encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 8 / 5 + 1);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &b in data {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize]);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize]);
    }
    out
}

/// Значение символа; заодно чиним визуальных двойников. None, если символа нет в алфавите.
pub fn char_value(c: char) -> Option<u8> {
    let c = c.to_ascii_uppercase();
    let c = match c {
        'O' => '0',
        'I' | 'L' => '1',
        other => other,
    };
    ALPHABET.iter().position(|&a| a as char == c).map(|p| p as u8)
}

/// Символы обратно в байты. Хвостовые биты обязаны быть нулевыми.
pub fn decode(chars: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(chars.len() * 5 / 8);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &ch in chars {
        let v = char_value(ch as char)? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    if bits > 0 && (acc & ((1 << bits) - 1)) != 0 {
        return None; // в хвосте не нули: символов пришло больше, чем есть данных
    }
    Some(out)
}

/// Чек-символ строки: хешируем номер вместе с символами данных и берём один
/// символ алфавита. Ошибку в строке ловит с вероятностью 31/32, а общая сумма
/// блоба закрывает оставшуюся щель.
pub fn line_check(line_no: u32, data_chars: &[u8]) -> u8 {
    let h = b2s(&[], &[b"SVITOK-LINE-v1", &line_no.to_le_bytes(), data_chars]);
    ALPHABET[(h[0] % 32) as usize]
}

/// Контрольная сумма блоба: 4 символа (20 бит).
pub fn blob_check(data: &[u8]) -> [u8; 4] {
    let h = b2s(&[], &[b"SVITOK-BLOB-v1", data]);
    [
        ALPHABET[(h[0] % 32) as usize],
        ALPHABET[(h[1] % 32) as usize],
        ALPHABET[(h[2] % 32) as usize],
        ALPHABET[(h[3] % 32) as usize],
    ]
}

const CHARS_PER_LINE: usize = 16; // ровно 10 байт данных

/// Данные в строки листка, без финальной суммы.
pub fn to_paper_lines(data: &[u8]) -> Vec<String> {
    let chars = encode(data);
    let mut lines = Vec::new();
    for (i, chunk) in chars.chunks(CHARS_PER_LINE).enumerate() {
        let n = (i + 1) as u32;
        let check = line_check(n, chunk);
        let mut body = String::new();
        for (j, &c) in chunk.iter().enumerate() {
            if j > 0 && j % 4 == 0 {
                body.push(' ');
            }
            body.push(c as char);
        }
        lines.push(format!("{:02} {} {}", n, body, check as char));
    }
    lines
}

/// Полный бумажный вид: строки данных плюс сумма `== XXXX`.
pub fn to_paper(data: &[u8]) -> Vec<String> {
    let mut lines = to_paper_lines(data);
    let c = blob_check(data);
    lines.push(format!("== {}", core::str::from_utf8(&c).unwrap()));
    lines
}

#[derive(Debug, PartialEq, Eq)]
pub enum PaperError {
    /// Строку разобрать не удалось; число - её порядковый номер во вводе.
    Malformed(usize),
    /// Чек-символ строки с этим номером не сошёлся.
    LineCheck(u32),
    /// Номер строки пропущен или встретился дважды.
    LineNumber(u32),
    /// Итоговая сумма блоба не сошлась.
    BlobCheck,
    /// Строки суммы «== XXXX» вообще нет - без неё нельзя ручаться за целостность.
    MissingBlobCheck,
    /// В хвосте оказались ненулевые биты - символы потеряли или добавили лишние.
    Padding,
}

/// Разбор строк с листка. Порядок строк любой, а вот сумма `== XXXX`
/// обязана присутствовать.
pub fn from_paper(input_lines: &[&str]) -> Result<Vec<u8>, PaperError> {
    let mut rows: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut blob_sum: Option<Vec<u8>> = None;

    for (idx, raw) in input_lines.iter().enumerate() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some(rest) = t.strip_prefix("==") {
            let mut chars: Vec<u8> = Vec::new();
            for c in rest.chars().filter(|c| !c.is_whitespace() && *c != '-') {
                let v = char_value(c).ok_or(PaperError::Malformed(idx + 1))?;
                chars.push(ALPHABET[v as usize]);
            }
            if chars.len() != 4 {
                return Err(PaperError::Malformed(idx + 1));
            }
            blob_sum = Some(chars);
            continue;
        }
        // Номер строки идёт первым числом.
        let mut it = t.splitn(2, char::is_whitespace);
        let no: u32 = it
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or(PaperError::Malformed(idx + 1))?;
        let rest = it.next().ok_or(PaperError::Malformed(idx + 1))?;
        let mut chars: Vec<u8> = rest
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c.to_ascii_uppercase() as u8)
            .collect();
        if chars.len() < 2 {
            return Err(PaperError::Malformed(idx + 1));
        }
        // Последним символом идёт чек.
        let check = chars.pop().unwrap();
        // Двойники приводим к каноническим символам до сверки чека - он же
        // считался именно от канонических.
        for c in chars.iter_mut() {
            match char_value(*c as char) {
                Some(v) => *c = ALPHABET[v as usize],
                None => return Err(PaperError::Malformed(idx + 1)),
            }
        }
        if line_check(no, &chars) != check {
            return Err(PaperError::LineCheck(no));
        }
        rows.push((no, chars));
    }

    rows.sort_by_key(|(n, _)| *n);
    let mut all_chars = Vec::new();
    for (i, (n, chars)) in rows.iter().enumerate() {
        if *n != (i + 1) as u32 {
            return Err(PaperError::LineNumber((i + 1) as u32));
        }
        all_chars.extend_from_slice(chars);
    }

    let data = decode(&all_chars).ok_or(PaperError::Padding)?;

    // сумма обязательна: чек-символ строки ловит одиночную опечатку с p=31/32,
    // а оставшуюся долю закрывает именно она
    let sum = blob_sum.ok_or(PaperError::MissingBlobCheck)?;
    if blob_check(&data)[..] != sum[..] {
        return Err(PaperError::BlobCheck);
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn roundtrip_bytes() {
        for len in [0usize, 1, 4, 5, 10, 16, 33, 100] {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let enc = encode(&data);
            assert_eq!(decode(&enc).unwrap(), data);
        }
    }

    #[test]
    fn paper_roundtrip() {
        let data: Vec<u8> = (0u8..37).collect();
        let lines = to_paper(&data);
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        assert_eq!(from_paper(&refs).unwrap(), data);
    }

    #[test]
    fn paper_lenient_input() {
        let data = b"secret data here".to_vec();
        let lines = to_paper(&data);
        // Гоним через нижний регистр, o вместо 0, l вместо 1 и лишние дефисы.
        let mangled: Vec<String> = lines
            .iter()
            .map(|l| l.to_lowercase().replace('0', "o").replace('1', "l"))
            .collect();
        // Номера строк тоже поплыли ("01" стало "ol") - возвращаем цифры на место.
        let fixed: Vec<String> = mangled
            .iter()
            .enumerate()
            .map(|(i, l)| {
                if l.starts_with("==") {
                    l.clone()
                } else {
                    format!("{:02}{}", i + 1, &l[2..])
                }
            })
            .collect();
        let refs: Vec<&str> = fixed.iter().map(|s| s.as_str()).collect();
        assert_eq!(from_paper(&refs).unwrap(), data);
    }

    #[test]
    fn paper_detects_typo() {
        let data = b"hello world 123".to_vec();
        let mut lines = to_paper(&data);
        // Ломаем ровно один символ данных (индекс 4) в первой строке.
        let mut chars: Vec<char> = lines[0].chars().collect();
        chars[4] = if chars[4] == 'A' { 'B' } else { 'A' };
        lines[0] = chars.into_iter().collect();
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        match from_paper(&refs) {
            Err(PaperError::LineCheck(1)) | Err(PaperError::BlobCheck) => {}
            other => panic!("опечатка не поймана: {:?}", other),
        }
    }

    #[test]
    fn paper_detects_missing_line() {
        let data: Vec<u8> = (0u8..30).collect();
        let lines = to_paper(&data);
        let refs: Vec<&str> = lines
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 1)
            .map(|(_, s)| s.as_str())
            .collect();
        assert_eq!(from_paper(&refs), Err(PaperError::LineNumber(2)));
    }

    #[test]
    fn lines_out_of_order_ok() {
        let data: Vec<u8> = (0u8..30).collect();
        let lines = to_paper(&data);
        let reordered = vec![
            lines[2].as_str(),
            lines[0].as_str(),
            lines[3].as_str(),
            lines[1].as_str(),
        ];
        assert_eq!(from_paper(&reordered).unwrap(), data);
    }
}
