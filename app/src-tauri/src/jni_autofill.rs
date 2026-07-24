//! JNI-мост для AutofillService. Он живёт вне Tauri-активити (это отдельный
//! системный компонент), поэтому Tauri-команды ему недоступны - зовём ядро
//! напрямую через эти экспортируемые функции.
//!
//! Секреты через них не утекают: `canonicalDomain` работает с публичными
//! доменами, а `derivePassword` получает сид (после биометрии) и фразу, считает
//! мастер-ключ, выводит ОДИН пароль и тут же затирает ключ. KDF тяжёлый
//! (десятки МиБ, секунды) - вызывать только из фонового потока, иначе ANR.

use jni::objects::{JClass, JString};
use jni::sys::{jint, jstring};
use jni::JNIEnv;

fn read(env: &mut JNIEnv, s: &JString) -> Option<String> {
    env.get_string(s).ok().map(|v| v.into())
}

fn make(env: &mut JNIEnv, s: &str) -> jstring {
    env.new_string(s).map(|j| j.into_raw()).unwrap_or(core::ptr::null_mut())
}

/// Registrable domain (eTLD+1) для строки. Пусто, если не сводится.
/// Kotlin сравнивает результат для сайта из списка и для домена страницы.
#[no_mangle]
pub extern "system" fn Java_app_svitok_vault_Native_canonicalDomain<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JString<'local>,
) -> jstring {
    let s = read(&mut env, &input).unwrap_or_default();
    let out = svitok_core::domain::canonical(&s).unwrap_or_default();
    make(&mut env, &out)
}

/// Полная деривация одного пароля: сид (hex, из Keystore после биометрии) +
/// фраза + параметры KDF (M, T из «# kdf» в списке) + строка сайта из списка.
/// Возвращает пароль или пустую строку при ошибке.
#[no_mangle]
pub extern "system" fn Java_app_svitok_vault_Native_derivePassword<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    seed_hex: JString<'local>,
    phrase: JString<'local>,
    m: jint,
    t: jint,
    site_line: JString<'local>,
) -> jstring {
    let out = derive_inner(&mut env, &seed_hex, &phrase, m, t, &site_line).unwrap_or_default();
    make(&mut env, &out)
}

fn derive_inner(
    env: &mut JNIEnv,
    seed_hex: &JString,
    phrase: &JString,
    m: jint,
    t: jint,
    site_line: &JString,
) -> Option<String> {
    let seed_hex = read(env, seed_hex)?;
    let mut phrase_s = read(env, phrase)?;
    let line = read(env, site_line)?;

    let mut seed = hex16(&seed_hex)?;
    let kdf = svitok_core::kdf::KdfParams::parse(m as u8, t as u8)?;
    let site = svitok_common::store::Site::from_line(line.trim()).ok()?;

    let mut mk = svitok_core::kdf::master_key(&seed, phrase_s.as_bytes(), kdf);
    svitok_core::wipe::wipe(&mut seed);
    svitok_core::wipe::wipe_str(&mut phrase_s);

    let pw = svitok_core::derive::site_password(&mk, &site.name, &site.login, site.counter, &site.policy);
    svitok_core::wipe::wipe(&mut mk);
    pw
}

/// Текущий код привязанного TOTP: сид (после биометрии) + фраза + KDF + бумажные
/// строки vault.b32 + метка записи. Расшифровывает сейф, находит TOTP по метке,
/// отдаёт код. Секреты держим кратко и затираем. Тяжёлый KDF - только из фонового
/// потока (как derivePassword).
#[no_mangle]
pub extern "system" fn Java_app_svitok_vault_Native_deriveTotp<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    seed_hex: JString<'local>,
    phrase: JString<'local>,
    m: jint,
    t: jint,
    vault_b32: JString<'local>,
    label: JString<'local>,
) -> jstring {
    let out = totp_inner(&mut env, &seed_hex, &phrase, m, t, &vault_b32, &label).unwrap_or_default();
    make(&mut env, &out)
}

fn totp_inner(
    env: &mut JNIEnv,
    seed_hex: &JString,
    phrase: &JString,
    m: jint,
    t: jint,
    vault_b32: &JString,
    label: &JString,
) -> Option<String> {
    let seed_hex = read(env, seed_hex)?;
    let mut phrase_s = read(env, phrase)?;
    let vault_text = read(env, vault_b32)?;
    let want = read(env, label)?;

    let mut seed = hex16(&seed_hex)?;
    let kdf = svitok_core::kdf::KdfParams::parse(m as u8, t as u8)?;

    let lines: Vec<&str> = vault_text.lines().collect();
    let blob = svitok_core::base32::from_paper(&lines).ok()?;

    let mut mk = svitok_core::kdf::master_key(&seed, phrase_s.as_bytes(), kdf);
    svitok_core::wipe::wipe(&mut seed);
    svitok_core::wipe::wipe_str(&mut phrase_s);

    let entries = svitok_core::vault::decrypt(&mk, &blob);
    svitok_core::wipe::wipe(&mut mk);
    let entries = entries.ok()?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for e in &entries {
        if let svitok_core::vault::Entry::Totp { label: l, secret, digits8, period, .. } = e {
            if *l == want {
                let digits = if *digits8 { 8 } else { 6 };
                let n = svitok_core::totp::totp(secret, now, *period, digits);
                return Some(format!("{:0width$}", n, width = digits as usize));
            }
        }
    }
    None
}

fn hex16(s: &str) -> Option<[u8; 16]> {
    let s = s.trim();
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}
