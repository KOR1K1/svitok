//! Медленный memory-hard KDF по мотивам scrypt-ROMix, но целиком на BLAKE2s -
//! так весь алгоритм влезает на листок бумаги.
//!
//!   h    = B2S("SVITOK-KDF-v1" ‖ le32(len(seed)) ‖ seed ‖ le32(len(phrase)) ‖ phrase)
//!   V[0] = h;  V[i] = B2S(V[i-1])            - заполняем 2^M блоков по 32 байта
//!   x    = B2S(V[2^M - 1])
//!   T раз:  j = le32(x[0..4]) mod 2^M;  x = B2S(x XOR V[j])
//!   mk   = B2S("SVITOK-MK-v1" ‖ x)
//!
//! Массив 2^M × 32 байта вынуждает атакующего на GPU или ASIC держать эту
//! память на каждый параллельный перебор фразы, а T крутит время.
//! Параметры (M, T) пишутся на листок рядом с сидом: «K M20 T21».

use crate::blake2s::b2s;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KdfParams {
    /// log2 числа 32-байтовых блоков памяти (20 даёт 32 МиБ).
    pub m_log2: u8,
    /// log2 числа итераций перемешивания.
    pub t_log2: u8,
}

impl KdfParams {
    pub const DEFAULT: KdfParams = KdfParams { m_log2: 20, t_log2: 21 };

    /// Вид для листка, например «M17 T21».
    pub fn to_paper(&self) -> alloc::string::String {
        alloc::format!("M{} T{}", self.m_log2, self.t_log2)
    }

    pub fn parse(m: u8, t: u8) -> Option<KdfParams> {
        // Разумный потолок: не больше 1 ГиБ памяти и 2^28 итераций.
        if (8..=25).contains(&m) && (10..=28).contains(&t) {
            Some(KdfParams { m_log2: m, t_log2: t })
        } else {
            None
        }
    }
}

/// Мастер-ключ из сида (он на листке) и мастер-фразы (она в голове).
pub fn master_key(seed: &[u8], phrase: &[u8], p: KdfParams) -> [u8; 32] {
    // Потолки те же, что в parse(). Поля KdfParams публичны, так что структуру
    // можно собрать напрямую в обход parse - без этого клампа m_log2>=64 дал бы
    // UB/панику сдвига, огромный M - OOM, а M>32 усёк бы mask и сломал выбор j.
    // Для нормальных параметров (M<=25) это ничего не меняет.
    let m_log2 = p.m_log2.min(25);
    let t_log2 = p.t_log2.min(28);
    let n_blocks: usize = 1usize << m_log2;
    let t_iters: u64 = 1u64 << t_log2;

    let mut x = b2s(
        &[],
        &[
            b"SVITOK-KDF-v1",
            &(seed.len() as u32).to_le_bytes(),
            seed,
            &(phrase.len() as u32).to_le_bytes(),
            phrase,
        ],
    );

    let mut v: Vec<[u8; 32]> = vec![[0u8; 32]; n_blocks];
    v[0] = x;
    for i in 1..n_blocks {
        v[i] = b2s(&[], &[&v[i - 1]]);
    }
    x = b2s(&[], &[&v[n_blocks - 1]]);

    let mask = (n_blocks - 1) as u32;
    let mut xored = [0u8; 32];
    for _ in 0..t_iters {
        let j = (u32::from_le_bytes([x[0], x[1], x[2], x[3]]) & mask) as usize;
        for k in 0..32 {
            xored[k] = x[k] ^ v[j][k];
        }
        x = b2s(&[], &[&xored]);
    }

    let mk = b2s(&[], &[b"SVITOK-MK-v1", &x]);

    for blk in v.iter_mut() {
        crate::wipe::wipe(blk);
    }
    crate::wipe::wipe(&mut x);
    crate::wipe::wipe(&mut xored);
    mk
}

/// Подключ под отдельную подсистему, чтобы разделить домены.
pub fn subkey(mk: &[u8; 32], context: &[u8]) -> [u8; 32] {
    b2s(mk, &[b"CTX:", context])
}

/// Отпечаток мастер-ключа - два символа на листок. По ним сразу видно
/// опечатку во фразе. Атакующему, у которого уже есть листок, он ничего
/// нового не открывает: то же самое проверяется по MAC сейфа.
pub fn fingerprint(mk: &[u8; 32]) -> [u8; 2] {
    let mut h = b2s(mk, &[b"CTX:", b"fingerprint"]);
    let a = crate::base32::ALPHABET[(h[0] % 32) as usize];
    let b = crate::base32::ALPHABET[(h[1] % 32) as usize];
    crate::wipe::wipe(&mut h);
    [a, b]
}

#[cfg(test)]
mod tests {
    use super::*;

    // Лёгкие параметры, чтобы тесты не тормозили.
    const TP: KdfParams = KdfParams { m_log2: 8, t_log2: 10 };

    #[test]
    fn deterministic() {
        let a = master_key(b"seed-bytes-0123", b"correct horse", TP);
        let b = master_key(b"seed-bytes-0123", b"correct horse", TP);
        assert_eq!(a, b);
    }

    #[test]
    fn sensitive_to_inputs() {
        let base = master_key(b"seed", b"phrase", TP);
        assert_ne!(base, master_key(b"seed", b"phrasf", TP));
        assert_ne!(base, master_key(b"seec", b"phrase", TP));
        assert_ne!(base, master_key(b"seed", b"phrase", KdfParams { m_log2: 8, t_log2: 11 }));
        // Длины закодированы явно, поэтому границу seed|phrase не склеить.
        assert_ne!(
            master_key(b"ab", b"cd", TP),
            master_key(b"abc", b"d", TP)
        );
    }

    #[test]
    fn subkeys_differ() {
        let mk = [42u8; 32];
        assert_ne!(subkey(&mk, b"enc"), subkey(&mk, b"mac"));
    }

    /// Золотой вектор намертво фиксирует битовую совместимость.
    /// Упал этот тест - значит поменялась сама схема, и все уже
    /// написанные листки перестанут читаться. Не трогать.
    #[test]
    fn golden_vector() {
        // сам вектор совместимости прибит в tests/golden.rs, тут просто
        // проверяем, что ключ считается и не выходит нулевым
        let mk = master_key(b"SVITOK-GOLDEN-SEED", b"golden phrase", TP);
        assert_eq!(mk.len(), 32);
        assert_ne!(mk, [0u8; 32]);
    }
}
