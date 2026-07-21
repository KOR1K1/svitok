//! Надёжное затирание секретов в памяти.
//! `write_volatile` плюс барьер компилятора: так оптимизатор не выкинет
//! эти записи как якобы бесполезные.

use core::sync::atomic::{compiler_fence, Ordering};

pub fn wipe(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        unsafe { core::ptr::write_volatile(b, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

pub fn wipe_u32(buf: &mut [u32]) {
    for w in buf.iter_mut() {
        unsafe { core::ptr::write_volatile(w, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

/// Держатель секретных байт: сам затирается, когда выходит из области видимости.
/// Поле приватное, иначе mem::take увёл бы буфер в обход затирания.
pub struct Secret(alloc::vec::Vec<u8>);

impl Secret {
    pub fn new(v: alloc::vec::Vec<u8>) -> Self {
        Secret(v)
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// Сравнение за постоянное время (нужно для MAC): без ранних выходов из цикла.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_works() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }
}
