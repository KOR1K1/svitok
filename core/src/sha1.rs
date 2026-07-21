//! SHA-1 и HMAC-SHA1. Тут они нужны исключительно ради TOTP (RFC 6238):
//! Google Authenticator и почти все сайты намертво завязаны на HMAC-SHA1.
//! Сам «Свиток» на SHA-1 в плане безопасности не полагается.

pub fn sha1(parts: &[&[u8]]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let total_len: u64 = parts.iter().map(|p| p.len() as u64).sum();

    let mut buf = [0u8; 64];
    let mut buflen = 0usize;

    let process = |buf: &[u8; 64], h: &mut [u32; 5]| {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([buf[4 * i], buf[4 * i + 1], buf[4 * i + 2], buf[4 * i + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    };

    for part in parts {
        let mut data: &[u8] = part;
        while !data.is_empty() {
            let n = core::cmp::min(64 - buflen, data.len());
            buf[buflen..buflen + n].copy_from_slice(&data[..n]);
            buflen += n;
            data = &data[n..];
            if buflen == 64 {
                process(&buf, &mut h);
                buflen = 0;
            }
        }
    }

    // Дополнение: байт 0x80, потом нули, в конце длина в битах (big-endian, 8 байт).
    buf[buflen] = 0x80;
    buf[buflen + 1..].fill(0);
    if buflen + 1 > 56 {
        process(&buf, &mut h);
        buf.fill(0);
    }
    buf[56..].copy_from_slice(&(total_len * 8).to_be_bytes());
    process(&buf, &mut h);

    let mut out = [0u8; 20];
    for (i, w) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&w.to_be_bytes());
    }
    out
}

pub fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; 20] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..20].copy_from_slice(&sha1(&[key]));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = sha1(&[&ipad, msg]);
    let out = sha1(&[&opad, &inner]);
    crate::wipe::wipe(&mut k);
    crate::wipe::wipe(&mut ipad);
    crate::wipe::wipe(&mut opad);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_abc() {
        let d = sha1(&[b"abc"]);
        assert_eq!(
            d,
            [
                0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78,
                0x50, 0xc2, 0x6c, 0x9c, 0xd0, 0xd8, 0x9d
            ]
        );
    }

    #[test]
    fn sha1_empty() {
        let d = sha1(&[]);
        assert_eq!(
            d,
            [
                0xda, 0x39, 0xa3, 0xee, 0x5e, 0x6b, 0x4b, 0x0d, 0x32, 0x55, 0xbf, 0xef, 0x95,
                0x60, 0x18, 0x90, 0xaf, 0xd8, 0x07, 0x09
            ]
        );
    }

    #[test]
    fn hmac_rfc2202_case1() {
        // key = 0x0b × 20, data = "Hi There"
        let key = [0x0bu8; 20];
        let d = hmac_sha1(&key, b"Hi There");
        assert_eq!(
            d,
            [
                0xb6, 0x17, 0x31, 0x86, 0x55, 0x05, 0x72, 0x64, 0xe2, 0x8b, 0xc0, 0xb6, 0xfb,
                0x37, 0x8c, 0x8e, 0xf1, 0x46, 0xbe, 0x00
            ]
        );
    }

    #[test]
    fn hmac_rfc2202_case2() {
        let d = hmac_sha1(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            d,
            [
                0xef, 0xfc, 0xdf, 0x6a, 0xe5, 0xeb, 0x2f, 0xa2, 0xd2, 0x74, 0x16, 0xd5, 0xf1,
                0x84, 0xdf, 0x9c, 0x25, 0x9a, 0x7c, 0x79
            ]
        );
    }
}
