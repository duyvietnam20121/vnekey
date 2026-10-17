//! Nhận diện phím chuyển Việt/Anh. Không phụ thuộc Windows nên dễ kiểm thử.

use crate::Hotkey;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HotkeyEvent {
    None,
    /// Nuốt sự kiện phím (không cho ứng dụng thấy).
    Swallow,
    /// Chuyển V/E; `swallow` cho biết có nuốt phím hiện tại không.
    Toggle {
        swallow: bool,
    },
}

pub fn is_modifier_vk(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0xA0..=0xA5 | 0x5B | 0x5C)
}

fn key_to_vk(key: char) -> u32 {
    if key == ' ' {
        0x20
    } else {
        key.to_ascii_uppercase() as u32
    }
}

fn mods_match(hk: &Hotkey, m: Mods) -> bool {
    hk.ctrl == m.ctrl && hk.shift == m.shift && hk.alt == m.alt && hk.win == m.win
}

#[derive(Default)]
pub struct HotkeyDetector {
    /// Chế độ chỉ-phím-bổ-trợ: tổ hợp đang được giữ và chưa có phím nào khác chen vào.
    candidate: bool,
    /// Phím chính vừa bị nuốt lúc nhấn xuống, cần nuốt luôn lúc nhả.
    swallow_up: Option<u32>,
}

impl HotkeyDetector {
    pub fn feed(&mut self, hk: &Hotkey, vk: u32, is_down: bool, mods: Mods) -> HotkeyEvent {
        if !is_down {
            if self.swallow_up == Some(vk) {
                self.swallow_up = None;
                return HotkeyEvent::Swallow;
            }
            if hk.key.is_none() && is_modifier_vk(vk) && self.candidate {
                self.candidate = false;
                return HotkeyEvent::Toggle { swallow: false };
            }
            return HotkeyEvent::None;
        }

        // Phím chính không kèm phím bổ trợ sẽ nuốt mất chữ khi gõ: bỏ qua.
        if !hk.has_modifier() {
            return HotkeyEvent::None;
        }

        match hk.key {
            Some(k) => {
                if !is_modifier_vk(vk) && vk == key_to_vk(k) && mods_match(hk, mods) {
                    self.swallow_up = Some(vk);
                    return HotkeyEvent::Toggle { swallow: true };
                }
            }
            None => {
                self.candidate = is_modifier_vk(vk) && mods_match(hk, mods);
            }
        }
        HotkeyEvent::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: u32 = 0xA2;
    const SHIFT: u32 = 0xA0;

    #[test]
    fn ctrl_shift_toggles_on_release() {
        let hk = Hotkey::default();
        let mut d = HotkeyDetector::default();
        let ctrl = Mods { ctrl: true, ..Mods::default() };
        let both = Mods { ctrl: true, shift: true, ..Mods::default() };
        assert_eq!(d.feed(&hk, CTRL, true, ctrl), HotkeyEvent::None);
        assert_eq!(d.feed(&hk, SHIFT, true, both), HotkeyEvent::None);
        assert_eq!(d.feed(&hk, SHIFT, false, ctrl), HotkeyEvent::Toggle { swallow: false });
    }

    #[test]
    fn other_key_cancels_modifier_combo() {
        let hk = Hotkey::default();
        let mut d = HotkeyDetector::default();
        let both = Mods { ctrl: true, shift: true, ..Mods::default() };
        d.feed(&hk, CTRL, true, Mods { ctrl: true, ..Mods::default() });
        d.feed(&hk, SHIFT, true, both);
        d.feed(&hk, 0x54, true, both); // Ctrl+Shift+T
        assert_eq!(
            d.feed(&hk, SHIFT, false, Mods { ctrl: true, ..Mods::default() }),
            HotkeyEvent::None
        );
    }

    #[test]
    fn alt_z_toggles_and_swallows() {
        let hk = Hotkey { ctrl: false, shift: false, alt: true, win: false, key: Some('Z') };
        let mut d = HotkeyDetector::default();
        let alt = Mods { alt: true, ..Mods::default() };
        assert_eq!(d.feed(&hk, 0x5A, true, alt), HotkeyEvent::Toggle { swallow: true });
        assert_eq!(d.feed(&hk, 0x5A, false, alt), HotkeyEvent::Swallow);
    }
}
