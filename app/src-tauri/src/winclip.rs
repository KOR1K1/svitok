//! Windows: копирование в буфер с исключением из истории буфера (Win+V) и
//! облачного буфера. arboard и плагин clipboard-manager этого не умеют, поэтому
//! пишем через Win32 напрямую и помечаем содержимое спец-форматами, чтобы
//! скопированный пароль не оседал в истории и не синхронизировался в облако.

use core::ffi::c_void;

const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x0002;

#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(hwnd: isize) -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, mem: isize) -> isize;
    fn CloseClipboard() -> i32;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
    fn GlobalLock(mem: isize) -> *mut c_void;
    fn GlobalUnlock(mem: isize) -> i32;
    fn GlobalFree(mem: isize) -> isize;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

/// Кладёт HGLOBAL с данными нужного формата. При успехе владение уходит системе,
/// поэтому память не освобождаем; при неудаче - освобождаем.
unsafe fn put(format: u32, data: &[u8]) -> bool {
    let h = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1));
    if h == 0 {
        return false;
    }
    let p = GlobalLock(h);
    if p.is_null() {
        GlobalFree(h);
        return false;
    }
    core::ptr::copy_nonoverlapping(data.as_ptr(), p as *mut u8, data.len());
    GlobalUnlock(h);
    if SetClipboardData(format, h) == 0 {
        GlobalFree(h);
        return false;
    }
    true
}

pub fn copy_excluded(text: &str) -> Result<(), String> {
    let w = wide(text);
    let bytes = unsafe { core::slice::from_raw_parts(w.as_ptr() as *const u8, w.len() * 2) };
    unsafe {
        if OpenClipboard(0) == 0 {
            return Err("не открыть буфер обмена".into());
        }
        EmptyClipboard();
        let ok_text = put(CF_UNICODETEXT, bytes);
        // не в историю (Win+V), не в облачный буфер, не мониторить
        let zero = 0u32.to_ne_bytes();
        for name in ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"] {
            let fmt = RegisterClipboardFormatW(wide(name).as_ptr());
            if fmt != 0 {
                let _ = put(fmt, &zero);
            }
        }
        let ex = RegisterClipboardFormatW(wide("ExcludeClipboardContentFromMonitorProcessing").as_ptr());
        if ex != 0 {
            let _ = put(ex, &zero);
        }
        CloseClipboard();
        if ok_text {
            Ok(())
        } else {
            Err("не записать текст в буфер".into())
        }
    }
}
