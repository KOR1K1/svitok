//! Общий слой поверх svitok-core для приложений на std (CLI и GUI).
//! Тут только файлы и системный ГСЧ - никакой криптографии сверх ядра.
//! Секретов на диске нет: sites.txt - метаданные, vault.b32 - шифртекст.

pub mod lockmem;
pub mod osrng;
pub mod qr;
pub mod store;
