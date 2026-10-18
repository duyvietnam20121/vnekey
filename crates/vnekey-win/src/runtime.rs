//! Vòng chạy chính của vnekey (tiến trình khay): hook bàn phím/chuột, theo dõi cửa sổ đang
//! focus, biểu tượng khay hệ thống và tự nạp lại cấu hình khi cửa sổ cài đặt lưu.
//!
//! Logic xử lý phím nằm ở [`Controller`] (kiểm thử được ở mọi nền tảng); module này chỉ nối
//! Controller với Windows. Mọi thứ chạy trên một luồng có vòng lặp thông điệp nên trạng thái
//! nằm trong `thread_local`, không cần khoá. Hook phải trả về nhanh.

use std::cell::RefCell;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use vnekey_config::{Config, Mods};
use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, VIRTUAL_KEY, VK_CAPITAL, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetMessageW,
    HC_ACTION, KBDLLHOOKSTRUCT, KillTimer, LLKHF_INJECTED, MB_ICONASTERISK, MB_ICONEXCLAMATION,
    MSG, MessageBeep, PostQuitMessage, SetTimer, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::controller::{Controller, Platform};
use crate::plan::SendOptions;
use crate::{icon, log, process, send};

/// Chu kỳ của bộ đếm giờ (ms): xử lý sự kiện khay, trả lại Clipboard, kiểm tra file cấu hình.
const TICK_MS: u32 = 100;
/// Kiểm tra file cấu hình mỗi `CONFIG_CHECK_TICKS` chu kỳ.
const CONFIG_CHECK_TICKS: u32 = 5;

/// Đã ghi nhật ký lần đầu hook nhận được phím chưa (để biết hook có hoạt động không).
static FIRST_KEY_LOGGED: AtomicBool = AtomicBool::new(false);

enum UiAction {
    Toggle,
    OpenSettings,
    OpenLog,
    Quit,
}

struct Tray {
    icon: TrayIcon,
    toggle_item: MenuItem,
    settings_item: MenuItem,
    log_item: MenuItem,
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
        let log_item = MenuItem::new("Xem nhật ký", true, None);
        let quit_item = MenuItem::new("Thoát", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &toggle_item,
            &settings_item,
            &log_item,
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
        Ok(Self { icon, toggle_item, settings_item, log_item, quit_item, vi_icon, en_icon })
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
            } else if event.id == *self.log_item.id() {
                actions.push(UiAction::OpenLog);
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

/// Phần Windows của [`Platform`].
struct WinPlatform<'a> {
    settings: &'a mut Option<Child>,
    beep: bool,
}

impl Platform for WinPlatform<'_> {
    fn mods(&mut self, vk: u32, is_down: bool) -> Mods {
        read_mods(vk, is_down)
    }

    fn caps_lock(&mut self) -> bool {
        caps_lock_on()
    }

    fn replace(&mut self, backspaces: usize, text: &str, opts: SendOptions) {
        send::replace(backspaces, text, opts);
    }

    fn tap_key(&mut self, vk: u32) {
        send::tap_key(vk);
    }

    fn open_settings(&mut self) {
        process::open_settings(self.settings);
    }

    fn toggled(&mut self, vietnamese: bool) {
        notify_toggle(vietnamese, self.beep, "phím tắt");
    }
}

fn notify_toggle(vietnamese: bool, beep: bool, source: &str) {
    log::info(&format!("Chuyển sang {} (nguồn: {source})", if vietnamese { "V" } else { "E" }));
    if beep {
        let style = if vietnamese { MB_ICONASTERISK } else { MB_ICONEXCLAMATION };
        let _ = unsafe { MessageBeep(style) };
    }
}

struct Runtime {
    controller: Controller,
    config_mtime: Option<SystemTime>,
    tray: Option<Tray>,
    settings: Option<Child>,
    ticks: u32,
    shown_icon: Option<bool>,
}

impl Runtime {
    fn new(config: Config) -> Self {
        Self {
            config_mtime: Config::modified(),
            controller: Controller::new(config),
            tray: None,
            settings: None,
            ticks: 0,
            shown_icon: None,
        }
    }

    fn on_foreground(&mut self, hwnd: HWND) {
        let exe = process::exe_of_window(hwnd).unwrap_or_default();
        log::debug(&format!("Cửa sổ focus: {exe:?}"));
        self.controller.set_foreground(&exe);
    }

    /// Trả về `true` nếu cần nuốt phím (ứng dụng không được thấy phím gốc).
    fn on_key(&mut self, vk: u32, is_down: bool) -> bool {
        let beep = self.controller.config().beep_on_toggle;
        let mut platform = WinPlatform { settings: &mut self.settings, beep };
        let swallow = self.controller.on_key(&mut platform, vk, is_down);
        if log::debug_enabled() {
            log::debug(&format!(
                "vk={vk:#04x} {} -> {} | V={} loại_trừ={} exe={:?}",
                if is_down { "xuống" } else { "lên" },
                if swallow { "nuốt" } else { "cho qua" },
                self.controller.vietnamese(),
                self.controller.is_excluded(),
                self.controller.foreground_exe(),
            ));
        }
        swallow
    }

    fn toggle_from_ui(&mut self, source: &str) {
        self.controller.toggle();
        notify_toggle(
            self.controller.vietnamese(),
            self.controller.config().beep_on_toggle,
            source,
        );
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
                    log::info("Đã nạp lại cấu hình");
                    self.controller.apply_config(config);
                }
            }
        }

        let actions = self.tray.as_ref().map(Tray::poll).unwrap_or_default();
        for action in actions {
            match action {
                UiAction::Toggle => self.toggle_from_ui("khay"),
                UiAction::OpenSettings => process::open_settings(&mut self.settings),
                UiAction::OpenLog => process::open_log(),
                UiAction::Quit => {
                    log::info("Thoát theo yêu cầu");
                    unsafe { PostQuitMessage(0) }
                }
            }
        }

        let vietnamese = self.controller.vietnamese();
        if self.shown_icon != Some(vietnamese) {
            if let Some(tray) = &self.tray {
                tray.show(vietnamese);
            }
            self.shown_icon = Some(vietnamese);
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
        if !FIRST_KEY_LOGGED.swap(true, Ordering::Relaxed) {
            log::info(&format!(
                "Hook bàn phím đã nhận phím đầu tiên (vk={:#04x}, do chương trình gửi={injected})",
                kb.vkCode
            ));
        }
        let msg = wparam.0 as u32;
        let is_down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
        let is_up = matches!(msg, WM_KEYUP | WM_SYSKEYUP);
        if !injected && (is_down || is_up) {
            match with_runtime(|rt| rt.on_key(kb.vkCode, is_down)) {
                Some(true) => return LRESULT(1),
                Some(false) => {}
                None => log::debug("Bỏ qua phím vì trạng thái đang bận"),
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
            let _ = with_runtime(|rt| rt.controller.cancel_context());
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
/// `debug`: ghi nhật ký chi tiết từng phím vào `vnekey.log`.
pub fn run(debug: bool) -> Result<(), String> {
    log::init(Config::dir().map(|d| d.join("vnekey.log")), debug);
    log::info(&format!(
        "vnekey {} khởi động (gỡ lỗi={debug}, exe={:?}, cấu hình={:?})",
        env!("CARGO_PKG_VERSION"),
        std::env::current_exe().ok(),
        Config::path(),
    ));

    let first_run = !Config::exists();
    let config = Config::load();
    let open_settings_at_start = first_run || config.show_settings_on_start;
    let mut runtime = Runtime::new(config);
    let vietnamese = runtime.controller.vietnamese();
    runtime.tray =
        Some(Tray::new(vietnamese).inspect_err(|e| log::info(&format!("Lỗi tạo khay: {e}")))?);
    runtime.shown_icon = Some(vietnamese);
    log::info(&format!(
        "Đã tạo biểu tượng khay, chế độ ban đầu: {}",
        if vietnamese { "V" } else { "E" }
    ));
    if open_settings_at_start {
        process::open_settings(&mut runtime.settings);
    }
    runtime.on_foreground(unsafe { GetForegroundWindow() });
    RT.with(|cell| *cell.borrow_mut() = Some(runtime));

    let hmod: HINSTANCE = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?.into();
    let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0) }
        .map_err(|e| {
            let msg = format!("Không cài được hook bàn phím: {e}");
            log::info(&msg);
            msg
        })?;
    log::info("Đã cài hook bàn phím");
    // Hook chuột và theo dõi cửa sổ chỉ để reset từ; lỗi thì bỏ qua.
    let mouse = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0) }.ok();
    log::info(&format!("Hook chuột: {}", if mouse.is_some() { "ok" } else { "lỗi" }));
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
    log::info(&format!(
        "Theo dõi cửa sổ focus: {}",
        if win_event.is_invalid() { "lỗi" } else { "ok" }
    ));
    let timer = unsafe { SetTimer(None, 0, TICK_MS, Some(timer_proc)) };
    log::info(&format!("Bộ đếm giờ: {}", if timer == 0 { "lỗi" } else { "ok" }));

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
    log::info("Đã dừng");
    // Giải phóng trạng thái (gỡ biểu tượng khay).
    RT.with(|cell| cell.borrow_mut().take());
    Ok(())
}
