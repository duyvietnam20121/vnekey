//! Lập kế hoạch gửi phím: xoá phần cũ, gõ phần mới.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Nhấn Backspace n lần.
    Backspace(usize),
    /// Chọn n ký tự bằng Shift+← (sau đó chữ gõ tiếp theo sẽ ghi đè phần chọn).
    ShiftLeft(usize),
    /// Gõ từng ký tự Unicode.
    Type(String),
    /// Đưa chuỗi vào Clipboard rồi nhấn Ctrl+V.
    Paste(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SendOptions {
    pub use_clipboard: bool,
    pub select_with_shift: bool,
}

pub fn plan(backspaces: usize, text: &str, opts: SendOptions) -> Vec<Step> {
    let mut steps = Vec::new();
    if backspaces > 0 {
        // Chọn bằng Shift+← chỉ có ý nghĩa khi có chữ gõ đè lên; xoá thuần túy phải dùng Backspace.
        if opts.select_with_shift && !text.is_empty() {
            steps.push(Step::ShiftLeft(backspaces));
        } else {
            steps.push(Step::Backspace(backspaces));
        }
    }
    if !text.is_empty() {
        steps.push(if opts.use_clipboard {
            Step::Paste(text.to_owned())
        } else {
            Step::Type(text.to_owned())
        });
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    const STANDARD: SendOptions = SendOptions { use_clipboard: false, select_with_shift: false };

    #[test]
    fn standard_is_backspace_then_type() {
        assert_eq!(plan(2, "ệt", STANDARD), vec![Step::Backspace(2), Step::Type("ệt".into())]);
        assert_eq!(plan(0, "ư", STANDARD), vec![Step::Type("ư".into())]);
        assert!(plan(0, "", STANDARD).is_empty());
    }

    #[test]
    fn shift_select_replaces_backspace_when_typing() {
        let opts = SendOptions { select_with_shift: true, ..STANDARD };
        assert_eq!(plan(1, "ê", opts), vec![Step::ShiftLeft(1), Step::Type("ê".into())]);
        // Chỉ xoá, không có chữ thay: vẫn phải Backspace.
        assert_eq!(plan(2, "", opts), vec![Step::Backspace(2)]);
    }

    #[test]
    fn clipboard_pastes_instead_of_typing() {
        let opts = SendOptions { use_clipboard: true, ..STANDARD };
        assert_eq!(plan(1, "ê", opts), vec![Step::Backspace(1), Step::Paste("ê".into())]);
        let both = SendOptions { use_clipboard: true, select_with_shift: true };
        assert_eq!(plan(1, "ê", both), vec![Step::ShiftLeft(1), Step::Paste("ê".into())]);
    }
}
