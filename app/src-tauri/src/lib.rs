//! Точка входа Tauri-приложения «Свиток».
//! Мастер-ключ держим тут, в Rust-состоянии. В JS через мост он не уходит.

mod commands;
// импорт файлов - только десктоп: телефону список переносится по QR
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod import;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod ipc_server;
#[cfg(target_os = "android")]
mod jni_autofill;
mod seed;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod seedstore;
#[cfg(windows)]
mod winclip;

use std::path::PathBuf;
use std::sync::Mutex;

/// Заголовок окна Windows 11 под тему: «Чернила» (#141210) или «Пергамент»
/// (#F4EEE4). Дёргаем DWM: иммерсивный режим плюс явные цвета фона и текста.
#[cfg(windows)]
pub(crate) mod win_titlebar {
    use core::ffi::c_void;

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attr: u32, pv: *const c_void, cb: u32) -> i32;
    }

    fn set(hwnd: isize, attr: u32, val: u32) {
        unsafe {
            DwmSetWindowAttribute(hwnd, attr, &val as *const u32 as *const c_void, 4);
        }
    }

    /// у COLORREF порядок байтов 0x00BBGGRR
    pub fn apply(hwnd: isize, light: bool) {
        if light {
            set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, 0);
            set(hwnd, DWMWA_CAPTION_COLOR, 0x00E4_EEF4); // фон #F4EEE4
            set(hwnd, DWMWA_TEXT_COLOR, 0x001D_242A); // текст #2A241D
        } else {
            set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
            set(hwnd, DWMWA_CAPTION_COLOR, 0x0010_1214); // фон #141210
            set(hwnd, DWMWA_TEXT_COLOR, 0x00DE_E7ED); // текст #EDE7DE
        }
    }
}

/// Дескриптор Kotlin-плагина Keystore (только Android).
#[cfg(target_os = "android")]
pub struct SeedPlugin(pub tauri::plugin::PluginHandle<tauri::Wry>);

/// Плагин хранения сида. На Android цепляет Kotlin KeystorePlugin, на
/// десктопе это заглушка - там сид лежит в OS-хранилище секретов.
fn init_seed_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("svitokseed")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle = _api.register_android_plugin("app.svitok.vault", "KeystorePlugin")?;
                _app.manage(SeedPlugin(handle));
            }
            Ok(())
        })
        .build()
}

/// Разблокированное состояние. Мастер-ключ лежит в LockedKey: заперт в RAM
/// (не уходит в своп) и затирается при Drop - когда приложение закрывается
/// или по команде lock.
pub struct Inner {
    pub master_key: Option<svitok_common::lockmem::LockedKey>,
    pub dir: PathBuf,
}

pub type AppState = Mutex<Inner>;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(init_seed_plugin())
        .plugin(tauri_plugin_clipboard_manager::init());
    // файловый диалог для импорта; зовётся только из Rust-команды, поэтому
    // JS-разрешений на него нет и содержимое файла в webview не попадает
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder.plugin(tauri_plugin_dialog::init());
    }
    builder
        .setup(|_app| {
            #[cfg(windows)]
            {
                use tauri::Manager;
                if let Some(win) = _app.get_webview_window("main") {
                    if let Ok(hwnd) = win.hwnd() {
                        // тёмный по умолчанию; фронт перекрасит под тему в boot()
                        win_titlebar::apply(hwnd.0 as isize, false);
                    }
                }
            }
            // локальный сокет для автозаполнения через браузерное расширение
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            ipc_server::start(_app.handle().clone());
            Ok(())
        })
        .manage(Mutex::new(Inner { master_key: None, dir: PathBuf::new() }))
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::create_vault,
            commands::restore_vault,
            commands::unlock,
            commands::lock,
            commands::destroy_vault,
            commands::list_sites,
            commands::add_site,
            commands::bump_site,
            commands::update_site,
            commands::remove_site,
            commands::show_seed,
            commands::derive_password,
            commands::vault_list,
            commands::totp_list,
            commands::totp_code,
            commands::vault_add_totp,
            commands::vault_add_password,
            commands::vault_add_note,
            commands::vault_add_codes,
            commands::vault_remove,
            commands::qr_svg,
            commands::set_screen_protection,
            commands::clip_copy,
            commands::clip_clear,
            commands::backup_export,
            commands::backup_import,
            commands::sync_export,
            commands::sync_preview,
            commands::sync_import,
            commands::autofill_token,
            commands::paper_export,
            commands::import_pick,
            commands::import_apply,
            commands::scan_qr,
            commands::apply_theme
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска Tauri");
}
