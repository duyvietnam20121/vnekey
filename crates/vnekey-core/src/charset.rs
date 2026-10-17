//! Bảng mã đầu ra: bộ gõ luôn làm việc với Unicode dựng sẵn, rồi mã hoá sang bảng mã
//! người dùng chọn ngay trước khi gửi ra ứng dụng.

use crate::charset_tables::{TCVN3, VNI_WINDOWS};
use crate::syllable::{Mark, Tone, decompose, vowel_char};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Charset {
    /// Unicode dựng sẵn (NFC), mặc định.
    #[default]
    Unicode,
    /// Unicode tổ hợp (NFD): chữ gốc + dấu kết hợp.
    UnicodeCompound,
    /// TCVN3 (ABC), font .Vn*
    Tcvn3,
    /// VNI Windows, font VNI-*
    VniWindows,
    /// VIQR: chỉ dùng ký tự ASCII (tie^'ng Vie^.t).
    Viqr,
}

impl Charset {
    pub const ALL: [Charset; 5] = [
        Charset::Unicode,
        Charset::UnicodeCompound,
        Charset::Tcvn3,
        Charset::VniWindows,
        Charset::Viqr,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Charset::Unicode => "Unicode",
            Charset::UnicodeCompound => "Unicode tổ hợp",
            Charset::Tcvn3 => "TCVN3 (ABC)",
            Charset::VniWindows => "VNI Windows",
            Charset::Viqr => "VIQR",
        }
    }

    /// Mã hoá một chuỗi Unicode dựng sẵn sang bảng mã này.
    pub fn encode(self, text: &str) -> String {
        if self == Charset::Unicode {
            return text.to_owned();
        }
        let mut out = String::with_capacity(text.len() + text.len() / 2);
        for ch in text.chars() {
            self.encode_char(ch, &mut out);
        }
        out
    }

    /// Số ký tự mà `ch` chiếm trên màn hình sau khi mã hoá (để biết cần xoá bao nhiêu phím).
    pub fn encoded_len(self, ch: char) -> usize {
        if self == Charset::Unicode || ch.is_ascii() {
            return 1;
        }
        let mut tmp = String::new();
        self.encode_char(ch, &mut tmp);
        tmp.chars().count()
    }

    fn encode_char(self, ch: char, out: &mut String) {
        if ch.is_ascii() {
            out.push(ch);
            return;
        }
        match self {
            Charset::Unicode => out.push(ch),
            Charset::Tcvn3 => push_table(TCVN3, ch, out),
            Charset::VniWindows => push_table(VNI_WINDOWS, ch, out),
            Charset::UnicodeCompound => encode_compound(ch, out),
            Charset::Viqr => encode_viqr(ch, out),
        }
    }
}

fn push_table(table: &[(char, &str)], ch: char, out: &mut String) {
    match table.binary_search_by_key(&ch, |(c, _)| *c) {
        Ok(i) => out.push_str(table[i].1),
        Err(_) => out.push(ch),
    }
}

fn base_letter(base: char, upper: bool) -> char {
    if upper { base.to_ascii_uppercase() } else { base }
}

/// Unicode tổ hợp (kiểu Windows-1258/UniKey): chữ có dấu phụ (â ê ô ă ơ ư) giữ dạng
/// dựng sẵn, chỉ dấu thanh được tách thành ký tự kết hợp đặt ngay sau chữ.
fn encode_compound(ch: char, out: &mut String) {
    let Some(d) = decompose(ch) else {
        out.push(ch);
        return;
    };
    if d.base == 'd' {
        out.push(ch); // đ/Đ không tách
        return;
    }
    let letter = vowel_char(d.base, d.mark, Tone::None);
    out.extend(if d.upper { letter.to_uppercase().collect::<Vec<_>>() } else { vec![letter] });
    match d.tone {
        Tone::Acute => out.push('\u{301}'),
        Tone::Grave => out.push('\u{300}'),
        Tone::Hook => out.push('\u{309}'),
        Tone::Tilde => out.push('\u{303}'),
        Tone::Dot => out.push('\u{323}'),
        Tone::None => {}
    }
}

/// VIQR: chữ gốc, dấu phụ (^ ( +), rồi dấu thanh (' ` ? ~ .). đ viết là dd, Đ là DD.
fn encode_viqr(ch: char, out: &mut String) {
    let Some(d) = decompose(ch) else {
        out.push(ch);
        return;
    };
    let letter = base_letter(d.base, d.upper);
    out.push(letter);
    match d.mark {
        Mark::Circumflex => out.push('^'),
        Mark::Breve => out.push('('),
        Mark::Horn => out.push('+'),
        Mark::Stroke => out.push(letter),
        Mark::None => {}
    }
    match d.tone {
        Tone::Acute => out.push('\''),
        Tone::Grave => out.push('`'),
        Tone::Hook => out.push('?'),
        Tone::Tilde => out.push('~'),
        Tone::Dot => out.push('.'),
        Tone::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charset_tables::{REF_COMPOUND, REF_VIQR};

    #[test]
    fn viqr_matches_reference_for_every_letter() {
        for (ch, expected) in REF_VIQR {
            assert_eq!(Charset::Viqr.encode(&ch.to_string()), *expected, "ký tự {ch}");
        }
    }

    #[test]
    fn compound_matches_reference_for_every_letter() {
        for (ch, expected) in REF_COMPOUND {
            assert_eq!(Charset::UnicodeCompound.encode(&ch.to_string()), *expected, "ký tự {ch}");
        }
    }

    #[test]
    fn legacy_tables_cover_every_vietnamese_letter() {
        // Mọi chữ có dấu mà bộ gõ có thể sinh ra phải có trong TCVN3 và VNI Windows.
        let rows = [
            "aáàảãạăắằẳẵặâấầẩẫậ",
            "eéèẻẽẹêếềểễệ",
            "iíìỉĩị",
            "oóòỏõọôốồổỗộơớờởỡợ",
            "uúùủũụưứừửữự",
            "yýỳỷỹỵ",
            "đ",
        ];
        for ch in rows.iter().flat_map(|r| r.chars()).filter(|c| !c.is_ascii()) {
            for c in [ch, ch.to_uppercase().next().unwrap()] {
                assert!(TCVN3.binary_search_by_key(&c, |(k, _)| *k).is_ok(), "TCVN3 thiếu {c}");
                assert!(VNI_WINDOWS.binary_search_by_key(&c, |(k, _)| *k).is_ok(), "VNI thiếu {c}");
            }
        }
    }

    #[test]
    fn tables_are_sorted_for_binary_search() {
        assert!(TCVN3.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(VNI_WINDOWS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn sample_words() {
        assert_eq!(Charset::Viqr.encode("tiếng Việt"), "tie^'ng Vie^.t");
        assert_eq!(Charset::Unicode.encode("tiếng Việt"), "tiếng Việt");
        assert_eq!(Charset::UnicodeCompound.encode("ệ"), "\u{EA}\u{323}");
        assert_eq!(Charset::VniWindows.encode("ế"), "e\u{E1}");
        assert_eq!(Charset::Tcvn3.encode("đ"), "\u{AE}");
    }

    #[test]
    fn encoded_len_counts_screen_characters() {
        assert_eq!(Charset::Unicode.encoded_len('ế'), 1);
        assert_eq!(Charset::Tcvn3.encoded_len('ế'), 1);
        assert_eq!(Charset::VniWindows.encoded_len('ế'), 2);
        assert_eq!(Charset::Viqr.encoded_len('ế'), 3);
        assert_eq!(Charset::UnicodeCompound.encoded_len('ế'), 2);
        assert_eq!(Charset::Viqr.encoded_len('a'), 1);
    }
}