//! Золотые векторы фиксируют битовую совместимость схемы.
//! Упал любой из этих тестов - значит поменялась сама схема, и старые
//! записи с бумаги уже не прочитаются. Ожидаемые значения править нельзя,
//! правим код, пока не совпадёт.
//!
//! Значения впечатываются один раз, командой:
//!   cargo test -p svitok-core --test golden -- --nocapture
//! (print_golden печатает то, что получилось на самом деле)

use svitok_core::base32;
use svitok_core::derive::{site_password, Policy};
use svitok_core::kdf::{fingerprint, master_key, KdfParams};
use svitok_core::vault::{decrypt, encrypt, Entry};

const TP: KdfParams = KdfParams { m_log2: 8, t_log2: 10 };
const SEED: &[u8] = b"\x00\x11\x22\x33\x44\x55\x66\x77\x88\x99\xaa\xbb\xcc\xdd\xee\xff";
const PHRASE: &[u8] = "тайная фраза".as_bytes();

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn mk() -> [u8; 32] {
    master_key(SEED, PHRASE, TP)
}

#[test]
fn print_golden() {
    let mk = mk();
    println!("master_key = {}", hex(&mk));
    println!("fingerprint = {}", String::from_utf8_lossy(&fingerprint(&mk)));
    let p = Policy::default_luds();
    println!("pw(mega.nz, me, 1) = {}", site_password(&mk, "mega.nz", "me", 1, &p).unwrap());
    let seed_paper = base32::to_paper(SEED);
    for l in &seed_paper {
        println!("seed-paper: {l}");
    }
}

// Значения из самого первого запуска.
const GOLDEN_MK: &str = "7c92b2aafa7d6f2c644f709bab0b6b2ffd8329ed5e12f66313cf4c9034c2625d";
const GOLDEN_FP: &str = "F5";
const GOLDEN_PW: &str = "t^QeMQf0a#*Tl24(mC$?";

#[test]
fn golden_master_key() {
    assert_eq!(hex(&mk()), GOLDEN_MK);
}

#[test]
fn golden_fingerprint() {
    assert_eq!(String::from_utf8_lossy(&fingerprint(&mk())), GOLDEN_FP);
}

#[test]
fn golden_site_password() {
    let p = Policy::default_luds();
    assert_eq!(site_password(&mk(), "mega.nz", "me", 1, &p).unwrap(), GOLDEN_PW);
}

#[test]
fn full_cycle_paper() {
    // Гоняем цикл целиком: сейф -> бумага -> переписали с листка -> обратно в сейф.
    let mk = mk();
    let entries = vec![
        Entry::Totp { label: "gh".into(), secret: b"12345678901234567890".to_vec(), digits8: false, period: 30 },
        Entry::Codes { label: "goog".into(), codes: vec!["12345678".into(), "00001111".into()] },
        Entry::Note { label: "n".into(), text: "заметка".into() },
    ];
    let blob = encrypt(&mk, &entries, [0x42; 12]);
    let paper = base32::to_paper(&blob);
    // Как будто человек переписал от руки: строчные буквы и путаница похожих символов.
    let copied: Vec<String> = paper
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let m = l.to_lowercase().replace('0', "o").replace('1', "l");
            if m.starts_with("==") { m } else { format!("{:02}{}", i + 1, &m[2..]) }
        })
        .collect();
    let refs: Vec<&str> = copied.iter().map(|s| s.as_str()).collect();
    let restored = base32::from_paper(&refs).unwrap();
    assert_eq!(decrypt(&mk, &restored).unwrap(), entries);
}
