//! Biểu diễn một âm tiết tiếng Việt đang gõ: danh sách chữ cái + dấu thanh,
//! cùng các luật đặt dấu và kiểm tra chính tả.

use std::ops::Range;

/// Dấu phụ gắn với chữ cái (â, ê, ô, ă, ơ, ư, đ).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    /// Dấu mũ: â, ê, ô
    Circumflex,
    /// Dấu trăng: ă
    Breve,
    /// Dấu móc: ơ, ư
    Horn,
    /// Dấu gạch ngang: đ
    Stroke,
}

/// Dấu thanh. Thứ tự khớp với các hàng trong bảng nguyên âm bên dưới.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    None,
    /// sắc
    Acute,
    /// huyền
    Grave,
    /// hỏi
    Hook,
    /// ngã
    Tilde,
    /// nặng
    Dot,
}

/// Một chữ cái trong âm tiết (chưa tính dấu thanh, dấu thanh nằm ở cấp âm tiết).
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    /// Chữ cái gốc, viết thường, không dấu (hoặc một chữ số khi hoàn tác kiểu VNI).
    pub base: char,
    pub mark: Mark,
    pub upper: bool,
}

impl Cell {
    pub fn new(base: char, upper: bool) -> Self {
        Self { base, mark: Mark::None, upper }
    }

    pub fn is_vowel(&self) -> bool {
        matches!(self.base, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
    }

    /// Chữ cái này có thể mang dấu `mark` không?
    pub fn can_take(&self, mark: Mark) -> bool {
        matches!(
            (self.base, mark),
            ('a', Mark::Circumflex | Mark::Breve)
                | ('e', Mark::Circumflex)
                | ('o', Mark::Circumflex | Mark::Horn)
                | ('u', Mark::Horn)
                | ('d', Mark::Stroke)
        )
    }

    /// Chữ cái hiển thị (đã có dấu phụ, chưa có dấu thanh, viết thường).
    fn letter(&self) -> char {
        if self.is_vowel() {
            vowel_char(self.base, self.mark, Tone::None)
        } else if self.base == 'd' && self.mark == Mark::Stroke {
            'đ'
        } else {
            self.base
        }
    }
}

/// Bảng nguyên âm dựng sẵn (NFC): mỗi hàng có 6 ký tự theo thứ tự dấu thanh
/// không dấu, sắc, huyền, hỏi, ngã, nặng.
const ROWS: [(char, Mark, &str); 12] = [
    ('a', Mark::None, "aáàảãạ"),
    ('a', Mark::Breve, "ăắằẳẵặ"),
    ('a', Mark::Circumflex, "âấầẩẫậ"),
    ('e', Mark::None, "eéèẻẽẹ"),
    ('e', Mark::Circumflex, "êếềểễệ"),
    ('i', Mark::None, "iíìỉĩị"),
    ('o', Mark::None, "oóòỏõọ"),
    ('o', Mark::Circumflex, "ôốồổỗộ"),
    ('o', Mark::Horn, "ơớờởỡợ"),
    ('u', Mark::None, "uúùủũụ"),
    ('u', Mark::Horn, "ưứừửữự"),
    ('y', Mark::None, "yýỳỷỹỵ"),
];

const TONES: [Tone; 6] = [Tone::None, Tone::Acute, Tone::Grave, Tone::Hook, Tone::Tilde, Tone::Dot];

/// Tra bảng nguyên âm: (chữ gốc, dấu phụ, dấu thanh) -> ký tự Unicode dựng sẵn (NFC).
pub(crate) fn vowel_char(base: char, mark: Mark, tone: Tone) -> char {
    // i và y không mang dấu phụ.
    let mark = if matches!(base, 'i' | 'y') { Mark::None } else { mark };
    ROWS.iter()
        .find(|(b, m, _)| *b == base && *m == mark)
        .and_then(|(_, _, row)| row.chars().nth(tone as usize))
        .unwrap_or(base)
}

/// Một ký tự tiếng Việt được tách thành chữ gốc, dấu phụ và dấu thanh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decomposed {
    /// Chữ gốc viết thường ('a', 'e', ..., 'd').
    pub base: char,
    pub mark: Mark,
    pub tone: Tone,
    pub upper: bool,
}

/// Tách một ký tự dựng sẵn (hoa hoặc thường) thành các thành phần.
/// Trả về `None` nếu không phải nguyên âm có dấu hoặc đ/Đ.
pub fn decompose(ch: char) -> Option<Decomposed> {
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    let upper = lower != ch;
    if lower == 'đ' {
        return Some(Decomposed { base: 'd', mark: Mark::Stroke, tone: Tone::None, upper });
    }
    ROWS.iter().find_map(|(base, mark, row)| {
        row.chars().position(|c| c == lower).map(|i| Decomposed {
            base: *base,
            mark: *mark,
            tone: TONES[i],
            upper,
        })
    })
}

fn to_upper(ch: char) -> char {
    ch.to_uppercase().next().unwrap_or(ch)
}

/// Phụ âm đầu hợp lệ (cũng dùng để kiểm tra tiền tố khi đang gõ dở).
const INITIALS: &[&str] = &[
    "b", "c", "ch", "d", "đ", "g", "gh", "gi", "h", "k", "kh", "l", "m", "n", "ng", "ngh", "nh",
    "p", "ph", "q", "qu", "r", "s", "t", "th", "tr", "v", "x",
];

/// Phụ âm cuối hợp lệ.
const FINALS: &[&str] = &["", "c", "ch", "m", "n", "ng", "nh", "p", "t"];

/// Cụm nguyên âm hợp lệ, tính theo chữ cái gốc (bỏ qua dấu phụ).
/// Bỏ qua dấu phụ để các trạng thái trung gian khi gõ Telex
/// (ví dụ "vie" trước khi gõ thêm "e" thành "viê") không bị coi là sai.
const NUCLEI: &[&str] = &[
    "a", "e", "i", "o", "u", "y", //
    "ai", "ao", "au", "ay", "eo", "eu", "ia", "ie", "iu", "oa", "oe", "oi", "ua", "ue", "ui", "uo",
    "uu", "uy", "ye", //
    "oai", "oao", "oay", "oeo", "uay", "uoi", "uou", "uye", "ieu", "yeu", "uya", "uyu", "ueu",
];

#[derive(Debug, Default, Clone)]
pub struct Syllable {
    pub cells: Vec<Cell>,
    pub tone: Tone,
}

impl Syllable {
    pub fn clear(&mut self) {
        self.cells.clear();
        self.tone = Tone::None;
    }

    pub fn has_vowel(&self) -> bool {
        self.cells.iter().any(Cell::is_vowel)
    }

    /// Vị trí cụm nguyên âm. "qu" và "gi" (khi còn nguyên âm khác theo sau)
    /// được tính vào phụ âm đầu, không thuộc cụm nguyên âm.
    fn nucleus(&self) -> Option<Range<usize>> {
        let first = self.cells.iter().position(Cell::is_vowel)?;
        let mut end = first;
        while end < self.cells.len() && self.cells[end].is_vowel() {
            end += 1;
        }
        let mut start = first;
        if start > 0 && end - start >= 2 {
            let prev = self.cells[start - 1].base;
            let cur = self.cells[start].base;
            let after_n = start >= 2 && self.cells[start - 2].base == 'n';
            if (prev == 'q' && cur == 'u') || (prev == 'g' && cur == 'i' && !after_n) {
                start += 1;
            }
        }
        Some(start..end)
    }

    /// Chọn chữ cái đặt dấu thanh.
    fn tone_index(&self, modern: bool) -> Option<usize> {
        let r = self.nucleus()?;
        let vowels = &self.cells[r.clone()];
        match vowels.len() {
            0 => None,
            1 => Some(r.start),
            n => {
                // Có chữ mang dấu phụ (â ê ô ă ơ ư): đặt vào chữ cuối cùng trong số đó
                // (ươ -> ơ, uyê -> ê, iêu -> ê).
                if let Some(i) = vowels.iter().rposition(|c| c.mark != Mark::None) {
                    return Some(r.start + i);
                }
                let has_final = r.end < self.cells.len();
                if n >= 3 || has_final {
                    return Some(r.start + 1);
                }
                // Hai nguyên âm, không có phụ âm cuối.
                let second_style = matches!(
                    (vowels[0].base, vowels[1].base),
                    ('o', 'a') | ('o', 'e') | ('u', 'y')
                );
                Some(if modern && second_style { r.start + 1 } else { r.start })
            }
        }
    }

    /// Dựng chuỗi hiển thị.
    pub fn render(&self, modern: bool) -> String {
        let tone_at = self.tone_index(modern);
        self.cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let ch = if c.is_vowel() {
                    let tone = if Some(i) == tone_at { self.tone } else { Tone::None };
                    vowel_char(c.base, c.mark, tone)
                } else if c.base == 'd' && c.mark == Mark::Stroke {
                    'đ'
                } else {
                    c.base
                };
                if c.upper { to_upper(ch) } else { ch }
            })
            .collect()
    }

    /// Chuẩn hoá "uơ"/"ưo" thành "ươ" khi phía sau còn ký tự
    /// (dược, tương, người), nhưng giữ "uơ" ở cuối từ (thuở, huơ).
    pub fn normalize(&mut self) {
        let n = self.cells.len();
        for i in 0..n.saturating_sub(2) {
            let (a, b) = (self.cells[i], self.cells[i + 1]);
            let after_q = i > 0 && self.cells[i - 1].base == 'q';
            if a.base == 'u' && b.base == 'o' && !after_q {
                let pair = (a.mark, b.mark);
                if pair == (Mark::None, Mark::Horn) || pair == (Mark::Horn, Mark::None) {
                    self.cells[i].mark = Mark::Horn;
                    self.cells[i + 1].mark = Mark::Horn;
                }
            }
        }
    }

    /// Kiểm tra âm tiết (có thể đang gõ dở) có hợp lệ tiếng Việt không.
    pub fn is_valid(&self) -> bool {
        let Some(nuc) = self.nucleus() else {
            // Chưa có nguyên âm: chỉ cần là tiền tố của một phụ âm đầu.
            let s: String = self.cells.iter().map(Cell::letter).collect();
            return INITIALS.iter().any(|i| i.starts_with(s.as_str()));
        };
        // Nguyên âm xuất hiện lại sau phụ âm cuối -> không phải một âm tiết.
        if self.cells[nuc.end..].iter().any(Cell::is_vowel) {
            return false;
        }
        let initial: String = self.cells[..nuc.start].iter().map(Cell::letter).collect();
        if !initial.is_empty() && !INITIALS.contains(&initial.as_str()) {
            return false;
        }
        let nucleus: String = self.cells[nuc.clone()].iter().map(|c| c.base).collect();
        if !NUCLEI.contains(&nucleus.as_str()) {
            return false;
        }
        let fin: String = self.cells[nuc.end..].iter().map(Cell::letter).collect();
        if !FINALS.contains(&fin.as_str()) {
            return false;
        }
        // Vần kết thúc bằng c, ch, p, t chỉ mang dấu sắc hoặc nặng.
        if matches!(fin.as_str(), "c" | "ch" | "p" | "t")
            && !matches!(self.tone, Tone::None | Tone::Acute | Tone::Dot)
        {
            return false;
        }
        true
    }
}