//! svitok-core - криптографическое ядро «Свитка».
//!
//! Внешних зависимостей нет, `no_std` плюс alloc. Собирается под любой таргет
//! (x86/x64/ARM/RISC-V/WASM) и всюду выдаёт побитово одинаковый результат.
//!
//! В основе два примитива: BLAKE2s (хеш, MAC, KDF, PRF) и ChaCha20 (шифр и
//! поток), оба - чистый ARX на 32-битных словах. SHA-1 с HMAC держим только
//! ради совместимости с TOTP.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod base32;
pub mod blake2s;
pub mod chacha20;
pub mod derive;
pub mod domain;
pub mod kdf;
mod psl_data;
pub mod sha1;
pub mod totp;
pub mod vault;
pub mod wipe;

/// Сид на листке: 128 бит, это 26 символов Base32.
pub const SEED_LEN: usize = 16;
