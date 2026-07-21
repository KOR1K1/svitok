//! Десктоп-хранилище сида. Кладём его в нативное хранилище секретов через `keyring`:
//!   Windows: Credential Manager, шифрование DPAPI под аккаунт пользователя,
//!   macOS:   Keychain,
//!   Linux:   Secret Service (libsecret / GNOME Keyring).
//! Сид - это 16 байт, лежит одной записью в виде hex-строки. На Android всё иначе,
//! там Android Keystore (см. seed.rs и KeystorePlugin.kt).

use std::path::Path;
use zeroize::Zeroize;

const SERVICE: &str = "app.svitok.vault";
const ACCOUNT: &str = "seed-v1";

pub struct FileSeedStore;

impl FileSeedStore {
    /// `dir` тут не нужен: хранилище одно на пользователя.
    pub fn new(_dir: &Path) -> Self {
        FileSeedStore
    }

    fn entry() -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())
    }

    pub fn has_seed(&self) -> bool {
        match Self::entry() {
            Ok(e) => !matches!(e.get_password(), Err(keyring::Error::NoEntry)),
            Err(_) => false,
        }
    }

    pub fn load_seed(&self) -> Result<[u8; 16], String> {
        let mut hex = Self::entry()?.get_password().map_err(|e| e.to_string())?;
        let res = decode_hex16(&hex);
        hex.zeroize();
        res
    }

    pub fn store_seed(&self, seed: &[u8; 16]) -> Result<(), String> {
        let mut hex = encode_hex(seed);
        let res = Self::entry()?.set_password(&hex).map_err(|e| e.to_string());
        hex.zeroize();
        res
    }

    pub fn clear_seed(&self) -> Result<(), String> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn encode_hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

fn decode_hex16(s: &str) -> Result<[u8; 16], String> {
    let s = s.trim();
    if s.len() != 32 {
        return Err("сид не 16 байт".into());
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| "плохой hex сида")?;
    }
    Ok(out)
}
