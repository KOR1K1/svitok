//! Платформенный слой: системный ГСЧ и ввод без эха.
//! Сырой FFI вместо зависимостей, чтобы весь код был на виду.

use std::io::{self, BufRead, Write};

// ГСЧ живёт в общем крейте - им пользуется и приложение.
pub use svitok_common::osrng::{generate_seed, os_random};

// ---------- Ввод без эха ----------

#[cfg(windows)]
mod echo {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: i32) -> *mut core::ffi::c_void;
        fn GetConsoleMode(h: *mut core::ffi::c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(h: *mut core::ffi::c_void, mode: u32) -> i32;
    }
    const STD_INPUT_HANDLE: i32 = -10;
    const ENABLE_ECHO_INPUT: u32 = 0x0004;

    pub fn set(on: bool) {
        unsafe {
            let h = GetStdHandle(STD_INPUT_HANDLE);
            let mut mode = 0u32;
            if GetConsoleMode(h, &mut mode) != 0 {
                let new = if on { mode | ENABLE_ECHO_INPUT } else { mode & !ENABLE_ECHO_INPUT };
                SetConsoleMode(h, new);
            }
        }
    }
}

#[cfg(unix)]
mod echo {
    pub fn set(on: bool) {
        let arg = if on { "echo" } else { "-echo" };
        let _ = std::process::Command::new("stty").arg(arg).status();
    }
}

/// Секретная строка без эха: фраза или TOTP-секрет.
pub fn read_secret(prompt: &str) -> io::Result<svitok_core::wipe::Secret> {
    print!("{prompt}");
    io::stdout().flush()?;
    echo::set(false);
    let mut line = String::new();
    let res = io::stdin().lock().read_line(&mut line);
    echo::set(true);
    println!();
    res?;
    let mut bytes = line.into_bytes();
    while matches!(bytes.last(), Some(b'\n') | Some(b'\r')) {
        bytes.pop();
    }
    Ok(svitok_core::wipe::Secret::new(bytes))
}

/// Обычная строка (с эхом).
pub fn read_line(prompt: &str) -> io::Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

/// Читает строки до пустой.
pub fn read_multiline(prompt: &str) -> io::Result<Vec<String>> {
    println!("{prompt}");
    let mut lines = Vec::new();
    loop {
        let l = read_line("  ")?;
        if l.trim().is_empty() {
            break;
        }
        lines.push(l);
    }
    Ok(lines)
}

/// По возможности включаем ANSI-коды в старой консоли Windows.
#[cfg(windows)]
pub fn enable_ansi() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: i32) -> *mut core::ffi::c_void;
        fn GetConsoleMode(h: *mut core::ffi::c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(h: *mut core::ffi::c_void, mode: u32) -> i32;
    }
    unsafe {
        let h = GetStdHandle(-11); // STD_OUTPUT_HANDLE
        let mut mode = 0u32;
        if GetConsoleMode(h, &mut mode) != 0 {
            SetConsoleMode(h, mode | 0x0004); // ENABLE_VIRTUAL_TERMINAL_PROCESSING
        }
    }
}

#[cfg(unix)]
pub fn enable_ansi() {}

/// Стереть n последних строк - например, после показа пароля.
pub fn erase_lines(n: usize) {
    for _ in 0..n {
        print!("\x1b[1A\x1b[2K");
    }
    let _ = io::stdout().flush();
}
