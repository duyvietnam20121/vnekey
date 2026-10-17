//! Vòng chạy chính của vnekey (tiến trình khay): hook bàn phím/chuột, theo dõi cửa sổ đang
//! focus, biểu tượng khay hệ thống và tự nạp lại cấu hình khi cửa sổ cài đặt lưu.
//!
//! Mọi thứ chạy trên một luồng có vòng lặp thông điệp; trạng thái nằm trong `thread_local`
//! nên không cần khoá. Hook phải trả về nhanh nên mọi việc nặng đều được tránh ở đây.

use std::cell::RefCell;
use std::process::Child;
use std::time::SystemTime;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use vnekey_config::{Config, HotkeyDetector, HotkeyEvent, Mods};
use vnekey_core::{Engine, InputMethod, Reaction};
use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, VIRTUAL_KEY, VK_CAPITAL, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetMessageW,
    HC_ACTION, KBDLLHOOKSTRUCT, KillTimer, LLKHF_INJECTED, MSG, PostQuitMessage, SetTimer,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::icon;
use crate::keys::{self, Key};
use crate::plan::SendOptions;
use crate::process;
use crate::send;

/// Chu kỳ của bộ đếm giờ (ms): xử lý sự kiện khay, trả lại Clipboard, kiểm tra file cấu hình.
const TICK_MS: u32 = 100;
/// Kiểm tra file cấu hình mỗi `CONFIG_CHECK_TICKS` chu kỳ.
const CONFIG_CHECK_TICKS: u32 = 5;

enum UiAction {
    Toggle,
    OpenSettings,
    Quit,
}

struct Tray {
    icon: TrayIcon,
    toggle_item: MenuItem,
    settings_item: MenuItem,
    quit_item: MenuItem,
    vi_icon: Icon,
    en_icon: Icon,
}

fn tooltip(vietnamese: bool) -> &'static str {
    if vietnamese { "vnekey: Tiếng Việt (V)" } else { "vnekey: English (E)" }
}

fn toggle_text(vietnamese: bool) -> &'static str {
    if vietnamese { "Chuyển sang tiếng Anh (E)" } else { "Chuyển sang tiếng Việt (V)" }
}

impl Tray {
    fn new(vietnamese: bool) -> Result<Self, String> {
        let make = |v: bool| {
            Icon::from_rgba(icon::render(v), icon::SIZE, icon::SIZE).map_err(|e| e.to_string())
        };
        let vi_icon = make(true)?;
        let en_icon = make(false)?;

        let toggle_item = MenuItem::new(toggle_text(vietnamese), true, None);
        let settings_item = MenuItem::new("Cài đặt...", true, None);
        let quit_item = MenuItem::new("Thoát", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &toggle_item,
            &settings_item,
            &PredefinedMenuItem::separator(),
            &quit_item,
        ])
        .map_err(|e| e.to_string())?;

        let current = if vietnamese { vi_icon.clone() } else { en_icon.clone() };
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(tooltip(vietnamese))
            .with_icon(current)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { icon, toggle_item, settings_item, quit_item, vi_icon, en_icon })
    }

    fn show(&self, vietnamese: bool) {
        let icon = if vietnamese { &self.vi_icon } else { &self.en_icon };
        let _ = self.icon.set_icon(Some(icon.clone()));
        let _ = self.icon.set_tooltip(Some(tooltip(vietnamese)));
        self.toggle_item.set_text(toggle_text(vietnamese));
    }

    /// Lấy các thao tác người dùng vừa chọn trên menu hoặc biểu tượng khay.
    fn poll(&self) -> Vec<UiAction> {
        let mut actions = Vec::new();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == *self.toggle_item.id() {
                actions.push(UiAction::Toggle);
            } else if event.id == *self.settings_item.id() {
                actions.push(UiAction::OpenSettings);
            } else if event.id == *self.quit_item.id() {
                actions.push(UiAction::Quit);
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => actions.push(UiAction::Toggle),
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                    actions.push(UiAction::OpenSettings);
                }
                _ => {}
            }
        }
        actions
    }
}

struct Runtime {
    config: Config,
    config_mtime: Option<SystemTime>,
    engine: Engine,
    detector: HotkeyDetector,
    /// `true`: đang gõ tiếng Việt (V); `false`: tiếng Anh (E).
    vietnamese: bool,
    foreground_exe: String,
    /// Ứng dụng đang focus nằm trong danh sách loại trừ.
    excluded: bool,
    tray: Option<Tray>,
    settings: Option<Child>,
    ticks: u32,
    shown_icon: Option<bool>,
}

impl Runtime {
    fn new(config: Config) -> Self {
        let vietnamese = config.start_vietnamese;
        let config_mtime = Config::modified();
        let mut engine = Engine::new(config.engine_config());
        engine.set_abbreviations(config.abbreviation_map());
        Self {
            config,
            config_mtime,
            engine,
            detector: HotkeyDetector::default(),
            vietnamese,
            foreground_exe: String::new(),
            excluded: false,
            tray: None,
            settings: None,
            ticks: 0,
            shown_icon: None,
        }
    }

    fn apply_config(&mut self, config: Config) {
        self.engine.set_config(config.engine_config());
        self.engine.set_abbreviations(config.abbreviation_map());
        self.excluded = config.is_excluded(&self.foreground_exe);
        self.detector = HotkeyDetector::default();
        self.config = config;
    }

    fn toggle(&mut self) {
        self.vietnamese = !self.vietnamese;
        self.engine.cancel_context();
    }

    fn on_foreground(&mut self, hwnd: HWND) {
        self.engine.cancel_context();
        self.foreground_exe = process::exe_of_window(hwnd).unwrap_or_default();
        self.excluded = self.config.is_excluded(&self.foreground_exe);
    }

    fn send_options(&self) -> SendOptions {
        SendOptions {
            use_clipboard: self.config.use_clipboard,
            select_with_shift: self.config.select_with_shift,
        }
    }

    /// Trả về `true` nếu cần nuốt phím (ứng dụng không được thấy phím gốc).
    fn on_key(&mut self, vk: u32, is_down: bool) -> bool {
        let mods = read_mods(vk, is_down);
        match self.detector.feed(&self.config.hotkey, vk, is_down, mods) {
            HotkeyEvent::None => {}
            HotkeyEvent::Swallow => return true,
            HotkeyEvent::Toggle { swallow } => {
                self.toggle();
                if swallow {
                    return true;
                }
            }
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
        let key = keys::classify(vk, mods.shift, caps_lock_on(), bracket);
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
                send::replace(backspaces, &text, self.send_options());
                if let Some(vk) = resend {
                    send::tap_key(vk);
                }
                true
            }
        }
    }

    fn tick(&mut self) {
        send::tick_clipboard();

        self.ticks += 1;
        if self.ticks >= CONFIG_CHECK_TICKS {
            self.ticks = 0;
            let modified = Config::modified();
            if modified != self.config_mtime {
                // Nếu chưa đọc được (đang ghi dở) thì thử lại ở chu kỳ sau.
                if let Some(config) = Config::try_load() {
                    self.config_mtime = modified;
                    self.apply_config(config);
                }
            }
        }

        let actions = self.tray.as_ref().map(Tray::poll).unwrap_or_default();
        for action in actions {
            match action {
                UiAction::Toggle => self.toggle(),
                UiAction::OpenSettings => process::open_settings(&mut self.settings),
                UiAction::Quit => unsafe { PostQuitMessage(0) },
            }
        }

        if self.shown_icon != Some(self.vietnamese) {
            if let Some(tray) = &self.tray {
                tray.show(self.vietnamese);
            }
            self.shown_icon = Some(self.vietnamese);
        }
    }
}

thread_local! {
    static RT: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Chạy `f` nếu trạng thái chưa bị mượn (tránh lỗi khi hook gọi lồng nhau).
fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> Option<R> {
    RT.with(|cell| cell.try_borrow_mut().ok().and_then(|mut guard| guard.as_mut().map(f)))
}

fn caps_lock_on() -> bool {
    (unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) } & 1) != 0
}

/// Đọc trạng thái phím bổ trợ. Với phím đang xử lý dùng chính sự kiện (vì
/// GetAsyncKeyState trong hook cấp thấp chưa phản ánh sự kiện này).
fn read_mods(event_vk: u32, is_down: bool) -> Mods {
    let pressed = |pair: [VIRTUAL_KEY; 2]| {
        pair.iter().any(|v| {
            if u32::from(v.0) == event_vk {
                is_down
            } else {
                let state = unsafe { GetAsyncKeyState(i32::from(v.0)) };
                state < 0
            }
        })
    };
    Mods {
        ctrl: pressed([VK_LCONTROL, VK_RCONTROL]),
        shift: pressed([VK_LSHIFT, VK_RSHIFT]),
        alt: pressed([VK_LMENU, VK_RMENU]),
        win: pressed([VK_LWIN, VK_RWIN]),
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0 || kb.dwExtraInfo == send::MAGIC;
        let msg = wparam.0 as u32;
        let is_down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
        let is_up = matches!(msg, WM_KEYUP | WM_SYSKEYUP);
        if !injected && (is_down || is_up) {
            let swallow = with_runtime(|rt| rt.on_key(kb.vkCode, is_down)).unwrap_or(false);
            if swallow {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Nhấp chuột đổi vị trí con trỏ nên phải bỏ từ đang gõ dở, nếu không sẽ xoá nhầm chữ.
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let msg = wparam.0 as u32;
        if matches!(msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
            let _ = with_runtime(|rt| rt.engine.cancel_context());
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Đổi cửa sổ focus: bỏ từ đang gõ và cập nhật ứng dụng đang dùng (để áp dụng danh sách loại trừ).
unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    let _ = with_runtime(|rt| rt.on_foreground(hwnd));
}

unsafe extern "system" fn timer_proc(_hwnd: HWND, _msg: u32, _id: usize, _time: u32) {
    let _ = with_runtime(Runtime::tick);
}

/// Chạy bộ gõ cho đến khi người dùng chọn "Thoát" ở menu khay.
pub fn run() -> Result<(), String> {
    let first_run = !Config::exists();
    let mut runtime = Runtime::new(Config::load());
    runtime.tray = Some(Tray::new(runtime.vietnamese)?);
    runtime.shown_icon = Some(runtime.vietnamese);
    if first_run {
        process::open_settings(&mut runtime.settings);
    }
    runtime.on_foreground(unsafe { GetForegroundWindow() });
    RT.with(|cell| *cell.borrow_mut() = Some(runtime));

    let hmod: HINSTANCE = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?.into();
    let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0) }
        .map_err(|e| format!("Không cài được hook bàn phím: {e}"))?;
    // Hook chuột và theo dõi cửa sổ chỉ để reset từ; lỗi thì bỏ qua.
    let mouse = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0) }.ok();
    let win_event = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            HMODULE::default(),
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    let timer = unsafe { SetTimer(None, 0, TICK_MS, Some(timer_proc)) };

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if timer != 0 {
            let _ = KillTimer(None, timer);
        }
        if !win_event.is_invalid() {
            let _ = UnhookWinEvent(win_event);
        }
        if let Some(hook) = mouse {
            let _ = UnhookWindowsHookEx(hook);
        }
        let _ = UnhookWindowsHookEx(keyboard);
    }
    // Giải phóng trạng thái (gỡ biểu tượng khay).
    RT.with(|cell| cell.borrow_mut().take());
    Ok(())
}