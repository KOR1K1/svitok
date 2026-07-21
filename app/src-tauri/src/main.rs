// в релизе на Windows не показываем консольное окно
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    svitok_app_lib::run()
}
