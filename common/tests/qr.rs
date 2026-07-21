//! Прогоняем наш QR-энкодер через сторонний декодер rqrr.
//! Раз он читает то, что мы закодировали, значит матрица собрана по стандарту:
//! формат, маски, Рид-Соломон, перемежение, размещение - всё на месте.

use svitok_common::qr;

fn render_gray(m: &[Vec<bool>], scale: u32) -> image::GrayImage {
    let size = m.len() as u32;
    let quiet = 4 * scale;
    let total = size * scale + quiet * 2;
    let mut img = image::GrayImage::from_pixel(total, total, image::Luma([255u8]));
    for (r, row) in m.iter().enumerate() {
        for (c, &dark) in row.iter().enumerate() {
            if dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(
                            quiet + c as u32 * scale + dx,
                            quiet + r as u32 * scale + dy,
                            image::Luma([0u8]),
                        );
                    }
                }
            }
        }
    }
    img
}

fn roundtrip(text: &str) {
    let m = qr::matrix(text.as_bytes()).expect("encode");
    let img = render_gray(&m, 4);
    let mut prepared = rqrr::PreparedImage::prepare(img);
    let grids = prepared.detect_grids();
    assert_eq!(grids.len(), 1, "QR не найден для «{}» (размер {})", text, m.len());
    let (_meta, content) = grids[0].decode().expect("decode");
    assert_eq!(content, text, "декодировано неверно");
}

#[test]
fn short_password() {
    roundtrip("p.h39}nqSz[35pucZDgY");
}

#[test]
fn version_boundaries() {
    // Проверяем стыки версий: 14 (v1), 15 (v2), 26/27, 42/43, 213 (потолок v10).
    for len in [1usize, 14, 15, 26, 27, 42, 43, 84, 106, 152, 180, 213] {
        let s: String = (0..len).map(|i| (b'!' + (i % 90) as u8) as char).collect();
        roundtrip(&s);
    }
}

#[test]
fn utf8_cyrillic() {
    roundtrip("пароль от mega.nz — тест");
}

#[test]
fn paper_line() {
    roundtrip("01 PRYK FT1J 8E5R SZXY H");
}

#[test]
fn large_versions() {
    // Длины из старших версий (v11-v40) и их границы блоков.
    // Прогон через сторонний декодер заодно проверяет таблицы EC и центры
    // выравнивающих узоров для каждой версии.
    for len in [214usize, 251, 252, 320, 450, 620, 850, 1100, 1400, 1700, 2000, 2331] {
        let s: String = (0..len).map(|i| (b'!' + (i % 90) as u8) as char).collect();
        roundtrip(&s);
    }
}

#[test]
fn too_long_rejected() {
    let s = "x".repeat(qr::MAX_BYTES + 1);
    assert!(qr::matrix(s.as_bytes()).is_err());
    // А ровно максимум - кодируется.
    let ok = "y".repeat(qr::MAX_BYTES);
    assert!(qr::matrix(ok.as_bytes()).is_ok());
}

#[test]
fn svg_output_sane() {
    let svg = qr::to_svg("test").unwrap();
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("path"));
}
