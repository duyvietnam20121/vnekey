//! Gửi phím bằng SendInput, kèm đường dán qua Clipboard.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_LEFT,
    VK_SHIFT,
};

use crate::plan::{SendOptions, Step, plan};

/// Dấu hiệu gắn vào phím do chính vnekey gửi, để hook bỏ qua (tránh lặp vô hạn).
pub(crate) const MAGIC: usize = 0x564E_454B; // "VNEK"

const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(0x56);
/// Chờ ứng dụng đích dán xong rồi mới trả lại nội dung Clipboard cũ.
const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(400);

struct ClipState {
    /// `Some(Some(text))`: nội dung chữ ban đầu cần khôi phục; `Some(None)`: Clipboard không có chữ.
    saved: Option<Option<String>>,
    restore_at: Option<Instant>,
}

thread_local! {
    static CLIP: RefCell<ClipState> = const { RefCell::new(ClipState { saved: None, restore_at: None }) };
}

fn key_input(vk: VIRTUAL_KEY, up: bool, extended: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: MAGIC },
        },
    }
}

fn unicode_input(unit: u16, up: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: MAGIC,
            },
        },
    }
}

fn tap(batch: &mut Vec<INPUT>, vk: VIRTUAL_KEY, extended: bool) {
    batch.push(key_input(vk, false, extended));
    batch.push(key_input(vk, true, extended));
}

fn flush(batch: &[INPUT]) {
    if !batch.is_empty() {
        unsafe {
            SendInput(batch, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

/// Xoá `backspaces` ký tự rồi gõ `text`, gom thành một lần SendInput để không bị chen phím.
pub(crate) fn replace(backspaces: usize, text: &str, opts: SendOptions) {
    let mut batch: Vec<INPUT> = Vec::new();
    for step in plan(backspaces, text, opts) {
        match step {
            Step::Backspace(n) => {
                for _ in 0..n {
                    tap(&mut batch, VK_BACK, false);
                }
            }
            Step::ShiftLeft(n) => {
                batch.push(key_input(VK_SHIFT, false, false));
                for _ in 0..n {
                    tap(&mut batch, VK_LEFT, true);
                }
                batch.push(key_input(VK_SHIFT, true, false));
            }
            Step::Type(chars) => {
                for unit in chars.encode_utf16() {
                    batch.push(unicode_input(unit, false));
                    batch.push(unicode_input(unit, true));
                }
            }
            Step::Paste(chars) => {
                stage_clipboard(&chars);
                batch.push(key_input(VK_CONTROL, false, false));
                tap(&mut batch, VK_V, false);
                batch.push(key_input(VK_CONTROL, true, false));
            }
        }
    }
    flush(&batch);
}

/// Gửi lại một phím (dùng cho phím kết thúc từ sau khi mở rộng gõ tắt).
pub(crate) fn tap_key(vk: u32) {
    let mut batch = Vec::with_capacity(2);
    tap(&mut batch, VIRTUAL_KEY(vk as u16), false);
    flush(&batch);
}

/// Đặt chữ vào Clipboard, nhớ nội dung cũ để trả lại sau.
fn stage_clipboard(text: &str) {
    CLIP.with_borrow_mut(|clip| {
        if clip.saved.is_none() {
            clip.saved = Some(clipboard_win::get_clipboard_string().ok());
        }
        let _ = clipboard_win::set_clipboard_string(text);
        clip.restore_at = Some(Instant::now() + CLIPBOARD_RESTORE_DELAY);
    });
}

/// Gọi định kỳ: khi đã đủ thời gian thì trả lại nội dung Clipboard ban đầu.
pub(crate) fn tick_clipboard() {
    CLIP.with_borrow_mut(|clip| {
        if clip.restore_at.is_some_and(|t| Instant::now() >= t) {
            if let Some(Some(text)) = clip.saved.take() {
                let _ = clipboard_win::set_clipboard_string(&text);
            }
            clip.saved = None;
            clip.restore_at = None;
        }
    });
}