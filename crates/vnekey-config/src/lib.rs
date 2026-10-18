//! Cấu hình người dùng của vnekey, lưu tại `%APPDATA%\vnekey\config.toml`.
//!
//! Crate này không phụ thuộc Windows nên kiểm thử được ở mọi nền tảng.

mod hotkey;

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use vnekey_core::{Charset, EngineConfig, InputMethod};

pub use hotkey::{HotkeyDetector, HotkeyEvent, Mods, is_modifier_vk};

/// Phím chuyển chế độ Việt/Anh.
/// `key = None`: chỉ dùng tổ hợp phím bổ trợ (ví dụ Ctrl+Shift, nhả phím để chuyển).
/// `key = Some(c)`: tổ hợp có phím chính, `' '` là Space (ví dụ Alt+Z).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub key: Option<char>,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self { ctrl: true, shift: true, alt: false, win: false, key: None }
    }
}

impl Hotkey {
    pub fn has_modifier(&self) -> bool {
        self.ctrl || self.shift || self.alt || self.win
    }

    /// Các phím bổ trợ của tổ hợp (bỏ qua phím chính).
    pub fn mods(&self) -> Mods {
        Mods { ctrl: self.ctrl, shift: self.shift, alt: self.alt, win: self.win }
    }

    /// Ctrl + Space, mặc định của phím phục hồi từ.
    pub fn ctrl_space() -> Self {
        Self { ctrl: true, shift: false, alt: false, win: false, key: Some(' ') }
    }
}

/// Màu giao diện của cửa sổ cài đặt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // Bộ gõ
    pub method: InputMethod,
    pub charset: Charset,
    pub spell_check: bool,
    pub modern_tone: bool,
    pub allow_fjwz: bool,
    pub auto_capitalize: bool,
    pub bracket_horn: bool,
    // Gõ tắt
    pub abbreviations_enabled: bool,
    /// Vẫn mở rộng gõ tắt khi đang ở chế độ tiếng Anh.
    pub abbreviations_in_english: bool,
    // Cách gửi phím
    /// Dán qua Clipboard thay vì gõ từng ký tự (cho ứng dụng không nhận được ký tự Unicode).
    pub use_clipboard: bool,
    /// Chọn chữ bằng Shift+← rồi gõ đè, thay cho Backspace (sửa lỗi thanh địa chỉ Chrome/Edge).
    pub select_with_shift: bool,
    // Hệ thống
    pub start_vietnamese: bool,
    pub autostart: bool,
    pub run_as_admin: bool,
    /// Mở cửa sổ cài đặt mỗi lần khởi động (lần chạy đầu tiên luôn mở).
    pub show_settings_on_start: bool,
    /// Phát tiếng bíp khi chuyển Việt/Anh.
    pub beep_on_toggle: bool,
    /// Nhớ chế độ Việt/Anh riêng cho từng ứng dụng.
    pub per_app_state: bool,
    /// Bật phím phục hồi từ (đưa từ đang gõ về đúng các phím đã bấm).
    pub restore_enabled: bool,
    /// Bật phím tắt chức năng F1, F2, F3, F5, F9, F12 (cùng các phím bổ trợ `function_mods`).
    pub function_keys_enabled: bool,
    pub theme: Theme,
    /// Tên file .exe (không phân biệt hoa thường) mà vnekey không can thiệp.
    pub excluded_apps: Vec<String>,
    // TOML yêu cầu các bảng (table) nằm sau các giá trị đơn.
    pub hotkey: Hotkey,
    pub restore_hotkey: Hotkey,
    /// Chỉ dùng các phím bổ trợ; phím chính là F1-F12.
    pub function_mods: Hotkey,
    /// Bảng gõ tắt: từ gõ -> cụm mở rộng.
    pub abbreviations: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            method: InputMethod::Telex,
            charset: Charset::Unicode,
            spell_check: true,
            modern_tone: true,
            allow_fjwz: false,
            auto_capitalize: false,
            bracket_horn: false,
            abbreviations_enabled: false,
            abbreviations_in_english: false,
            use_clipboard: false,
            select_with_shift: false,
            start_vietnamese: true,
            autostart: false,
            run_as_admin: false,
            show_settings_on_start: false,
            beep_on_toggle: false,
            per_app_state: false,
            restore_enabled: false,
            function_keys_enabled: false,
            theme: Theme::Dark,
            excluded_apps: Vec::new(),
            hotkey: Hotkey::default(),
            restore_hotkey: Hotkey::ctrl_space(),
            function_mods: Hotkey::default(),
            abbreviations: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn engine_config(&self) -> EngineConfig {
        EngineConfig {
            method: self.method,
            charset: self.charset,
            spell_check: self.spell_check,
            modern_tone: self.modern_tone,
            allow_fjwz: self.allow_fjwz,
            auto_capitalize: self.auto_capitalize,
            bracket_horn: self.bracket_horn,
            abbreviations: self.abbreviations_enabled,
        }
    }

    pub fn abbreviation_map(&self) -> HashMap<String, String> {
        self.abbreviations.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// `exe` là tên file của ứng dụng đang focus (ví dụ `Code.exe`).
    pub fn is_excluded(&self, exe: &str) -> bool {
        let exe = normalize_exe(exe);
        !exe.is_empty() && self.excluded_apps.iter().any(|e| normalize_exe(e) == exe)
    }

    pub fn dir() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("vnekey"))
    }

    pub fn path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("config.toml"))
    }

    /// Chưa có file cấu hình nghĩa là chạy lần đầu.
    pub fn exists() -> bool {
        Self::path().is_some_and(|p| p.exists())
    }

    /// Thời điểm sửa file cấu hình, để tiến trình khay tự nạp lại khi cửa sổ cài đặt lưu.
    pub fn modified() -> Option<SystemTime> {
        std::fs::metadata(Self::path()?).ok()?.modified().ok()
    }

    /// Đọc cấu hình; `None` nếu thiếu file hoặc nội dung không hợp lệ.
    pub fn try_load() -> Option<Self> {
        let text = std::fs::read_to_string(Self::path()?).ok()?;
        toml::from_str(&text).ok()
    }

    /// Đọc cấu hình; thiếu file hoặc file hỏng thì dùng mặc định.
    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    /// Ghi cấu hình. Ghi ra file tạm rồi đổi tên để tiến trình khác không bao giờ đọc phải file ghi dở.
    pub fn save(&self) -> io::Result<()> {
        let path =
            Self::path().ok_or_else(|| io::Error::other("không xác định được thư mục cấu hình"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)
    }
}

/// Chuẩn hoá tên exe để so sánh: bỏ khoảng trắng, đường dẫn, viết thường.
pub fn normalize_exe(name: &str) -> String {
    let name = name.trim().trim_matches('"');
    let file = name.rsplit(['\\', '/']).next().unwrap_or(name);
    file.to_lowercase()
}

/// Đọc bảng gõ tắt từ văn bản, mỗi dòng `từ = cụm mở rộng`. Bỏ dòng trống và dòng bắt đầu bằng `#`.
pub fn parse_abbreviations(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let (key, value) = (key.trim(), value.trim());
            (!key.is_empty() && !value.is_empty() && !key.contains(char::is_whitespace))
                .then(|| (key.to_owned(), value.to_owned()))
        })
        .collect()
}

pub fn format_abbreviations(table: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (key, value) in table {
        out.push_str(&format!("{key} = {value}\n"));
    }
    out
}

/// Danh sách ứng dụng loại trừ, mỗi dòng một tên exe.
pub fn parse_exe_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let name = normalize_exe(line);
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.charset = Charset::Tcvn3;
        cfg.excluded_apps = vec!["game.exe".into()];
        cfg.abbreviations.insert("ko".into(), "không".into());
        cfg.hotkey.key = Some('Z');
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let cfg: Config = toml::from_str("spell_check = false").unwrap();
        assert!(!cfg.spell_check);
        assert!(cfg.modern_tone);
        assert_eq!(cfg.hotkey, Hotkey::default());
    }

    #[test]
    fn exe_names_are_normalized() {
        assert_eq!(normalize_exe(" C:\\Games\\Foo\\Game.EXE "), "game.exe");
        assert_eq!(normalize_exe("\"Code.exe\""), "code.exe");
        let cfg = Config { excluded_apps: vec!["Game.exe".into()], ..Config::default() };
        assert!(cfg.is_excluded("game.EXE"));
        assert!(!cfg.is_excluded("other.exe"));
        assert!(!cfg.is_excluded(""));
    }

    #[test]
    fn abbreviations_parse_and_format() {
        let table =
            parse_abbreviations("# ghi chú\nko = không\n\ndc=được\nbad line\n= x\nvn = Việt Nam");
        assert_eq!(table.len(), 3);
        assert_eq!(table["ko"], "không");
        assert_eq!(table["dc"], "được");
        assert_eq!(table["vn"], "Việt Nam");
        assert_eq!(parse_abbreviations(&format_abbreviations(&table)), table);
    }

    #[test]
    fn exe_list_deduplicates() {
        assert_eq!(parse_exe_list("A.exe\n\na.EXE\nb.exe\n"), vec!["a.exe", "b.exe"]);
    }
}
