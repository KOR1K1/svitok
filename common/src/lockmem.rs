//! Ключевой материал, запертый в RAM: страница с ним не должна уйти в своп,
//! чтобы мастер-ключ не осел на диске. Best-effort - если ОС отказала (лимит
//! залоченной памяти, нет прав), просто не заперто, но при Drop всё равно
//! затирается. Запираем только сам ключ (32 байта), а не 32-МиБ буфер KDF:
//! такой объём упёрся бы в RLIMIT_MEMLOCK.

use svitok_core::wipe::wipe;

pub struct LockedKey {
    buf: Box<[u8; 32]>,
    locked: bool,
}

impl LockedKey {
    pub fn new(key: [u8; 32]) -> Self {
        let mut buf = Box::new(key);
        let locked = imp::lock_mem(buf.as_mut_ptr(), 32);
        LockedKey { buf, locked }
    }

    pub fn get(&self) -> &[u8; 32] {
        &self.buf
    }
}

impl Drop for LockedKey {
    fn drop(&mut self) {
        wipe(&mut self.buf[..]);
        if self.locked {
            imp::unlock_mem(self.buf.as_mut_ptr(), 32);
        }
    }
}

#[cfg(windows)]
mod imp {
    #[link(name = "kernel32")]
    extern "system" {
        fn VirtualLock(addr: *mut u8, size: usize) -> i32;
        fn VirtualUnlock(addr: *mut u8, size: usize) -> i32;
    }
    pub fn lock_mem(addr: *mut u8, size: usize) -> bool {
        unsafe { VirtualLock(addr, size) != 0 }
    }
    pub fn unlock_mem(addr: *mut u8, size: usize) {
        unsafe {
            let _ = VirtualUnlock(addr, size);
        }
    }
}

#[cfg(unix)]
mod imp {
    use core::ffi::c_void;
    extern "C" {
        fn mlock(addr: *const c_void, len: usize) -> i32;
        fn munlock(addr: *const c_void, len: usize) -> i32;
    }
    pub fn lock_mem(addr: *mut u8, size: usize) -> bool {
        unsafe { mlock(addr as *const c_void, size) == 0 }
    }
    pub fn unlock_mem(addr: *mut u8, size: usize) {
        unsafe {
            let _ = munlock(addr as *const c_void, size);
        }
    }
}

#[cfg(not(any(windows, unix)))]
mod imp {
    pub fn lock_mem(_addr: *mut u8, _size: usize) -> bool {
        false
    }
    pub fn unlock_mem(_addr: *mut u8, _size: usize) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wipe() {
        let k = LockedKey::new([7u8; 32]);
        assert_eq!(k.get(), &[7u8; 32]);
        // Drop затирает и (если было) разблокирует - тут просто проверяем, что
        // конструирование/чтение работают на всех платформах, даже без прав mlock.
    }
}
