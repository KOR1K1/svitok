//! Платформенный слой хранения сида.
//!   Android: Kotlin-плагин KeystorePlugin. Шифрует через Android Keystore,
//!            ключ не вытащить из TEE, вторым шагом идёт биометрия.
//!   Десктоп: FileSeedStore поверх OS-хранилища секретов.
//! Между Rust и Kotlin сид передаётся как hex и только внутри процесса (JNI).
//! В JS он не уходит.

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::seedstore::FileSeedStore;
use std::path::Path;

#[cfg(target_os = "android")]
fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{:02x}", x));
    }
    s
}

#[cfg(target_os = "android")]
fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

pub fn has_seed(app: &tauri::AppHandle, dir: &Path) -> Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let _ = dir;
        #[derive(serde::Deserialize)]
        struct R {
            value: bool,
        }
        let p = app.state::<crate::SeedPlugin>();
        let r: R = p.0.run_mobile_plugin("hasSeed", ()).map_err(|e| e.to_string())?;
        Ok(r.value)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        FileSeedStore::new(dir).has_seed()
    }
}

pub fn store_seed(app: &tauri::AppHandle, dir: &Path, seed: &[u8; 16]) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let _ = dir;
        #[derive(serde::Serialize)]
        struct A<'a> {
            seed: &'a str,
        }
        // держим hex у себя и передаём по ссылке, чтобы затереть свою копию после
        // вызова (JNI/serde сделают промежуточную копию - она вне нашего контроля)
        let mut h = hex(seed);
        let p = app.state::<crate::SeedPlugin>();
        let out: Result<serde_json::Value, _> = p.0.run_mobile_plugin("storeSeed", A { seed: &h });
        svitok_core::wipe::wipe_str(&mut h);
        out.map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        FileSeedStore::new(dir).store_seed(seed)
    }
}

pub fn load_seed(app: &tauri::AppHandle, dir: &Path) -> Result<[u8; 16], String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let _ = dir;
        #[derive(serde::Deserialize)]
        struct R {
            seed: String,
        }
        let p = app.state::<crate::SeedPlugin>();
        let mut r: R = p.0.run_mobile_plugin("loadSeed", ()).map_err(|e| e.to_string())?;
        let b = unhex(&r.seed);
        svitok_core::wipe::wipe_str(&mut r.seed); // hex-строку сида не оставляем в куче
        let mut b = b.ok_or("плохой hex сида")?;
        if b.len() != 16 {
            svitok_core::wipe::wipe(&mut b);
            return Err("сид не 16 байт".into());
        }
        let mut s = [0u8; 16];
        s.copy_from_slice(&b);
        svitok_core::wipe::wipe(&mut b);
        Ok(s)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        FileSeedStore::new(dir).load_seed()
    }
}

/// Стереть сид из хранилища (для «Уничтожить Свиток»).
pub fn clear_seed(app: &tauri::AppHandle, dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let _ = dir;
        let p = app.state::<crate::SeedPlugin>();
        let _: serde_json::Value = p.0.run_mobile_plugin("clearSeed", ()).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        FileSeedStore::new(dir).clear_seed()
    }
}
