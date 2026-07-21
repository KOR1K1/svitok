//! Сколько времени занимает KDF. Запускать в release, иначе цифры бессмысленны:
//! cargo test -p svitok-core --release --test bench -- --nocapture
use svitok_core::kdf::{master_key, KdfParams};

#[test]
fn bench_default_kdf() {
    // Прогреваем кэш и страницы памяти.
    let _ = master_key(b"warmup", b"warmup", KdfParams { m_log2: 12, t_log2: 12 });
    let t = std::time::Instant::now();
    let mk = master_key(b"seed-bytes-01234", b"correct horse battery", KdfParams::DEFAULT);
    let dt = t.elapsed();
    println!("KDF {} : {:?}", KdfParams::DEFAULT.to_paper(), dt);
    assert_ne!(mk, [0u8; 32]);
}
