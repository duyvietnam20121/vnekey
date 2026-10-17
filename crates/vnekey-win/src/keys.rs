//! Phân loại mã phím ảo (VK) của Windows thành hành động của bộ gõ.
//! Giả định bố cục QWERTY của Mỹ cho các phím dấu câu (cũng là bố cục tiếng Việt thông dụng).

use vnekey_core::Boundary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Ký tự đưa vào bộ gõ (chữ, số, hoặc `[ ] { }` khi bật `bracket_horn`).
    Char(char),
    Backspace,
    /// Shift, Ctrl, Alt, Win, CapsLock: không ảnh hưởng đến từ đang gõ.
    Modifier,
    /// Phím kết thúc từ.
    Boundary(Boundary),
}

/// `shift`/`caps` quyết định hoa/thường. `bracket_horn`: nhận `[`/`]` làm ơ/ư.
pub fn classify(vk: u32, shift: bool, caps: bool, bracket_horn: bool) -> Key {
    match vk {
        0x41..=0x5A => {
            let c = vk as u8 as char;
            Key::Char(if shift ^ caps { c } else { c.to_ascii_lowercase() })
        }
        0x30..=0x39 if !shift => Key::Char(vk as u8 as char),
        0x60..=0x69 => Key::Char((b'0' + (vk - 0x60) as u8) as char),
        0x08 => Key::Backspace,
        0x10..=0x12 | 0xA0..=0xA5 | 0x5B | 0x5C | 0x14 => Key::Modifier,
        0x20 => Key::Boundary(Boundary::Space),
        0x0D => Key::Boundary(Boundary::Enter),
        0xDB if bracket_horn => Key::Char(if shift { '{' } else { '[' }),
        0xDD if bracket_horn => Key::Char(if shift { '}' } else { ']' }),
        _ => match punct_char(vk, shift) {
            Some(c) => Key::Boundary(Boundary::Punct(c)),
            None => Key::Boundary(Boundary::Other),
        },
    }
}

/// Ký tự dấu câu ứng với phím (bố cục US).
pub fn punct_char(vk: u32, shift: bool) -> Option<char> {
    let (plain, shifted) = match vk {
        0x30 => ('0', ')'),
        0x31 => ('1', '!'),
        0x32 => ('2', '@'),
        0x33 => ('3', '#'),
        0x34 => ('4', '$'),
        0x35 => ('5', '%'),
        0x36 => ('6', '^'),
        0x37 => ('7', '&'),
        0x38 => ('8', '*'),
        0x39 => ('9', '('),
        0xBA => (';', ':'),
        0xBB => ('=', '+'),
        0xBC => (',', '<'),
        0xBD => ('-', '_'),
        0xBE => ('.', '>'),
        0xBF => ('/', '?'),
        0xC0 => ('`', '~'),
        0xDB => ('[', '{'),
        0xDC => ('\\', '|'),
        0xDD => (']', '}'),
        0xDE => ('\'', '"'),
        _ => return None,
    };
    Some(if shift { shifted } else { plain })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_follow_shift_xor_caps() {
        assert_eq!(classify(0x41, false, false, false), Key::Char('a'));
        assert_eq!(classify(0x41, true, false, false), Key::Char('A'));
        assert_eq!(classify(0x41, false, true, false), Key::Char('A'));
        assert_eq!(classify(0x41, true, true, false), Key::Char('a'));
    }

    #[test]
    fn digits_and_shifted_digits() {
        assert_eq!(classify(0x36, false, false, false), Key::Char('6'));
        assert_eq!(classify(0x31, true, false, false), Key::Boundary(Boundary::Punct('!')));
        assert_eq!(classify(0x65, false, false, false), Key::Char('5')); // numpad 5
    }

    #[test]
    fn boundaries() {
        assert_eq!(classify(0x20, false, false, false), Key::Boundary(Boundary::Space));
        assert_eq!(classify(0x0D, false, false, false), Key::Boundary(Boundary::Enter));
        assert_eq!(classify(0xBE, false, false, false), Key::Boundary(Boundary::Punct('.')));
        assert_eq!(classify(0xBF, true, false, false), Key::Boundary(Boundary::Punct('?')));
        assert_eq!(classify(0x25, false, false, false), Key::Boundary(Boundary::Other)); // ←
        assert_eq!(classify(0x1B, false, false, false), Key::Boundary(Boundary::Other));
        // Esc
    }

    #[test]
    fn modifiers_and_backspace() {
        assert_eq!(classify(0xA0, false, false, false), Key::Modifier);
        assert_eq!(classify(0x14, false, false, false), Key::Modifier);
        assert_eq!(classify(0x08, false, false, false), Key::Backspace);
    }

    #[test]
    fn brackets_depend_on_option() {
        assert_eq!(classify(0xDB, false, false, true), Key::Char('['));
        assert_eq!(classify(0xDD, true, false, true), Key::Char('}'));
        assert_eq!(classify(0xDB, false, false, false), Key::Boundary(Boundary::Punct('[')));
    }
}
