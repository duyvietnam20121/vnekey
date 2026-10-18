//! Bộ máy gõ: nhận từng phím, cập nhật âm tiết và trả về thao tác cần làm trên màn hình.

use std::collections::HashMap;

use crate::charset::Charset;
use crate::syllable::{Cell, Mark, Syllable, Tone};

/// Kiểu gõ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InputMethod {
    #[default]
    Telex,
    Vni,
}

/// Cấu hình của bộ máy gõ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineConfig {
    pub method: InputMethod,
    /// Bảng mã đầu ra (bộ gõ làm việc bằng Unicode rồi mã hoá khi gửi đi).
    pub charset: Charset,
    /// Kiểm tra chính tả: từ không hợp lệ sẽ được khôi phục về đúng phím đã gõ.
    pub spell_check: bool,
    /// Đặt dấu kiểu mới: hoà, thuý (thay vì hòa, thúy).
    pub modern_tone: bool,
    /// Cho phép `w` đứng đầu từ là phụ âm (thay vì biến thành ư).
    pub allow_fjwz: bool,
    /// Tự viết hoa chữ đầu câu (sau `. ! ?` rồi cách, hoặc sau Enter).
    pub auto_capitalize: bool,
    /// Telex: `[` -> ơ, `]` -> ư (và `{` -> Ơ, `}` -> Ư).
    pub bracket_horn: bool,
    /// Bật gõ tắt.
    pub abbreviations: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            method: InputMethod::Telex,
            charset: Charset::Unicode,
            spell_check: true,
            modern_tone: true,
            allow_fjwz: false,
            auto_capitalize: false,
            bracket_horn: false,
            abbreviations: false,
        }
    }
}

/// Phím kết thúc từ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    Space,
    Enter,
    /// Dấu câu hoặc ký hiệu in được (`.`, `,`, `!`, `?`, ...).
    Punct(char),
    /// Phím điều hướng hoặc phím khác: chỉ kết thúc từ, không kích hoạt gõ tắt.
    Other,
}

/// Thao tác mà tầng hệ điều hành cần thực hiện cho phím vừa gõ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reaction {
    /// Để phím đi qua nguyên vẹn.
    Pass,
    /// Nuốt phím gốc, xoá `backspaces` ký tự rồi gõ `text`.
    Replace { backspaces: usize, text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Áp dụng luật gõ bình thường.
    Normal,
    /// Vừa hoàn tác một dấu (aaa -> aa): các phím sau gõ nguyên văn.
    Escaped,
    /// Từ không hợp lệ: hiển thị đúng các phím đã gõ.
    Restored,
}

pub struct Engine {
    config: EngineConfig,
    syl: Syllable,
    /// Các phím đã gõ trong từ hiện tại, giữ nguyên hoa/thường.
    raw: String,
    /// Chuỗi Unicode đang hiển thị cho từ hiện tại.
    shown: String,
    mode: Mode,
    /// Bảng gõ tắt: từ gõ -> cụm mở rộng.
    abbreviations: HashMap<String, String>,
    /// Chữ cái tiếp theo cần viết hoa (đầu câu).
    cap_next: bool,
    /// Vừa gõ `.`/`!`/`?`; nếu tiếp theo là Space thì bắt đầu câu mới.
    after_terminator: bool,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            syl: Syllable::default(),
            raw: String::new(),
            shown: String::new(),
            mode: Mode::Normal,
            abbreviations: HashMap::new(),
            cap_next: false,
            after_terminator: false,
        }
    }

    pub fn set_config(&mut self, config: EngineConfig) {
        self.config = config;
        self.cancel_context();
    }

    /// Thay bảng gõ tắt. Khoá nên viết thường; "Ko"/"KO" vẫn khớp và tự đổi hoa/thường.
    pub fn set_abbreviations(&mut self, table: HashMap<String, String>) {
        self.abbreviations = table;
    }

    /// Bỏ từ đang gõ dở (nhưng nhớ trạng thái đầu câu).
    pub fn reset(&mut self) {
        self.syl.clear();
        self.raw.clear();
        self.shown.clear();
        self.mode = Mode::Normal;
    }

    /// Bỏ cả từ đang gõ lẫn trạng thái đầu câu (nhấp chuột, đổi cửa sổ, bật/tắt V/E).
    pub fn cancel_context(&mut self) {
        self.reset();
        self.cap_next = false;
        self.after_terminator = false;
    }

    /// Người dùng nhấn Backspace. Với bảng mã một chữ chiếm nhiều ký tự (VNI Windows,
    /// VIQR, Unicode tổ hợp) cần nuốt phím và tự xoá đủ số ký tự.
    pub fn backspace(&mut self) -> Reaction {
        let Some(popped) = self.shown.pop() else {
            self.reset();
            return Reaction::Pass;
        };
        if self.mode == Mode::Restored {
            self.raw.pop();
        } else {
            self.syl.cells.pop();
            if !self.syl.has_vowel() {
                self.syl.tone = Tone::None;
            }
            self.raw = self.shown.clone();
        }
        if self.shown.is_empty() {
            self.reset();
        }
        match self.config.charset.encoded_len(popped) {
            0 | 1 => Reaction::Pass,
            n => Reaction::Replace { backspaces: n, text: String::new() },
        }
    }

    /// Phím kết thúc từ. Có thể trả về `Replace` để mở rộng gõ tắt; khi đó tầng hệ điều hành
    /// phải gửi lại chính phím kết thúc sau phần thay thế.
    pub fn boundary(&mut self, boundary: Boundary) -> Reaction {
        let reaction =
            if matches!(boundary, Boundary::Other) { Reaction::Pass } else { self.try_expand() };
        match boundary {
            Boundary::Enter => {
                self.cap_next = true;
                self.after_terminator = false;
            }
            Boundary::Space => {
                if self.after_terminator {
                    self.cap_next = true;
                }
            }
            Boundary::Punct(c) => self.after_terminator = matches!(c, '.' | '!' | '?'),
            Boundary::Other => self.after_terminator = false,
        }
        self.reset();
        reaction
    }

    /// Theo dõi chữ gõ khi đang ở chế độ tiếng Anh, chỉ để gõ tắt vẫn hoạt động.
    pub fn process_plain(&mut self, ch: char) -> Reaction {
        if !ch.is_ascii_alphanumeric() {
            self.reset();
            return Reaction::Pass;
        }
        let shown_ch = self.apply_auto_cap(ch);
        self.mode = Mode::Restored;
        self.raw.push(shown_ch);
        self.shown.push(shown_ch);
        self.cap_next = false;
        self.after_terminator = false;
        if shown_ch == ch {
            Reaction::Pass
        } else {
            Reaction::Replace { backspaces: 0, text: shown_ch.to_string() }
        }
    }

    /// Xử lý một phím ký tự (chữ hoặc số, đã tính Shift/CapsLock).
    pub fn process(&mut self, typed: char) -> Reaction {
        let horn_key = self.config.method == InputMethod::Telex
            && self.config.bracket_horn
            && matches!(typed, '[' | ']' | '{' | '}');
        if !(typed.is_ascii_alphanumeric() || horn_key) {
            self.reset();
            return Reaction::Pass;
        }
        let is_digit = typed.is_ascii_digit();
        if is_digit && self.config.method == InputMethod::Telex {
            self.cancel_context();
            return Reaction::Pass;
        }

        let ch = self.apply_auto_cap(typed);
        let key = ch.to_ascii_lowercase();
        let upper = ch.is_ascii_uppercase();
        self.raw.push(ch);
        self.cap_next = false;
        self.after_terminator = false;

        if horn_key {
            self.push_bracket(ch);
        } else if self.mode == Mode::Normal && self.command(key, upper) {
            // phím đã được dùng làm lệnh biến đổi
        } else if is_digit && self.mode == Mode::Normal {
            // Chữ số VNI không áp dụng được: là ký tự thường, kết thúc từ.
            self.reset();
            return Reaction::Pass;
        } else {
            self.syl.cells.push(Cell::new(key, upper));
        }

        self.syl.normalize();
        if self.mode == Mode::Normal && self.config.spell_check && !self.syl.is_valid() {
            self.mode = Mode::Restored;
        }

        let new_text = self.current_text();
        let reaction = self.diff(&self.shown, &new_text, typed);
        self.shown = new_text;
        reaction
    }

    /// Viết hoa chữ cái đầu câu nếu đang chờ.
    fn apply_auto_cap(&self, ch: char) -> char {
        if self.config.auto_capitalize && self.cap_next && ch.is_ascii_lowercase() {
            ch.to_ascii_uppercase()
        } else {
            ch
        }
    }

    /// `[` -> ơ, `]` -> ư (Telex).
    fn push_bracket(&mut self, ch: char) {
        if self.mode == Mode::Normal {
            let base = if matches!(ch, '[' | '{') { 'o' } else { 'u' };
            let mut cell = Cell::new(base, matches!(ch, '{' | '}'));
            cell.mark = Mark::Horn;
            self.syl.cells.push(cell);
        } else {
            self.syl.cells.push(Cell::new(ch, false));
        }
    }

    /// So sánh chuỗi cũ/mới, chỉ gửi phần khác biệt (đã mã hoá theo bảng mã) để giảm nhấp nháy.
    fn diff(&self, old: &str, new: &str, typed: char) -> Reaction {
        let charset = self.config.charset;
        let common = old.chars().zip(new.chars()).take_while(|(a, b)| a == b).count();
        let backspaces: usize = old.chars().skip(common).map(|c| charset.encoded_len(c)).sum();
        let tail: String = new.chars().skip(common).collect();
        let text = charset.encode(&tail);
        if backspaces == 0 && text.chars().eq(std::iter::once(typed)) {
            Reaction::Pass
        } else {
            Reaction::Replace { backspaces, text }
        }
    }

    /// Mở rộng gõ tắt cho từ đang hiển thị, nếu có.
    fn try_expand(&self) -> Reaction {
        if !self.config.abbreviations || self.shown.is_empty() {
            return Reaction::Pass;
        }
        let Some(expansion) = self.expansion_for(&self.shown) else {
            return Reaction::Pass;
        };
        if expansion == self.shown {
            return Reaction::Pass;
        }
        let charset = self.config.charset;
        Reaction::Replace {
            backspaces: self.shown.chars().map(|c| charset.encoded_len(c)).sum(),
            text: charset.encode(&expansion),
        }
    }

    fn expansion_for(&self, word: &str) -> Option<String> {
        if let Some(exact) = self.abbreviations.get(word) {
            return Some(exact.clone());
        }
        let expansion = self.abbreviations.get(&word.to_lowercase())?;
        // Chuyển kiểu hoa/thường của từ đã gõ sang cụm mở rộng: KO -> KHÔNG, Ko -> Không.
        let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
            Some(expansion.to_uppercase())
        } else if letters.first().is_some_and(|c| c.is_uppercase()) {
            let mut chars = expansion.chars();
            let first = chars.next()?;
            Some(first.to_uppercase().chain(chars).collect())
        } else {
            Some(expansion.clone())
        }
    }

    fn current_text(&self) -> String {
        match self.mode {
            Mode::Restored => self.raw.clone(),
            _ => self.syl.render(self.config.modern_tone),
        }
    }

    fn escape_with(&mut self, key: char, upper: bool) {
        self.mode = Mode::Escaped;
        self.syl.cells.push(Cell::new(key, upper));
    }

    /// Thử xem phím có phải lệnh của kiểu gõ hiện tại. `true` nếu phím đã được dùng.
    fn command(&mut self, key: char, upper: bool) -> bool {
        match self.config.method {
            InputMethod::Telex => match key {
                's' => self.set_tone(Tone::Acute, key, upper),
                'f' => self.set_tone(Tone::Grave, key, upper),
                'r' => self.set_tone(Tone::Hook, key, upper),
                'x' => self.set_tone(Tone::Tilde, key, upper),
                'j' => self.set_tone(Tone::Dot, key, upper),
                'z' => self.remove_tone(),
                'a' | 'e' | 'o' => self.apply_mark(Mark::Circumflex, &[key], key, upper, true),
                'd' => self.apply_stroke(key, upper, false),
                'w' => self.telex_w(key, upper),
                _ => false,
            },
            InputMethod::Vni => match key {
                '1' => self.set_tone(Tone::Acute, key, upper),
                '2' => self.set_tone(Tone::Grave, key, upper),
                '3' => self.set_tone(Tone::Hook, key, upper),
                '4' => self.set_tone(Tone::Tilde, key, upper),
                '5' => self.set_tone(Tone::Dot, key, upper),
                '0' => self.remove_tone(),
                '6' => self.apply_mark(Mark::Circumflex, &['a', 'e', 'o'], key, upper, false),
                '7' => self.apply_mark(Mark::Horn, &['o', 'u'], key, upper, false),
                '8' => self.apply_mark(Mark::Breve, &['a'], key, upper, false),
                '9' => self.apply_stroke(key, upper, true),
                _ => false,
            },
        }
    }

    fn set_tone(&mut self, tone: Tone, key: char, upper: bool) -> bool {
        if !self.syl.has_vowel() {
            return false;
        }
        if self.syl.tone == tone {
            // Gõ lại cùng dấu: hoàn tác (ass -> as).
            self.syl.tone = Tone::None;
            self.escape_with(key, upper);
        } else {
            self.syl.tone = tone;
        }
        true
    }

    fn remove_tone(&mut self) -> bool {
        if self.syl.tone == Tone::None {
            return false;
        }
        self.syl.tone = Tone::None;
        true
    }

    /// Gắn dấu phụ vào chữ cái đích gần nhất, hoặc hoàn tác nếu đã có dấu đó.
    /// `strict_tail`: (Telex) chỉ nhận chữ đích khi phía sau nó không còn nguyên âm.
    fn apply_mark(
        &mut self,
        mark: Mark,
        targets: &[char],
        key: char,
        upper: bool,
        strict_tail: bool,
    ) -> bool {
        let plain =
            self.syl.cells.iter().rposition(|c| {
                targets.contains(&c.base) && c.mark == Mark::None && c.can_take(mark)
            });
        if let Some(i) = plain {
            let tail_ok = !strict_tail || self.syl.cells[i + 1..].iter().all(|c| !c.is_vowel());
            if tail_ok {
                self.syl.cells[i].mark = mark;
                return true;
            }
        }
        let marked =
            self.syl.cells.iter().rposition(|c| targets.contains(&c.base) && c.mark == mark);
        if let Some(i) = marked {
            if !strict_tail || i + 1 == self.syl.cells.len() {
                self.syl.cells[i].mark = Mark::None;
                if mark == Mark::Horn
                    && i > 0
                    && self.syl.cells[i].base == 'o'
                    && self.syl.cells[i - 1].base == 'u'
                    && self.syl.cells[i - 1].mark == Mark::Horn
                {
                    self.syl.cells[i - 1].mark = Mark::None; // hoàn tác cả "ươ"
                }
                self.escape_with(key, upper);
                return true;
            }
        }
        false
    }

    /// dd -> đ (Telex) hoặc d9 -> đ (VNI). `free`: cho phép gõ dấu sau các chữ khác (dau9).
    fn apply_stroke(&mut self, key: char, upper: bool, free: bool) -> bool {
        let Some(first) = self.syl.cells.first().copied() else {
            return false;
        };
        if first.base != 'd' || !(free || self.syl.cells.len() == 1) {
            return false;
        }
        match first.mark {
            Mark::None => {
                self.syl.cells[0].mark = Mark::Stroke;
                true
            }
            Mark::Stroke => {
                self.syl.cells[0].mark = Mark::None;
                self.escape_with(key, upper);
                true
            }
            _ => false,
        }
    }

    /// Phím `w` của Telex: aw -> ă, ow -> ơ, uw -> ư, uow -> ươ, w đứng riêng -> ư.
    fn telex_w(&mut self, key: char, upper: bool) -> bool {
        let found = self.syl.cells.iter().rposition(|c| matches!(c.base, 'a' | 'o' | 'u'));
        let Some(mut i) = found else {
            if self.syl.has_vowel() || self.config.allow_fjwz {
                return false;
            }
            let mut cell = Cell::new('u', upper);
            cell.mark = Mark::Horn;
            self.syl.cells.push(cell);
            return true;
        };

        // "ua" + w -> "ưa" (mưa), trừ "qua" (quă).
        let cells = &self.syl.cells;
        if cells[i].base == 'a'
            && cells[i].mark == Mark::None
            && i > 0
            && cells[i - 1].base == 'u'
            && matches!(cells[i - 1].mark, Mark::None | Mark::Horn)
            && !(i >= 2 && cells[i - 2].base == 'q')
        {
            i -= 1;
        }

        let want = if self.syl.cells[i].base == 'a' { Mark::Breve } else { Mark::Horn };
        if self.syl.cells[i].mark == want {
            // Gõ lại w: hoàn tác (aww -> aw).
            self.syl.cells[i].mark = Mark::None;
            if self.syl.cells[i].base == 'o'
                && i > 0
                && self.syl.cells[i - 1].base == 'u'
                && self.syl.cells[i - 1].mark == Mark::Horn
            {
                self.syl.cells[i - 1].mark = Mark::None;
            }
            self.escape_with(key, upper);
            return true;
        }
        self.syl.cells[i].mark = want;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mô phỏng màn hình: áp dụng Reaction lên một chuỗi, giống tầng hệ điều hành.
    fn run_with(config: EngineConfig, abbr: &[(&str, &str)], input: &str) -> String {
        let mut engine = Engine::new(config);
        engine
            .set_abbreviations(abbr.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect());
        let mut screen = String::new();
        for ch in input.chars() {
            let is_key = ch.is_ascii_alphanumeric()
                || (config.bracket_horn && matches!(ch, '[' | ']' | '{' | '}'));
            let reaction = if is_key {
                engine.process(ch)
            } else {
                engine.boundary(match ch {
                    ' ' => Boundary::Space,
                    '\n' => Boundary::Enter,
                    c if c.is_ascii_punctuation() => Boundary::Punct(c),
                    _ => Boundary::Other,
                })
            };
            match reaction {
                Reaction::Pass => screen.push(ch),
                Reaction::Replace { backspaces, text } => {
                    for _ in 0..backspaces {
                        screen.pop();
                    }
                    screen.push_str(&text);
                    if !is_key {
                        screen.push(ch); // phím kết thúc được gửi lại sau phần thay thế
                    }
                }
            }
        }
        screen
    }

    fn run(config: EngineConfig, input: &str) -> String {
        run_with(config, &[], input)
    }

    fn telex(input: &str) -> String {
        run(EngineConfig::default(), input)
    }

    fn telex_old(input: &str) -> String {
        run(EngineConfig { modern_tone: false, ..EngineConfig::default() }, input)
    }

    fn vni(input: &str) -> String {
        run(EngineConfig { method: InputMethod::Vni, ..EngineConfig::default() }, input)
    }

    #[test]
    fn telex_basic_words() {
        assert_eq!(telex("vieetj"), "việt");
        assert_eq!(telex("Vieetj"), "Việt");
        assert_eq!(telex("VIEETJ"), "VIỆT");
        assert_eq!(telex("tooi"), "tôi");
        assert_eq!(telex("toans"), "toán");
        assert_eq!(telex("tuyeenx"), "tuyễn");
        assert_eq!(telex("xin chaof cacs banj"), "xin chào các bạn");
    }

    #[test]
    fn telex_horn_and_breve() {
        assert_eq!(telex("nguwowif"), "người");
        assert_eq!(telex("dduwowcj"), "được");
        assert_eq!(telex("duocwj"), "dược");
        assert_eq!(telex("muaw"), "mưa");
        assert_eq!(telex("hoawcj"), "hoặc");
        assert_eq!(telex("thuowr"), "thuở");
        assert_eq!(telex("w"), "ư");
    }

    #[test]
    fn telex_stroke() {
        assert_eq!(telex("dd"), "đ");
        assert_eq!(telex("Dd"), "Đ");
        assert_eq!(telex("ddaau"), "đâu");
    }

    #[test]
    fn tone_placement_styles() {
        assert_eq!(telex("hoaf"), "hoà");
        assert_eq!(telex_old("hoaf"), "hòa");
        assert_eq!(telex("khoer"), "khoẻ");
        assert_eq!(telex_old("khoer"), "khỏe");
        assert_eq!(telex("tuys"), "tuý");
        assert_eq!(telex_old("tuys"), "túy");
        assert_eq!(telex("quas"), "quá");
        assert_eq!(telex("gif"), "gì");
        assert_eq!(telex("gias"), "giá");
    }

    #[test]
    fn undo_and_remove() {
        assert_eq!(telex("aaa"), "aa");
        assert_eq!(telex("asss"), "ass");
        assert_eq!(telex("asz"), "a");
        assert_eq!(telex("aww"), "aw");
    }

    #[test]
    fn spell_check_restores_foreign_words() {
        assert_eq!(telex("windows"), "windows");
        assert_eq!(telex("hello"), "hello");
    }

    #[test]
    fn vni_basic() {
        assert_eq!(vni("viet65"), "việt");
        assert_eq!(vni("d9"), "đ");
        assert_eq!(vni("dau9"), "đau");
        assert_eq!(vni("a66"), "a6");
        assert_eq!(vni("nguoi72"), "người");
    }

    #[test]
    fn backspace_keeps_state_in_sync() {
        let mut e = Engine::new(EngineConfig::default());
        e.process('a');
        e.process('s'); // á
        assert_eq!(e.backspace(), Reaction::Pass);
        assert_eq!(e.process('a'), Reaction::Pass);
    }

    #[test]
    fn bracket_keys_make_horn_vowels() {
        let cfg = EngineConfig { bracket_horn: true, ..EngineConfig::default() };
        assert_eq!(run(cfg, "["), "ơ");
        assert_eq!(run(cfg, "]"), "ư");
        assert_eq!(run(cfg, "{"), "Ơ");
        assert_eq!(run(cfg, "ng][if"), "người");
        assert_eq!(run(cfg, "dd][cj"), "được");
        // Khi tắt tuỳ chọn, `[` chỉ là ký tự thường.
        assert_eq!(telex("a[b"), "a[b");
    }

    #[test]
    fn abbreviations_expand_on_boundary() {
        let cfg = EngineConfig { abbreviations: true, ..EngineConfig::default() };
        let table = [("ko", "không"), ("dc", "được")];
        assert_eq!(run_with(cfg, &table, "ko dc "), "không được ");
        assert_eq!(run_with(cfg, &table, "Ko"), "Ko"); // chưa gặp phím kết thúc
        assert_eq!(run_with(cfg, &table, "Ko "), "Không ");
        assert_eq!(run_with(cfg, &table, "KO "), "KHÔNG ");
        assert_eq!(run_with(cfg, &table, "ko."), "không.");
        assert_eq!(run_with(cfg, &table, "ko\n"), "không\n");
        // Chỉ khớp cả từ, không khớp một phần.
        assert_eq!(run_with(cfg, &table, "kot "), "kot ");
        // Tắt gõ tắt thì giữ nguyên.
        assert_eq!(run_with(EngineConfig::default(), &table, "ko "), "ko ");
    }

    #[test]
    fn abbreviation_matches_visible_text_after_telex() {
        let cfg = EngineConfig { abbreviations: true, ..EngineConfig::default() };
        let table = [("việt", "Việt Nam")];
        assert_eq!(run_with(cfg, &table, "vieetj "), "Việt Nam ");
    }

    #[test]
    fn auto_capitalize_after_sentence_end() {
        let cfg = EngineConfig { auto_capitalize: true, ..EngineConfig::default() };
        assert_eq!(run(cfg, "hello. world"), "hello. World");
        assert_eq!(run(cfg, "ok! abc"), "ok! Abc");
        assert_eq!(run(cfg, "ok?  abc"), "ok?  Abc");
        assert_eq!(run(cfg, "ok\nabc"), "ok\nAbc");
        assert_eq!(run(cfg, "a, b"), "a, b");
        assert_eq!(run(cfg, "hello world"), "hello world");
        // Kết hợp với Telex: chữ đầu câu hoa, phần còn lại vẫn biến đổi.
        assert_eq!(run(cfg, "ok. vieetj"), "ok. Việt");
        // Tắt tuỳ chọn thì không viết hoa.
        assert_eq!(telex("hello. world"), "hello. world");
    }

    #[test]
    fn plain_mode_still_expands_abbreviations() {
        let cfg = EngineConfig { abbreviations: true, ..EngineConfig::default() };
        let mut e = Engine::new(cfg);
        e.set_abbreviations([("ko".to_string(), "không".to_string())].into());
        assert_eq!(e.process_plain('k'), Reaction::Pass);
        assert_eq!(e.process_plain('o'), Reaction::Pass);
        assert_eq!(
            e.boundary(Boundary::Space),
            Reaction::Replace { backspaces: 2, text: "không".to_string() }
        );
    }

    #[test]
    fn viqr_output_counts_screen_characters() {
        let cfg = EngineConfig { charset: Charset::Viqr, ..EngineConfig::default() };
        assert_eq!(run(cfg, "vieetj"), "vie^.t");
        assert_eq!(run(cfg, "Vieetj"), "Vie^.t");
        assert_eq!(run(cfg, "dd"), "dd");
        assert_eq!(run(cfg, "nguwowif"), "ngu+o+`i");
    }

    #[test]
    fn legacy_charsets_end_to_end() {
        let tcvn = EngineConfig { charset: Charset::Tcvn3, ..EngineConfig::default() };
        assert_eq!(run(tcvn, "dd"), "\u{AE}");
        assert_eq!(run(tcvn, "vieetj"), Charset::Tcvn3.encode("việt"));
        let vni_win = EngineConfig { charset: Charset::VniWindows, ..EngineConfig::default() };
        assert_eq!(run(vni_win, "vieetj"), Charset::VniWindows.encode("việt"));
        assert_eq!(run(vni_win, "xin chaof"), Charset::VniWindows.encode("xin chào"));
        let compound =
            EngineConfig { charset: Charset::UnicodeCompound, ..EngineConfig::default() };
        assert_eq!(run(compound, "vieetj"), Charset::UnicodeCompound.encode("việt"));
    }

    #[test]
    fn backspace_deletes_whole_multi_char_letter() {
        let cfg = EngineConfig { charset: Charset::VniWindows, ..EngineConfig::default() };
        let mut e = Engine::new(cfg);
        e.process('e');
        e.process('e'); // ê
        e.process('s'); // ế = 2 ký tự trong VNI Windows
        assert_eq!(e.backspace(), Reaction::Replace { backspaces: 2, text: String::new() });
        // Ký tự một byte thì để ứng dụng tự xoá.
        let mut u = Engine::new(EngineConfig::default());
        u.process('a');
        assert_eq!(u.backspace(), Reaction::Pass);
    }

    // ---- Kiểm tra theo từ điển: sinh phím từ chữ đúng rồi so lại ----

    use crate::syllable::{Mark, Tone, decompose};

    /// Các từ không phụ thuộc kiểu đặt dấu (hoà/hòa), nên dùng chung cho cả hai kiểu.
    const WORDS: &[&str] = &[
        "việt", "nam", "người", "được", "thuở", "quyền", "giữ", "ngoài", "hoạt", "tuyết",
        "nghiêng", "chuyện", "truyền", "nguyễn", "uống", "mượn", "hiểu", "yêu", "bầu", "cười",
        "trường", "thương", "nước", "bước", "đường", "muốn", "chuông", "buồn", "tuổi", "cũng",
        "những", "khách", "bạn", "chào", "các", "xin", "cảm", "ơn", "tôi", "là", "một", "anh",
        "em", "ngày", "tháng", "năm", "học", "sinh", "giáo", "viên", "trời", "mưa", "gió", "bão",
        "lửa", "sông", "núi", "biển", "rừng", "hoa", "quả", "cây", "nhà", "cửa", "sổ", "bàn",
        "ghế", "sách", "vở", "bút", "thước", "kẻ", "quê", "quyết", "quán", "giá", "giờ", "giúp",
        "giường", "gìn", "gì", "ăn", "lắm", "mặt", "cặp", "đầu", "tấm", "hấp", "bậc", "lấy",
        "máy", "tay", "đây", "yến", "ý", "xuyên", "khuyên", "nghề", "nghĩa", "mía", "múa", "mùa",
        "của", "chị", "mẹ", "không", "tiếng", "đời", "thời", "hoàng", "toàn", "loài", "hoặc",
        "đoàn", "khoảng", "toán", "quốc", "đẹp", "ông", "ừ", "ư", "đ", "đã", "đi", "để", "đến",
        "đúng", "sáng", "chiều", "tối", "đêm", "yêu", "kiến", "thức", "lập", "trình", "máy",
        "tính", "phần", "mềm", "gõ", "chữ", "dấu", "thanh", "sắc", "huyền", "hỏi", "ngã", "nặng",
        "ngủ", "nghỉ", "nghe", "nhìn", "nói", "viết", "đọc", "hiểu", "biết", "thấy", "muộn",
        "luôn", "cuộc", "cuối", "tuần", "tuyệt", "vời", "ruộng", "lượng", "quạt", "quặn", "giặt",
        "giếng", "giọng", "ngượng", "nhiều", "nhiệt", "khiêm", "thiết", "kiểm", "điều", "diễn",
        "liên", "miền", "tiền", "hiện", "nghiệp", "chiến", "thuật", "tuyển", "chọn", "trọng",
        // Các trường hợp từng bị coi là sai chính tả.
        "lưu", "hưu", "cứu", "mưu", "ngưu", "tựu", "tửu", "trừu", "sưu", "ưu", "ngía", "ngịu",
        "quới", "quờn", "khuều",
    ];

    #[derive(Clone, Copy)]
    enum ToneAt {
        /// Dấu thanh gõ ở cuối âm tiết (vieetj).
        End,
        /// Dấu thanh gõ ngay sau cụm nguyên âm, trước phụ âm cuối (vieejt).
        AfterNucleus,
    }

    fn is_vowel_char(c: char) -> bool {
        decompose(c).is_some_and(|d| d.base != 'd')
    }

    /// Dựng chuỗi phím từ một từ tiếng Việt đúng chính tả.
    fn keys_for(word: &str, method: InputMethod, at: ToneAt) -> String {
        let chars: Vec<char> = word.chars().collect();
        let mut parts: Vec<String> = Vec::new();
        let mut tone: Option<(usize, char)> = None;
        for (i, &c) in chars.iter().enumerate() {
            let Some(d) = decompose(c) else {
                parts.push(c.to_string());
                continue;
            };
            let mut s = String::new();
            s.push(if d.upper { d.base.to_ascii_uppercase() } else { d.base });
            match (method, d.mark) {
                (_, Mark::None) => {}
                (InputMethod::Telex, Mark::Circumflex) => s.push(d.base),
                (InputMethod::Telex, Mark::Breve | Mark::Horn) => s.push('w'),
                (InputMethod::Telex, Mark::Stroke) => s.push('d'),
                (InputMethod::Vni, Mark::Circumflex) => s.push('6'),
                (InputMethod::Vni, Mark::Horn) => s.push('7'),
                (InputMethod::Vni, Mark::Breve) => s.push('8'),
                (InputMethod::Vni, Mark::Stroke) => s.push('9'),
            }
            let key = match (method, d.tone) {
                (_, Tone::None) => None,
                (InputMethod::Telex, Tone::Acute) => Some('s'),
                (InputMethod::Telex, Tone::Grave) => Some('f'),
                (InputMethod::Telex, Tone::Hook) => Some('r'),
                (InputMethod::Telex, Tone::Tilde) => Some('x'),
                (InputMethod::Telex, Tone::Dot) => Some('j'),
                (InputMethod::Vni, Tone::Acute) => Some('1'),
                (InputMethod::Vni, Tone::Grave) => Some('2'),
                (InputMethod::Vni, Tone::Hook) => Some('3'),
                (InputMethod::Vni, Tone::Tilde) => Some('4'),
                (InputMethod::Vni, Tone::Dot) => Some('5'),
            };
            if let Some(k) = key {
                tone = Some((i, k));
            }
            parts.push(s);
        }
        if let Some((i, key)) = tone {
            match at {
                ToneAt::End => parts.push(key.to_string()),
                ToneAt::AfterNucleus => {
                    let mut j = i;
                    while j + 1 < chars.len() && is_vowel_char(chars[j + 1]) {
                        j += 1;
                    }
                    parts[j].push(key);
                }
            }
        }
        parts.concat()
    }

    fn check_words(words: &[String], method: InputMethod, failures: &mut Vec<String>) {
        let config = EngineConfig { method, ..EngineConfig::default() };
        for word in words {
            for at in [ToneAt::End, ToneAt::AfterNucleus] {
                let keys = keys_for(word, method, at);
                let got = run(config, &keys);
                if got != *word {
                    failures.push(format!("{word}: gõ `{keys}` ra `{got}`"));
                }
            }
        }
    }

    #[test]
    fn dictionary_round_trip_in_every_style() {
        let mut variants: Vec<String> = Vec::new();
        for w in WORDS {
            variants.push((*w).to_owned());
            variants.push(w.to_uppercase());
            let mut cs = w.chars();
            if let Some(first) = cs.next() {
                variants.push(first.to_uppercase().chain(cs).collect());
            }
        }
        let mut failures = Vec::new();
        check_words(&variants, InputMethod::Telex, &mut failures);
        check_words(&variants, InputMethod::Vni, &mut failures);
        assert!(failures.is_empty(), "{} lỗi:\n{}", failures.len(), failures.join("\n"));
    }

    #[test]
    fn sentences_round_trip() {
        let sentence = "tiếng việt rất đẹp và người việt nam yêu quý nó";
        let keys: String = sentence
            .split(' ')
            .map(|w| keys_for(w, InputMethod::Telex, ToneAt::End))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(telex(&keys), sentence);
        let keys: String = sentence
            .split(' ')
            .map(|w| keys_for(w, InputMethod::Vni, ToneAt::AfterNucleus))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(vni(&keys), sentence);
    }

    /// Mô phỏng người gõ có dùng Backspace: `\u{8}` là phím Backspace.
    fn type_with_backspace(config: EngineConfig, input: &str) -> String {
        let mut engine = Engine::new(config);
        let mut screen = String::new();
        let apply = |screen: &mut String, reaction: Reaction, key: Option<char>| match reaction {
            Reaction::Pass => {
                // Phím được để đi qua: ứng dụng tự xử lý (gõ thêm chữ hoặc xoá một ký tự).
                match key {
                    Some(c) => screen.push(c),
                    None => {
                        screen.pop();
                    }
                }
            }
            Reaction::Replace { backspaces, text } => {
                for _ in 0..backspaces {
                    screen.pop();
                }
                screen.push_str(&text);
            }
        };
        for ch in input.chars() {
            if ch == '\u{8}' {
                let reaction = engine.backspace();
                apply(&mut screen, reaction, None);
            } else {
                let reaction = engine.process(ch);
                apply(&mut screen, reaction, Some(ch));
            }
        }
        screen
    }

    #[test]
    fn backspace_in_the_middle_of_a_word_keeps_text_correct() {
        let cfg = EngineConfig::default();
        // Xoá chữ cuối rồi gõ lại.
        assert_eq!(type_with_backspace(cfg, "vieetj\u{8}t"), "việt");
        assert_eq!(type_with_backspace(cfg, "nguwowif\u{8}i"), "người");
        assert_eq!(type_with_backspace(cfg, "toans\u{8}n"), "toán");
        // Xoá cả chữ có dấu rồi gõ chữ khác.
        assert_eq!(type_with_backspace(cfg, "as\u{8}e"), "e");
        assert_eq!(type_with_backspace(cfg, "toi\u{8}\u{8}ooi"), "tôi");
        // Xoá hai ký tự cuối của "người" thì dấu huyền vẫn nằm trên chữ còn lại.
        assert_eq!(type_with_backspace(cfg, "nguwowif\u{8}\u{8}i"), "ngừi");
    }

    #[test]
    fn words_that_were_wrongly_restored() {
        assert_eq!(telex("luwu"), "lưu");
        assert_eq!(telex("cuwus"), "cứu");
        assert_eq!(telex("ngias"), "ngía");
        assert_eq!(telex("quowis"), "quới");
        assert_eq!(telex("ngiuj"), "ngịu");
        assert_eq!(vni("luw"), "luw");
        assert_eq!(vni("lu7u"), "lưu");
    }
}