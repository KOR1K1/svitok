//! Native messaging host для браузерного расширения Свитка.
//!
//! Браузер запускает этот процесс и общается с ним по stdin/stdout в формате
//! native messaging (4 байта длины в порядке машины + JSON). Хост ничего не
//! разбирает и не хранит - он лишь пересылает JSON в запущенный GUI Свитка по
//! локальному сокету (named pipe на Windows, unix socket на mac/linux) и
//! возвращает ответ обратно в браузер. Секретов и ключей у хоста нет: это
//! недоверенный релей, вся логика и подтверждения - на стороне GUI.

use interprocess::local_socket::{prelude::*, GenericNamespaced, Stream, ToNsName};
use std::io::{self, Read, Write};

const SOCKET_NAME: &str = "svitok-autofill.sock";
const MAX_MSG: usize = 1024 * 1024; // 1 МиБ - потолок native messaging от расширения

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut r = stdin.lock();
    let mut w = stdout.lock();

    while let Some(req) = read_native(&mut r) {
        let resp = round_trip(&req).unwrap_or_else(|_| {
            br#"{"ok":false,"error":"svitok-not-running"}"#.to_vec()
        });
        if write_native(&mut w, &resp).is_err() {
            break;
        }
    }
}

/// Один запрос - одно соединение с GUI: подключились, отдали, забрали, закрыли.
fn round_trip(req: &[u8]) -> io::Result<Vec<u8>> {
    let name = SOCKET_NAME
        .to_ns_name::<GenericNamespaced>()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let mut conn = Stream::connect(name)?;
    write_framed(&mut conn, req)?;
    read_framed(&mut conn)
}

// ---- native messaging: длина в нативном порядке байт ----

fn read_native(r: &mut impl Read) -> Option<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).ok()?; // EOF - браузер закрыл канал
    let n = u32::from_ne_bytes(len) as usize;
    if n == 0 || n > MAX_MSG {
        return None;
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn write_native(w: &mut impl Write, msg: &[u8]) -> io::Result<()> {
    w.write_all(&(msg.len() as u32).to_ne_bytes())?;
    w.write_all(msg)?;
    w.flush()
}

// ---- локальный сокет до GUI: длина little-endian (обе стороны наши) ----

fn write_framed(w: &mut impl Write, msg: &[u8]) -> io::Result<()> {
    w.write_all(&(msg.len() as u32).to_le_bytes())?;
    w.write_all(msg)?;
    w.flush()
}

fn read_framed(r: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let n = u32::from_le_bytes(len) as usize;
    if n > MAX_MSG {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "ответ слишком велик"));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(buf)
}
