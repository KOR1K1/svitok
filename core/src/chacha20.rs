//! ChaCha20 (RFC 8439). Шифрует мини-сейф и заодно служит источником
//! псевдослучайных байт при генерации паролей.
//! ARX (сложение, XOR, сдвиг), поэтому результат одинаков везде.

/// Строка "expand 32-byte k", разложенная на четыре little-endian слова.
const CONSTANTS: [u32; 4] = [0x6170_7865, 0x3320_646E, 0x7962_2D32, 0x6B20_6574];

#[inline(always)]
fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

/// Считает один 64-байтовый блок keystream.
pub fn block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut s = [0u32; 16];
    s[..4].copy_from_slice(&CONSTANTS);
    for i in 0..8 {
        s[4 + i] = u32::from_le_bytes([key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]]);
    }
    s[12] = counter;
    for i in 0..3 {
        s[13 + i] =
            u32::from_le_bytes([nonce[4 * i], nonce[4 * i + 1], nonce[4 * i + 2], nonce[4 * i + 3]]);
    }
    let mut init = s;
    for _ in 0..10 {
        quarter_round(&mut s, 0, 4, 8, 12);
        quarter_round(&mut s, 1, 5, 9, 13);
        quarter_round(&mut s, 2, 6, 10, 14);
        quarter_round(&mut s, 3, 7, 11, 15);
        quarter_round(&mut s, 0, 5, 10, 15);
        quarter_round(&mut s, 1, 6, 11, 12);
        quarter_round(&mut s, 2, 7, 8, 13);
        quarter_round(&mut s, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        let w = s[i].wrapping_add(init[i]);
        out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    crate::wipe::wipe_u32(&mut s);
    crate::wipe::wipe_u32(&mut init); // копия с ключевыми словами тоже секрет
    out
}

/// XOR данных с keystream. Шифрование и расшифрование - это одно и то же.
pub fn xor_stream(key: &[u8; 32], nonce: &[u8; 12], initial_counter: u32, data: &mut [u8]) {
    let mut counter = initial_counter;
    for chunk in data.chunks_mut(64) {
        let mut ks = block(key, counter, nonce);
        for (d, k) in chunk.iter_mut().zip(ks.iter()) {
            *d ^= k;
        }
        crate::wipe::wipe(&mut ks);
        counter = counter.wrapping_add(1);
    }
}

/// Бесконечный детерминированный поток байт из ключа. Из него берётся
/// вся «случайность» при генерации паролей.
pub struct ByteStream {
    key: [u8; 32],
    buf: [u8; 64],
    pos: usize,
    counter: u32,
}

impl ByteStream {
    pub fn new(key: [u8; 32]) -> Self {
        ByteStream { key, buf: [0u8; 64], pos: 64, counter: 0 }
    }

    pub fn next_byte(&mut self) -> u8 {
        if self.pos == 64 {
            self.buf = block(&self.key, self.counter, &[0u8; 12]);
            self.counter = self.counter.wrapping_add(1);
            self.pos = 0;
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        b
    }

    /// Ровное число в [0, n): лишние байты отбрасываем, чтобы не было перекоса от остатка.
    pub fn next_mod(&mut self, n: usize) -> usize {
        debug_assert!(n > 0 && n <= 256);
        let limit = 256 - (256 % n);
        loop {
            let b = self.next_byte() as usize;
            if b < limit {
                return b % n;
            }
        }
    }
}

impl Drop for ByteStream {
    fn drop(&mut self) {
        crate::wipe::wipe(&mut self.key);
        crate::wipe::wipe(&mut self.buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc8439_sunscreen() {
        // RFC 8439 §2.4.2: key=00..1f, nonce=00 00 00 00 00 00 00 4a 00 00 00 00, счётчик=1.
        let mut key = [0u8; 32];
        for (i, k) in key.iter_mut().enumerate() {
            *k = i as u8;
        }
        let nonce = [0, 0, 0, 0, 0, 0, 0, 0x4a, 0, 0, 0, 0];
        let mut data = *b"Ladies and Gentlemen of the class of '99: If I could offer you \
only one tip for the future, sunscreen would be it.";
        assert_eq!(data.len(), 114);
        xor_stream(&key, &nonce, 1, &mut data);
        assert_eq!(
            &data[..16],
            &[0x6e, 0x2e, 0x35, 0x9a, 0x25, 0x68, 0xf9, 0x80, 0x41, 0xba, 0x07, 0x28, 0xdd, 0x0d, 0x69, 0x81]
        );
        // Ещё раз тем же потоком - получаем исходный текст обратно.
        xor_stream(&key, &nonce, 1, &mut data);
        assert_eq!(&data[..6], b"Ladies");
    }

    #[test]
    fn stream_deterministic() {
        let mut a = ByteStream::new([7u8; 32]);
        let mut b = ByteStream::new([7u8; 32]);
        for _ in 0..1000 {
            assert_eq!(a.next_byte(), b.next_byte());
        }
    }

    #[test]
    fn next_mod_in_range() {
        let mut s = ByteStream::new([1u8; 32]);
        for n in [1usize, 2, 10, 26, 95, 256] {
            for _ in 0..200 {
                assert!(s.next_mod(n) < n);
            }
        }
    }
}
