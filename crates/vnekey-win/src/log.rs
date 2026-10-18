//! Luồng xử lý phím của vnekey, tách khỏi Windows để mô phỏng và kiểm thử được ở mọi nền tảng.
//!
//! `Controller` nhận từng sự kiện phím (mã VK + nhấn/nhả), quyết định có nuốt phím không và
//! gửi chữ thay thế qua [`Platform`]. Phần Windows chỉ cần hiện thực `Platform`.

use std::collections::HashMap;

use vnekey_config::{Config, HotkeyDetector, HotkeyEvent, Mods};
use vnekey_core::{Charset, Engine, InputMethod, Reaction};

use crate::keys::{self, Key};
use crate::plan::SendOptions;

/// Những thứ cần hệ điều hành làm hộ.
pub trait Platform {
    /// Trạng thái phím bổ trợ. Với phím đang xử lý, dùng chính sự kiện (`is_down`).
    fn mods(&mut self, vk: u32, is_down: bool) -> Mods;
    fn caps_lock(&mut self) -> bool;
    /// Xoá `backspaces` ký tự rồi gõ `text`.
    fn replace(&mut self, backspaces: usize, text: &str, opts: SendOptions);
    /// Gửi lại một phím (nhấn rồi nhả).
    fn tap_key(&mut self, vk: u32);
    fn open_settings(&mut self);
    /// Vừa đổi chế độ Việt/Anh.
    fn toggled(&mut self, vietnamese: bool);
}

pub struct Controller {
    config: Config,
    engine: Engine,
    toggle_detector: HotkeyDetector,
    restore_detector: HotkeyDetector,
    vietnamese: bool,
    foreground_exe: String,
    /// Ứng dụng đang focus nằm trong danh sách loại trừ.
    excluded: bool,
    /// Chế độ V/E nhớ riêng cho từng ứng dụng.
    app_states: HashMap<String, bool>,
    /// Phím chức năng vừa bị nuốt lúc nhấn, cần nuốt luôn lúc nhả.
    function_key_up: Option<u32>,
}

impl Controller {
    pub fn new(config: Config) -> Self {
        let mut engine = Engine::new(config.engine_config());
        engine.set_abbreviations(config.abbreviation_map());
        Self {
            vietnamese: config.start_vietnamese,
            config,
            engine,
            toggle_detector: HotkeyDetector::default(),
            restore_detector: HotkeyDetector::default(),
            foreground_exe: String::new(),
            excluded: false,
            app_states: HashMap::new(),
            function_key_up: None,
        }
    }

    pub fn vietnamese(&self) -> bool {
        self.vietnamese
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn foreground_exe(&self) -> &str {
        &self.foreground_exe
    }

    pub fn is_excluded(&self) -> bool {
        self.excluded
    }

    pub fn apply_config(&mut self, config: Config) {
        self.engine.set_config(config.engine_config());
        self.engine.set_abbreviations(config.abbreviation_map());
        self.excluded = config.is_excluded(&self.foreground_exe);
        self.toggle_detector = HotkeyDetector::default();
        self.restore_detector = HotkeyDetector::default();
        if !config.per_app_state {
            self.app_states.clear();
        }
        self.config = config;
    }

    pub fn toggle(&mut self) {
        self.set_vietnamese(!self.vietnamese);
    }

    pub fn set_vietnamese(&mut self, on: bool) {
        self.vietnamese = on;
        self.engine.cancel_context();
        if self.config.per_app_state && !self.foreground_exe.is_empty() {
            self.app_states.insert(self.foreground_exe.clone(), on);
        }
    }

    /// Cửa sổ focus đổi sang ứng dụng `exe` (tên file, viết thường).
    pub fn set_foreground(&mut self, exe: &str) {
        self.engine.cancel_context();
        self.foreground_exe = exe.to_owned();
        self.excluded = self.config.is_excluded(exe);
        if self.config.per_app_state && !exe.is_empty() {
            self.vietnamese =
                self.app_states.get(exe).copied().unwrap_or(self.config.start_vietnamese);
        }
    }

    /// Bỏ từ đang gõ dở và trạng thái đầu câu (nhấp chuột, ...).
    pub fn cancel_context(&mut self) {
        self.engine.cancel_context();
    }

    fn send_options(&self) -> SendOptions {
        SendOptions {
            use_clipboard: self.config.use_clipboard,
            select_with_shift: self.config.select_with_shift,
        }
    }

    /// Trả về `true` nếu cần nuốt phím (ứng dụng không được thấy phím gốc).
    pub fn on_key<P: Platform>(&mut self, p: &mut P, vk: u32, is_down: bool) -> bool {
        let mods = p.mods(vk, is_down);

        match self.toggle_detector.feed(&self.config.hotkey, vk, is_down, mods) {
            HotkeyEvent::None => {}
            HotkeyEvent::Swallow => return true,
            HotkeyEvent::Toggle { swallow } => {
                self.toggle();
                p.toggled(self.vietnamese);
                if swallow {
                    return true;
                }
            }
        }

        if self.config.restore_enabled {
            match self.restore_detector.feed(&self.config.restore_hotkey, vk, is_down, mods) {
                HotkeyEvent::None => {}
                HotkeyEvent::Swallow => return true,
                HotkeyEvent::Toggle { swallow } => {
                    self.restore_word(p);
                    if swallow {
                        return true;
                    }
                }
            }
        }

        if self.config.function_keys_enabled && self.function_key(p, vk, is_down, mods) {
            return true;
        }

        if !is_down || self.excluded {
            return false;
        }
        // Ở chế độ tiếng Anh vẫn theo dõi chữ gõ nếu người dùng muốn gõ tắt hoạt động.
        let track_english =
            self.config.abbreviations_enabled && self.config.abbreviations_in_english;
        if !self.vietnamese && !track_english {
            return false;
        }

        let bracket =
            self.vietnamese && self.config.bracket_horn && self.config.method == InputMethod::Telex;
        let key = keys::classify(vk, mods.shift, p.caps_lock(), bracket);
        if matches!(key, Key::Modifier) {
            return false;
        }
        // Ctrl/Alt/Win + phím là phím tắt của ứng dụng: kết thúc từ, không can thiệp.
        if mods.ctrl || mods.alt || mods.win {
            self.engine.cancel_context();
            return false;
        }

        let (reaction, resend) = match key {
            Key::Modifier => return false,
            Key::Backspace => (self.engine.backspace(), None),
            Key::Char(c) if self.vietnamese => (self.engine.process(c), None),
            Key::Char(c) => (self.engine.process_plain(c), None),
            // Phím kết thúc từ bị nuốt khi mở rộng gõ tắt nên phải gửi lại sau phần thay thế.
            Key::Boundary(b) => (self.engine.boundary(b), Some(vk)),
        };
        match reaction {
            Reaction::Pass => false,
            Reaction::Replace { backspaces, text } => {
                p.replace(backspaces, &text, self.send_options());
                if let Some(vk) = resend {
                    p.tap_key(vk);
                }
                true
            }
        }
    }

    fn restore_word<P: Platform>(&mut self, p: &mut P) {
        if self.excluded {
            return;
        }
        if let Reaction::Replace { backspaces, text } = self.engine.restore_word() {
            p.replace(backspaces, &text, self.send_options());
        }
    }

    /// Phím tắt chức năng: F1 bật tiếng Việt, F2 tắt, F3 bảng mã Unicode, F5 mở cài đặt,
    /// F9 bật/tắt gõ tắt, F12 bỏ từ đang gõ.
    fn function_key<P: Platform>(&mut self, p: &mut P, vk: u32, is_down: bool, mods: Mods) -> bool {
        if !is_down {
            if self.function_key_up == Some(vk) {
                self.function_key_up = None;
                return true;
            }
            return false;
        }
        let wanted = self.config.function_mods;
        if !(0x70..=0x7B).contains(&vk) || !wanted.has_modifier() || mods != wanted.mods() {
            return false;
        }
        match vk {
            0x70 => {
                self.set_vietnamese(true);
                p.toggled(true);
            }
            0x71 => {
                self.set_vietnamese(false);
                p.toggled(false);
            }
            0x72 => {
                self.config.charset = Charset::Unicode;
                self.engine.set_config(self.config.engine_config());
            }
            0x74 => p.open_settings(),
            0x78 => {
                self.config.abbreviations_enabled = !self.config.abbreviations_enabled;
                self.engine.set_config(self.config.engine_config());
            }
            0x7B => self.engine.cancel_context(),
            _ => return false,
        }
        self.function_key_up = Some(vk);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use vnekey_config::Hotkey;

    use super::*;

    const CTRL: u32 = 0xA2;
    const SHIFT: u32 = 0xA0;
    const ALT: u32 = 0xA4;
    const SPACE: u32 = 0x20;

    /// Bàn phím và màn hình giả: ghi lại chữ mà ứng dụng sẽ nhận được.
    #[derive(Default)]
    struct Fake {
        down: HashSet<u32>,
        caps: bool,
        screen: String,
        settings_opened: u32,
        toggles: Vec<bool>,
    }

    impl Platform for Fake {
        fn mods(&mut self, vk: u32, is_down: bool) -> Mods {
            let mut set = self.down.clone();
            if is_down {
                set.insert(vk);
            } else {
                set.remove(&vk);
            }
            let any = |a: u32, b: u32| set.contains(&a) || set.contains(&b);
            Mods {
                ctrl: any(0xA2, 0xA3),
                shift: any(0xA0, 0xA1),
                alt: any(0xA4, 0xA5),
                win: any(0x5B, 0x5C),
            }
        }
        fn caps_lock(&mut self) -> bool {
            self.caps
        }
        fn replace(&mut self, backspaces: usize, text: &str, _opts: SendOptions) {
            for _ in 0..backspaces {
                self.screen.pop();
            }
            self.screen.push_str(text);
        }
        fn tap_key(&mut self, vk: u32) {
            self.app_receives(vk);
        }
        fn open_settings(&mut self) {
            self.settings_opened += 1;
        }
        fn toggled(&mut self, vietnamese: bool) {
            self.toggles.push(vietnamese);
        }
    }

    impl Fake {
        /// Ứng dụng nhận một phím nhấn xuống và hiện chữ tương ứng.
        fn app_receives(&mut self, vk: u32) {
            let m = self.mods(vk, true);
            if m.ctrl || m.alt || m.win {
                return;
            }
            match keys::classify(vk, m.shift, self.caps, false) {
                Key::Char(c) => self.screen.push(c),
                Key::Backspace => {
                    self.screen.pop();
                }
                Key::Boundary(vnekey_core::Boundary::Space) => self.screen.push(' '),
                Key::Boundary(vnekey_core::Boundary::Enter) => self.screen.push('\n'),
                Key::Boundary(vnekey_core::Boundary::Punct(c)) => self.screen.push(c),
                _ => {}
            }
        }

        fn press(&mut self, c: &mut Controller, vk: u32) {
            let swallowed = c.on_key(self, vk, true);
            self.down.insert(vk);
            if !swallowed {
                self.app_receives(vk);
            }
        }

        fn release(&mut self, c: &mut Controller, vk: u32) {
            c.on_key(self, vk, false);
            self.down.remove(&vk);
        }

        fn tap(&mut self, c: &mut Controller, vk: u32) {
            self.press(c, vk);
            self.release(c, vk);
        }

        /// Gõ một chuỗi chữ thường, số và khoảng trắng; chữ hoa giữ Shift.
        fn type_str(&mut self, c: &mut Controller, text: &str) {
            for ch in text.chars() {
                let vk = match ch {
                    ' ' => SPACE,
                    '0'..='9' => ch as u32,
                    _ => ch.to_ascii_uppercase() as u32,
                };
                let shifted = ch.is_ascii_uppercase();
                if shifted {
                    self.press(c, SHIFT);
                }
                self.tap(c, vk);
                if shifted {
                    self.release(c, SHIFT);
                }
            }
        }
    }

    fn typed(config: Config, text: &str) -> String {
        let (mut c, mut f) = (Controller::new(config), Fake::default());
        f.type_str(&mut c, text);
        f.screen
    }

    #[test]
    fn vietnamese_typing_works_through_the_key_pipeline() {
        assert_eq!(typed(Config::default(), "vieetj"), "việt");
        assert_eq!(typed(Config::default(), "xin chaof cacs banj"), "xin chào các bạn");
        assert_eq!(typed(Config::default(), "nguwowif"), "người");
    }

    #[test]
    fn shift_and_capslock_choose_the_case() {
        assert_eq!(typed(Config::default(), "Vieetj"), "Việt");
        let (mut c, mut f) = (Controller::new(Config::default()), Fake::default());
        f.caps = true;
        f.type_str(&mut c, "vieetj");
        assert_eq!(f.screen, "VIỆT");
    }

    #[test]
    fn ctrl_shift_toggles_to_english_and_back() {
        let (mut c, mut f) = (Controller::new(Config::default()), Fake::default());
        f.press(&mut c, CTRL);
        f.press(&mut c, SHIFT);
        f.release(&mut c, SHIFT);
        f.release(&mut c, CTRL);
        assert!(!c.vietnamese());
        assert_eq!(f.toggles, vec![false]);
        f.type_str(&mut c, "vieetj");
        assert_eq!(f.screen, "vieetj");

        // Nhấn Shift trước rồi Ctrl cũng chuyển được.
        f.press(&mut c, SHIFT);
        f.press(&mut c, CTRL);
        f.release(&mut c, CTRL);
        f.release(&mut c, SHIFT);
        assert!(c.vietnamese());
        f.screen.clear();
        f.type_str(&mut c, "vieetj");
        assert_eq!(f.screen, "việt");
    }

    #[test]
    fn ctrl_shift_with_another_key_does_not_toggle() {
        let (mut c, mut f) = (Controller::new(Config::default()), Fake::default());
        f.press(&mut c, CTRL);
        f.press(&mut c, SHIFT);
        f.tap(&mut c, 0x54); // Ctrl+Shift+T
        f.release(&mut c, SHIFT);
        f.release(&mut c, CTRL);
        assert!(c.vietnamese());
        assert!(f.toggles.is_empty());
    }

    #[test]
    fn alt_z_hotkey_toggles_and_hides_the_key() {
        let config = Config {
            hotkey: Hotkey { ctrl: false, shift: false, alt: true, win: false, key: Some('Z') },
            ..Config::default()
        };
        let (mut c, mut f) = (Controller::new(config), Fake::default());
        f.press(&mut c, ALT);
        f.press(&mut c, 0x5A);
        f.release(&mut c, 0x5A);
        f.release(&mut c, ALT);
        assert!(!c.vietnamese());
        assert_eq!(f.screen, "");
    }

    #[test]
    fn excluded_app_is_left_alone() {
        let config = Config { excluded_apps: vec!["game.exe".into()], ..Config::default() };
        let (mut c, mut f) = (Controller::new(config), Fake::default());
        c.set_foreground("game.exe");
        f.type_str(&mut c, "vieetj");
        assert_eq!(f.screen, "vieetj");
        c.set_foreground("notepad.exe");
        f.screen.clear();
        f.type_str(&mut c, "vieetj");
        assert_eq!(f.screen, "việt");
    }

    #[test]
    fn ctrl_shortcuts_pass_through_and_end_the_word() {
        let (mut c, mut f) = (Controller::new(Config::default()), Fake::default());
        f.type_str(&mut c, "vie");
        f.press(&mut c, CTRL);
        f.tap(&mut c, 0x41); // Ctrl+A: ứng dụng không nhận chữ
        f.release(&mut c, CTRL);
        assert_eq!(f.screen, "vie");
        f.type_str(&mut c, "e"); // từ mới, không biến "vie" thành "viê"
        assert_eq!(f.screen, "viee");
    }

    #[test]
    fn abbreviations_expand_and_resend_the_boundary_key() {
        let mut config = Config { abbreviations_enabled: true, ..Config::default() };
        config.abbreviations.insert("ko".into(), "không".into());
        assert_eq!(typed(config.clone(), "ko dc"), "không dc");
        assert_eq!(typed(config.clone(), "ko "), "không ");
        assert_eq!(typed(config, "Ko "), "Không ");
    }

    #[test]
    fn abbreviations_can_work_in_english_mode() {
        let mut config = Config {
            abbreviations_enabled: true,
            abbreviations_in_english: true,
            start_vietnamese: false,
            ..Config::default()
        };
        config.abbreviations.insert("ko".into(), "không".into());
        assert_eq!(typed(config.clone(), "ko "), "không ");
        // Không bật tuỳ chọn thì tiếng Anh giữ nguyên.
        config.abbreviations_in_english = false;
        assert_eq!(typed(config, "ko "), "ko ");
    }

    #[test]
    fn per_app_state_is_remembered() {
        let config = Config { per_app_state: true, ..Config::default() };
        let mut c = Controller::new(config);
        c.set_foreground("a.exe");
        c.toggle(); // a.exe: E
        c.set_foreground("b.exe");
        assert!(c.vietnamese(), "ứng dụng mới bắt đầu bằng chế độ mặc định");
        c.set_foreground("a.exe");
        assert!(!c.vietnamese());
        c.set_foreground("b.exe");
        assert!(c.vietnamese());
    }

    #[test]
    fn state_is_global_without_per_app_option() {
        let mut c = Controller::new(Config::default());
        c.set_foreground("a.exe");
        c.toggle();
        c.set_foreground("b.exe");
        assert!(!c.vietnamese());
    }

    #[test]
    fn restore_hotkey_returns_the_word_to_typed_keys() {
        let config = Config { restore_enabled: true, ..Config::default() };
        let (mut c, mut f) = (Controller::new(config), Fake::default());
        f.type_str(&mut c, "vieetj");
        assert_eq!(f.screen, "việt");
        f.press(&mut c, CTRL);
        f.tap(&mut c, SPACE);
        f.release(&mut c, CTRL);
        assert_eq!(f.screen, "vieetj");
    }

    #[test]
    fn function_keys_switch_mode_and_open_settings() {
        let config = Config { function_keys_enabled: true, ..Config::default() };
        let (mut c, mut f) = (Controller::new(config), Fake::default());
        let with_mods = |f: &mut Fake, c: &mut Controller, vk: u32| {
            f.press(c, CTRL);
            f.press(c, SHIFT);
            f.tap(c, vk);
            f.release(c, SHIFT);
            f.release(c, CTRL);
        };
        with_mods(&mut f, &mut c, 0x71); // Ctrl+Shift+F2: tắt tiếng Việt
        assert!(!c.vietnamese());
        with_mods(&mut f, &mut c, 0x70); // F1: bật
        assert!(c.vietnamese());
        with_mods(&mut f, &mut c, 0x74); // F5: mở cài đặt
        assert_eq!(f.settings_opened, 1);
        // Không đủ phím bổ trợ thì F-key đi qua bình thường.
        f.tap(&mut c, 0x71);
        assert!(c.vietnamese());
    }

    #[test]
    fn typing_survives_a_config_reload() {
        let (mut c, mut f) = (Controller::new(Config::default()), Fake::default());
        f.type_str(&mut c, "vie");
        c.apply_config(Config { method: InputMethod::Vni, ..Config::default() });
        f.screen.clear();
        f.type_str(&mut c, "viet65");
        assert_eq!(f.screen, "việt");
    }
}
