package app.svitok.vault

// Прямой мост к ядру Rust (libsvitok_app_lib) для компонентов вне Tauri-активити,
// в первую очередь AutofillService. Символы экспортируются из src/jni_autofill.rs.
object Native {
    init {
        System.loadLibrary("svitok_app_lib")
    }

    // Registrable domain (eTLD+1) по встроенному PSL. Пусто, если не сводится.
    external fun canonicalDomain(input: String): String

    // Полная деривация одного пароля: сид (hex, после биометрии), фраза,
    // параметры KDF и строка сайта из списка. KDF тяжёлый - только с фонового потока.
    external fun derivePassword(seedHex: String, phrase: String, m: Int, t: Int, siteLine: String): String
}
