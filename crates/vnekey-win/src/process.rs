//! Tiến trình và cửa sổ: ứng dụng đang focus, mở cửa sổ cài đặt, chạy một bản duy nhất,
//! chạy lại với quyền admin.

use std::fs::{self, File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};

use vnekey_config::{normalize_exe, Config};
use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetWindowThreadProcessId, MessageBoxW, SetForegroundWindow, ShowWindow,
    MB_ICONERROR, MB_OK, SW_RESTORE,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// ERROR_SHARING_VIOLATION
const SHARING_VIOLATION: i32 = 32;

/// Tiêu đề cửa sổ cài đặt; `vnekey-app` dùng cùng chuỗi này để tiến trình khay tìm lại cửa sổ.
pub const SETTINGS_TITLE: &str = "vnekey - Cài đặt";

/// Tên file .exe (viết thường) của tiến trình sở hữu cửa sổ.
pub fn exe_of_window(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32)) };
    if pid == 0 {
        return None;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 520];
    let mut len = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
    }
    .is_ok();
    let _ = unsafe { CloseHandle(handle) };
    if !ok {
        return None;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    Some(normalize_exe(&path))
}

/// Mở cửa sổ cài đặt (tiến trình riêng `vnekey --settings`); nếu đã mở thì đưa lên phía trước.
pub fn open_settings(child: &mut Option<Child>) {
    if let Some(running) = child.as_mut() {
        if matches!(running.try_wait(), Ok(None)) {
            focus_settings();
            return;
        }
    }
    *child = spawn_settings();
}

pub fn spawn_settings() -> Option<Child> {
    let exe = std::env::current_exe().ok()?;
    Command::new(exe).arg("--settings").spawn().ok()
}

fn focus_settings() {
    let title = HSTRING::from(SETTINGS_TITLE);
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), &title) {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

/// Chạy lại vnekey với quyền admin (hiện hộp thoại UAC). `true` nếu bản nâng quyền đã được mở.
pub fn relaunch_elevated() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let path = exe.display().to_string().replace('\'', "''");
    let script = format!("Start-Process -FilePath '{path}' -ArgumentList '--elevated' -Verb RunAs");
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Khoá tệp độc quyền để chỉ chạy một tiến trình khay; tự nhả khi tiến trình kết thúc.
pub struct SingleInstance {
    _file: Option<File>,
}

impl SingleInstance {
    /// `None` nếu đã có một bản vnekey đang chạy.
    pub fn acquire() -> Option<Self> {
        let Some(dir) = Config::dir() else {
            return Some(Self { _file: None });
        };
        let _ = fs::create_dir_all(&dir);
        let opened = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(dir.join("instance.lock"));
        match opened {
            Ok(file) => Some(Self { _file: Some(file) }),
            Err(e) if e.raw_os_error() == Some(SHARING_VIOLATION) => None,
            // Không khoá được vì lý do khác (ví dụ thư mục không ghi được): vẫn cho chạy.
            Err(_) => Some(Self { _file: None }),
        }
    }
}

/// Hiện hộp thoại lỗi (ứng dụng không có cửa sổ console).
pub fn report_error(message: &str) {
    let text = HSTRING::from(message);
    unsafe {
        MessageBoxW(None, &text, w!("vnekey"), MB_OK | MB_ICONERROR);
    }
}
