//! Разок сгенерировать тестовый otpauth-QR, чтобы проверить, ловит ли его камера.
//! cargo test -p svitok-common --test genqr -- --nocapture

#[test]
fn gen_test_otpauth_qr() {
    let uri = "otpauth://totp/Svitok:test@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Svitok&digits=6&period=30";
    let svg = svitok_common::qr::to_svg(uri).unwrap();
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>Тестовый otpauth-QR</title>\
         <body style='margin:0;background:#fff;display:flex;flex-direction:column;\
         align-items:center;justify-content:center;height:100vh;font-family:sans-serif'>\
         <div style='width:340px'>{}</div>\
         <p style='color:#555'>Наведи телефон: Коды → + → Сканировать QR</p></body>",
        svg
    );
    std::fs::write("C:/Users/s7venteen/svitok/qr-test.html", html).unwrap();
    println!("wrote qr-test.html");
}
