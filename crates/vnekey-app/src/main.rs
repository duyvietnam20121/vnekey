//! vnekey: bộ gõ tiếng Việt cho Windows.
//!
//! Hai chế độ chạy (cùng một file .exe):
//! - mặc định: tiến trình khay, giữ hook bàn phím và biểu tượng khay hệ thống;
//! - `--settings`: cửa sổ cài đặt, ghi `config.toml`, tiến trình khay tự nạp lại.
//!
//! Cờ `--elevated` đánh dấu bản đã được chạy lại với quyền admin. Cờ `--debug` (hoặc biến môi
//! trường `VNEKEY_DEBUG`) ghi nhật ký từng phím vào `%APPDATA%\vnekey\vnekey.log`.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod ui;

#[cfg(windows)]
fn main() {
    use vnekey_win::process;

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--settings") {
        if let Err(e) = ui::run() {
            process::report_error(&format!("Không mở được cửa sổ cài đặt: {e}"));
        }
        return;
    }

    let elevated = args.iter().any(|a| a == "--elevated");
    if vnekey_config::Config::load().run_as_admin && !elevated && process::relaunch_elevated() {
        return;
    }
    let Some(_instance) = process::SingleInstance::acquire() else {
        // Đã có một bản đang chạy: mở cửa sổ cài đặt rồi thoát.
        let _ = process::spawn_settings();
        return;
    };
    let debug = args.iter().any(|a| a == "--debug") || std::env::var_os("VNEKEY_DEBUG").is_some();
    if let Err(e) = vnekey_win::runtime::run(debug) {
        process::report_error(&e);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("vnekey chỉ chạy trên Windows.");
}
