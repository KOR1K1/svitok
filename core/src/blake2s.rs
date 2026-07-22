//! BLAKE2s-256 (RFC 7693). На нём тут держится всё: хеш, MAC (keyed-режим
//! идёт из коробки), PRF, KDF. Внутри только 32-битные сложения, XOR и
//! повороты, так что байт в байт одно и то же на любой машине.

/// IV: дробные части корней первых восьми простых. Те же, что в SHA-256,
/// так что при нужде можно пересчитать на бумажке, а не искать таблицу.
const IV: [u32; 8] = [
    0x6A09_E667, 0xBB67_AE85, 0x3C6E_F372, 0xA54F_F53A,
    0x510E_527F, 0x9B05_688C, 0x1F83_D9AB, 0x5BE0_CD19,
];

/// Как переставляются слова сообщения на каждом раунде (RFC 7693, SIGMA).
const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

pub struct Blake2s {
    h: [u32; 8],
    buf: [u8; 64],
    buflen: usize,
    t: u64,
}

#[inline(always)]
fn g(v: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, x: u32, y: u32) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(12);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(8);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(7);
}

impl Blake2s {
    /// Новый контекст. Пустой `key` даёт обычный хеш, непустой (до 32 байт) - MAC/PRF.
    pub fn new(key: &[u8]) -> Self {
        debug_assert!(key.len() <= 32);
        let mut h = IV;
        // Блок параметров: длина дайджеста 32, длина ключа, fanout=1, depth=1.
        h[0] ^= 0x0101_0000 ^ ((key.len() as u32) << 8) ^ 32;
        let mut s = Blake2s { h, buf: [0u8; 64], buflen: 0, t: 0 };
        if !key.is_empty() {
            let mut block = [0u8; 64];
            block[..key.len()].copy_from_slice(key);
            s.update(&block);
            crate::wipe::wipe(&mut block);
        }
        s
    }

    fn compress(&mut self, last: bool) {
        let mut m = [0u32; 16];
        for (i, w) in m.iter_mut().enumerate() {
            *w = u32::from_le_bytes([
                self.buf[4 * i], self.buf[4 * i + 1], self.buf[4 * i + 2], self.buf[4 * i + 3],
            ]);
        }
        let mut v = [0u32; 16];
        v[..8].copy_from_slice(&self.h);
        v[8..].copy_from_slice(&IV);
        v[12] ^= self.t as u32;
        v[13] ^= (self.t >> 32) as u32;
        if last {
            v[14] ^= 0xFFFF_FFFF;
        }
        for s in &SIGMA {
            g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }
        for i in 0..8 {
            self.h[i] ^= v[i] ^ v[i + 8];
        }
        crate::wipe::wipe_u32(&mut m);
        crate::wipe::wipe_u32(&mut v);
    }

    pub fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            if self.buflen == 64 {
                self.t = self.t.wrapping_add(64);
                self.compress(false);
                self.buflen = 0;
            }
            let n = core::cmp::min(64 - self.buflen, data.len());
            self.buf[self.buflen..self.buflen + n].copy_from_slice(&data[..n]);
            self.buflen += n;
            data = &data[n..];
        }
    }

    pub fn finalize(mut self) -> [u8; 32] {
        self.t = self.t.wrapping_add(self.buflen as u64);
        self.buf[self.buflen..].fill(0);
        self.compress(true);
        let mut out = [0u8; 32];
        for (i, w) in self.h.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
        }
        crate::wipe::wipe(&mut self.buf);
        crate::wipe::wipe_u32(&mut self.h);
        out
    }
}

// finalize() затирает состояние сам, но контекст, бро́шенный без finalize
// (ранний возврат, паника), оставил бы ключевой блок в buf и state в h.
impl Drop for Blake2s {
    fn drop(&mut self) {
        crate::wipe::wipe(&mut self.buf);
        crate::wipe::wipe_u32(&mut self.h);
    }
}

/// Хеш от склеенных вместе кусков за один вызов, с ключом или без.
pub fn b2s(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Blake2s::new(key);
    for p in parts {
        h.update(p);
    }
    h.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> alloc::string::String {
        use core::fmt::Write;
        let mut s = alloc::string::String::new();
        for x in b {
            write!(s, "{:02x}", x).unwrap();
        }
        s
    }

    #[test]
    fn rfc7693_abc() {
        let d = b2s(&[], &[b"abc"]);
        assert_eq!(
            hex(&d),
            "508c5e8c327c14e2e1a72ba34eeb452f37458b209ed63a294d999b4c86675982"
        );
    }

    #[test]
    fn empty_input() {
        let d = b2s(&[], &[]);
        assert_eq!(
            hex(&d),
            "69217a3079908094e11121d042354a7c1f55b6482ca1a51e1b250dfd1ed0eef9"
        );
    }

    #[test]
    fn keyed_kat_0() {
        // Официальные keyed-векторы BLAKE2s: key=00..1f, сообщение пустое и один байт 00.
        let key: alloc::vec::Vec<u8> = (0u8..32).collect();
        let d = b2s(&key, &[]);
        assert_eq!(
            hex(&d),
            "48a8997da407876b3d79c0d92325ad3b89cbb754d86ab71aee047ad345fd2c49"
        );
        let d1 = b2s(&key, &[&[0u8]]);
        assert_eq!(
            hex(&d1),
            "40d15fee7c328830166ac3f918650f807e7e01e177258cdc0a39b11f598066f1"
        );
    }

    #[test]
    fn multi_part_equals_concat() {
        let a = b2s(b"k", &[b"hello", b" ", b"world"]);
        let b = b2s(b"k", &[b"hello world"]);
        assert_eq!(a, b);
    }

    #[test]
    fn long_input_crosses_blocks() {
        let data = [0xABu8; 300];
        let one = b2s(&[], &[&data]);
        let mut h = Blake2s::new(&[]);
        for c in data.chunks(7) {
            h.update(c);
        }
        assert_eq!(one, h.finalize());
    }
}
