//! Системный ГСЧ без внешних зависимостей. Сырой FFI, весь код на виду.

use std::io;

#[cfg(windows)]
pub fn os_random(buf: &mut [u8]) -> io::Result<()> {
    // RtlGenRandom: документированный экспорт advapi32 под именем SystemFunction036.
    #[link(name = "advapi32")]
    extern "system" {
        fn SystemFunction036(buf: *mut u8, len: u32) -> u8;
    }
    let ok = unsafe { SystemFunction036(buf.as_mut_ptr(), buf.len() as u32) };
    if ok == 0 {
        return Err(io::Error::new(io::ErrorKind::Other, "SystemFunction036 failed"));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn os_random(buf: &mut [u8]) -> io::Result<()> {
    // getrandom(2) через glibc-обёртку: без флагов он блокируется, пока пул не
    // проинициализирован, поэтому не отдаёт слабые байты на ранней загрузке
    // (чего не гарантирует /dev/urandom).
    extern "C" {
        fn getrandom(buf: *mut u8, buflen: usize, flags: u32) -> isize;
    }
    let mut filled = 0;
    while filled < buf.len() {
        let n = unsafe { getrandom(buf[filled..].as_mut_ptr(), buf.len() - filled, 0) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "getrandom вернул 0"));
        }
        filled += n as usize;
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn os_random(buf: &mut [u8]) -> io::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(buf)
}

/// 16 байт сида: берём ГСЧ ОС и подмешиваем время и любую лишнюю энтропию
/// (скажем, шум с клавиатуры), чтобы бэкдор в одном источнике не был фатален.
pub fn generate_seed(extra_entropy: &[u8]) -> io::Result<[u8; 16]> {
    let mut osr = [0u8; 32];
    os_random(&mut osr)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut h = svitok_core::blake2s::b2s(
        &[],
        &[b"SVITOK-SEEDGEN", &osr, &now.to_le_bytes(), extra_entropy],
    );
    svitok_core::wipe::wipe(&mut osr);
    let mut seed = [0u8; 16];
    seed.copy_from_slice(&h[..16]);
    svitok_core::wipe::wipe(&mut h); // в h[..16] лежит тот же сид
    Ok(seed)
}
