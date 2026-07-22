//! Локальный сокет-сервер для автозаполнения в браузере (десктоп).
//!
//! Native-messaging хост расширения (крейт `host`) присылает сюда JSON-запросы
//! по named pipe (Windows) / unix socket (mac/linux). Сокет доступен только
//! процессам того же пользователя. Расширение аутентифицируется токеном, который
//! пользователь один раз копирует из настроек Свитка (сам факт копирования из
//! GUI = подтверждение связки). На `fill` сервер матчит origin по списку сайтов
//! и, если ваулт разблокирован, выводит один пароль. Заблокирован - `locked`.

use crate::AppState;
use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Stream, ToNsName};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use svitok_common::store::Store;
use svitok_core::derive::site_password;
use tauri::Manager;

const SOCKET_NAME: &str = "svitok-autofill.sock";
const MAX_MSG: usize = 1024 * 1024;
const TOKEN_FILE: &str = "autofill.token";

pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        if let Err(e) = run(&app) {
            eprintln!("autofill ipc: {e}");
        }
    });
}

fn run(app: &tauri::AppHandle) -> io::Result<()> {
    let name = SOCKET_NAME
        .to_ns_name::<GenericNamespaced>()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let listener = ListenerOptions::new().name(name).create_sync()?;
    for conn in listener.incoming() {
        if let Ok(mut conn) = conn {
            let _ = handle(app, &mut conn);
        }
    }
    Ok(())
}

fn handle(app: &tauri::AppHandle, conn: &mut Stream) -> io::Result<()> {
    let req = read_framed(conn)?;
    let resp = process(app, &req);
    write_framed(conn, &resp)
}

fn process(app: &tauri::AppHandle, req: &[u8]) -> Vec<u8> {
    let v: serde_json::Value = match serde_json::from_slice(req) {
        Ok(v) => v,
        Err(_) => return err("bad-json"),
    };
    match v.get("op").and_then(|x| x.as_str()).unwrap_or("") {
        "ping" => br#"{"ok":true}"#.to_vec(),
        "fill" => fill(app, &v),
        _ => err("unknown-op"),
    }
}

fn fill(app: &tauri::AppHandle, v: &serde_json::Value) -> Vec<u8> {
    let dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(_) => return err("no-dir"),
    };
    let token = v.get("token").and_then(|x| x.as_str()).unwrap_or("");
    if !token_ok(&dir, token) {
        return err("unpaired");
    }
    let origin = v.get("origin").and_then(|x| x.as_str()).unwrap_or("");
    let canon = match svitok_core::domain::canonical(origin) {
        Some(c) => c,
        None => return err("bad-origin"),
    };
    // копия ключа из состояния; если заблокировано - просим разблокировать GUI
    let mk = {
        let g = app.state::<AppState>();
        let guard = g.lock().unwrap_or_else(|p| p.into_inner());
        guard.master_key.as_ref().map(|lk| *lk.get())
    };
    let mut mk = match mk {
        Some(k) => k,
        None => return err("locked"),
    };
    let store = match Store::load(&dir) {
        Ok(s) => s,
        Err(_) => {
            svitok_core::wipe::wipe(&mut mk);
            return err("no-store");
        }
    };
    let mut matches = Vec::new();
    for s in &store.sites {
        if svitok_core::domain::canonical(&s.name).as_deref() == Some(canon.as_str()) {
            if let Some(pw) = site_password(&mk, &s.name, &s.login, s.counter, &s.policy) {
                matches.push(serde_json::json!({
                    "name": s.name,
                    "login": s.login,
                    "password": pw,
                }));
            }
        }
    }
    svitok_core::wipe::wipe(&mut mk);
    serde_json::json!({ "ok": true, "matches": matches }).to_string().into_bytes()
}

fn err(code: &str) -> Vec<u8> {
    format!(r#"{{"ok":false,"error":"{code}"}}"#).into_bytes()
}

fn token_path(dir: &Path) -> PathBuf {
    dir.join(TOKEN_FILE)
}

fn token_ok(dir: &Path, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    match std::fs::read_to_string(token_path(dir)) {
        Ok(s) => svitok_core::wipe::ct_eq(s.trim().as_bytes(), token.as_bytes()),
        Err(_) => false,
    }
}

/// Токен связки для настроек: генерируем при первом обращении, дальше читаем.
pub fn get_or_create_token(dir: &Path) -> Result<String, String> {
    let path = token_path(dir);
    if let Ok(s) = std::fs::read_to_string(&path) {
        let t = s.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let mut raw = [0u8; 24];
    svitok_common::osrng::os_random(&mut raw).map_err(|e| e.to_string())?;
    let tok: String = raw.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    svitok_common::store::atomic_write(&path, tok.as_bytes())?;
    Ok(tok)
}

fn read_framed(r: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let n = u32::from_le_bytes(len) as usize;
    if n > MAX_MSG {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "запрос слишком велик"));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

fn write_framed(w: &mut impl Write, msg: &[u8]) -> io::Result<()> {
    w.write_all(&(msg.len() as u32).to_le_bytes())?;
    w.write_all(msg)?;
    w.flush()
}
