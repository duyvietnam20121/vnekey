//! Cửa sổ cài đặt (egui) với 4 tab: Cơ bản, Gõ tắt, Phím tắt, Hệ thống.
//!
//! Chạy trong tiến trình riêng (`vnekey --settings`) nên đóng cửa sổ không ảnh hưởng đến
//! bộ gõ. Bấm "Lưu" ghi `config.toml`; tiến trình khay tự nạp lại trong vòng một giây.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText};
use vnekey_config::{Config, Hotkey, format_abbreviations, parse_abbreviations, parse_exe_list};
use vnekey_core::{Charset, InputMethod};
use vnekey_win::process::SETTINGS_TITLE;

/// Màu nhấn, trùng với màu nền biểu tượng V ở khay hệ thống.
const ACCENT: Color32 = Color32::from_rgb(46, 160, 67);
const WARN: Color32 = Color32::from_rgb(230, 170, 60);
const ERROR: Color32 = Color32::from_rgb(235, 100, 100);

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(SETTINGS_TITLE)
            .with_inner_size([600.0, 700.0])
            .with_min_inner_size([520.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native("vnekey", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Basic,
    Abbreviations,
    Hotkeys,
    System,
}

impl Tab {
    const ALL: [(Tab, &'static str); 4] = [
        (Tab::Basic, "Cơ bản"),
        (Tab::Abbreviations, "Gõ tắt"),
        (Tab::Hotkeys, "Phím tắt"),
        (Tab::System, "Hệ thống"),
    ];
}

struct Status {
    ok: bool,
    text: String,
}

struct App {
    /// Bản đang chỉnh; chỉ có hiệu lực sau khi bấm "Lưu".
    draft: Config,
    /// Bản đã lưu gần nhất, để biết có thay đổi chưa lưu và có cần bật/tắt khởi động cùng Windows.
    saved: Config,
    abbreviations_text: String,
    excluded_text: String,
    /// Ô gõ thử: chữ gõ vào đây đi qua bộ gõ thật nên cho thấy ngay cài đặt hoạt động ra sao.
    try_text: String,
    tab: Tab,
    status: Option<Status>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        apply_style(&cc.egui_ctx);
        let config = Config::load();
        Self {
            abbreviations_text: format_abbreviations(&config.abbreviations),
            excluded_text: config.excluded_apps.join("\n"),
            saved: config.clone(),
            draft: config,
            try_text: String::new(),
            tab: Tab::Basic,
            status: None,
        }
    }

    /// Có thay đổi chưa lưu không (kể cả trong hai ô văn bản).
    fn is_dirty(&self) -> bool {
        self.draft != self.saved
            || parse_abbreviations(&self.abbreviations_text) != self.saved.abbreviations
            || parse_exe_list(&self.excluded_text) != self.saved.excluded_apps
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        section(ui, "Điều khiển", |ui| {
            egui::Grid::new("control").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
                ui.label("Bảng mã");
                egui::ComboBox::from_id_salt("charset")
                    .width(200.0)
                    .selected_text(self.draft.charset.name())
                    .show_ui(ui, |ui| {
                        for charset in Charset::ALL {
                            ui.selectable_value(&mut self.draft.charset, charset, charset.name());
                        }
                    });
                ui.end_row();

                ui.label("Kiểu gõ");
                egui::ComboBox::from_id_salt("method")
                    .width(200.0)
                    .selected_text(method_name(self.draft.method))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.draft.method, InputMethod::Telex, "Telex");
                        ui.selectable_value(&mut self.draft.method, InputMethod::Vni, "VNI");
                    });
                ui.end_row();

                ui.label("Cơ chế");
                ui.horizontal(|ui| {
                    let _ = ui.radio(true, "Keyboard hook");
                    ui.add_enabled(false, egui::RadioButton::new(false, "IME (sắp có)"));
                });
                ui.end_row();
            });
            hint(
                ui,
                "Bảng mã cũ (TCVN3, VNI Windows) cần font tương ứng như .VnTime hoặc VNI-Times.",
            );
        });
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (tab, label) in Tab::ALL {
                let selected = self.tab == tab;
                let text = if selected {
                    RichText::new(label).strong().color(ACCENT)
                } else {
                    RichText::new(label)
                };
                if ui.selectable_label(selected, text).clicked() {
                    self.tab = tab;
                }
            }
        });
        ui.separator();
    }

    fn tab_basic(&mut self, ui: &mut egui::Ui) {
        section(ui, "Chính tả và đặt dấu", |ui| {
            let d = &mut self.draft;
            ui.checkbox(&mut d.spell_check, "Kiểm tra chính tả (tự khôi phục phím với từ sai)");
            ui.checkbox(&mut d.modern_tone, "Đặt dấu kiểu mới (hoà, thuý thay vì hòa, thúy)");
            ui.checkbox(&mut d.allow_fjwz, "Cho phép 'f j w z' làm phụ âm");
            ui.checkbox(
                &mut d.auto_capitalize,
                "Tự động viết hoa chữ đầu câu (sau . ! ? và Enter)",
            );
            ui.checkbox(&mut d.bracket_horn, "Telex: dùng [ ] cho ơ ư");
        });

        section(ui, "Cách gửi phím", |ui| {
            let d = &mut self.draft;
            ui.checkbox(
                &mut d.select_with_shift,
                "Chọn chữ bằng Shift+← thay vì Backspace (sửa lỗi thanh địa chỉ Chrome/Edge)",
            );
            ui.checkbox(
                &mut d.use_clipboard,
                "Sử dụng Clipboard để gửi phím (cho ứng dụng không nhận được chữ Unicode)",
            );
            hint(
                ui,
                "Nếu gõ ở trình duyệt bị lặp hoặc mất chữ, thử bật tuỳ chọn đầu tiên. \
                 Lưu ý: tuỳ chọn Clipboard tạm thời thay nội dung Clipboard trong lúc gõ.",
            );
        });

        section(ui, "Gõ thử", |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.try_text)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("Gõ thử ở đây, ví dụ: Vieetj Nam"),
            );
            ui.horizontal(|ui| {
                if ui.button("Xoá").clicked() {
                    self.try_text.clear();
                }
                hint(
                    ui,
                    "Chỉ hoạt động khi vnekey đang chạy ở khay và đang ở chế độ V. \
                     Cài đặt chưa lưu chưa áp dụng.",
                );
            });
        });
    }

    fn tab_abbreviations(&mut self, ui: &mut egui::Ui) {
        section(ui, "Gõ tắt", |ui| {
            ui.checkbox(&mut self.draft.abbreviations_enabled, "Cho phép gõ tắt");
            ui.add_enabled(
                self.draft.abbreviations_enabled,
                egui::Checkbox::new(
                    &mut self.draft.abbreviations_in_english,
                    "Vẫn cho phép gõ tắt khi đang tắt tiếng Việt (E)",
                ),
            );
        });
        section(ui, "Bảng gõ tắt", |ui| {
            hint(ui, "Mỗi dòng một mục: từ gõ = cụm mở rộng. Dòng bắt đầu bằng # là ghi chú.");
            ui.add(
                egui::TextEdit::multiline(&mut self.abbreviations_text)
                    .desired_rows(12)
                    .desired_width(f32::INFINITY)
                    .hint_text("ko = không\ndc = được\nvn = Việt Nam"),
            );
            hint(
                ui,
                "Gõ tắt kích hoạt khi bấm Space, Enter hoặc dấu câu. \"Ko\" mở rộng thành \
                 \"Không\", \"KO\" thành \"KHÔNG\".",
            );
        });
    }

    fn tab_hotkeys(&mut self, ui: &mut egui::Ui) {
        section(ui, "Phím chuyển Việt/Anh", |ui| {
            let hk = &mut self.draft.hotkey;
            ui.horizontal(|ui| {
                ui.checkbox(&mut hk.ctrl, "Ctrl");
                ui.checkbox(&mut hk.shift, "Shift");
                ui.checkbox(&mut hk.alt, "Alt");
                ui.checkbox(&mut hk.win, "Win");
                ui.label("+");
                egui::ComboBox::from_id_salt("hotkey_key")
                    .width(90.0)
                    .selected_text(key_label(hk.key))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut hk.key, None, "Không");
                        ui.selectable_value(&mut hk.key, Some(' '), "Space");
                        for c in ('A'..='Z').chain('0'..='9') {
                            ui.selectable_value(&mut hk.key, Some(c), c.to_string());
                        }
                    });
            });
            ui.add_space(4.0);
            if hk.has_modifier() {
                ui.label(format!("Hiện tại: {}", hotkey_text(hk)));
                if hk.key.is_none() {
                    hint(ui, "Không chọn phím chính: nhấn rồi nhả tổ hợp để chuyển (như UniKey).");
                }
            } else {
                ui.colored_label(ERROR, "Cần chọn ít nhất một phím Ctrl/Shift/Alt/Win.");
            }
        });
        hint(ui, "Các phím tắt F1-F12 như EVKey sẽ bổ sung ở bản sau.");
    }

    fn tab_system(&mut self, ui: &mut egui::Ui) {
        section(ui, "Khởi động", |ui| {
            let d = &mut self.draft;
            ui.checkbox(&mut d.start_vietnamese, "Mở app ở chế độ tiếng Việt");
            ui.checkbox(&mut d.autostart, "Khởi động cùng Windows");
            ui.checkbox(&mut d.run_as_admin, "Chạy với quyền admin (hiện hộp thoại UAC khi mở)");
            hint(
                ui,
                "Quyền admin giúp gõ được trong ứng dụng đang chạy bằng quyền admin. Khi bật cùng \
                 \"Khởi động cùng Windows\", vnekey dùng Task Scheduler; cần mở cài đặt từ bản \
                 đang chạy bằng quyền admin để tạo tác vụ.",
            );
        });

        section(ui, "Loại trừ ứng dụng", |ui| {
            hint(
                ui,
                "Mỗi dòng một tên file .exe. vnekey không can thiệp khi các ứng dụng này đang focus.",
            );
            ui.add(
                egui::TextEdit::multiline(&mut self.excluded_text)
                    .desired_rows(6)
                    .desired_width(f32::INFINITY)
                    .hint_text("game.exe"),
            );
        });

        section(ui, "Về vnekey", |ui| {
            ui.label(format!("Phiên bản {}", env!("CARGO_PKG_VERSION")));
            if let Some(path) = Config::path() {
                hint(ui, &format!("Cấu hình: {}", path.display()));
            }
            if ui.button("Mở thư mục cấu hình").clicked() {
                if let Some(dir) = Config::dir() {
                    let _ = std::process::Command::new("explorer").arg(dir).spawn();
                }
            }
            hint(
                ui,
                "Biểu tượng khay: bấm trái để đổi V/E, bấm đúp để mở cài đặt, bấm phải để mở menu.",
            );
        });
    }

    fn footer(&mut self, ui: &mut egui::Ui, dirty: bool) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Thiết lập mặc định").clicked() {
                self.draft = Config::default();
                self.abbreviations_text.clear();
                self.excluded_text.clear();
                self.status = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let save = egui::Button::new(RichText::new("Lưu").strong().color(Color32::WHITE))
                    .fill(ACCENT);
                if ui.add_enabled(dirty, save).clicked() {
                    self.save();
                }
                if dirty {
                    ui.label(RichText::new("● Chưa lưu").color(WARN));
                } else if let Some(status) = &self.status {
                    let color = if status.ok { ACCENT } else { ERROR };
                    ui.label(RichText::new(&status.text).color(color));
                }
            });
        });
        ui.add_space(8.0);
    }

    fn save(&mut self) {
        if !self.draft.hotkey.has_modifier() {
            self.status = Some(Status {
                ok: false,
                text: "Phím chuyển cần ít nhất một phím Ctrl/Shift/Alt/Win.".to_owned(),
            });
            return;
        }
        self.draft.abbreviations = parse_abbreviations(&self.abbreviations_text);
        self.draft.excluded_apps = parse_exe_list(&self.excluded_text);

        let mut problems: Vec<String> = Vec::new();
        let startup_changed = self.draft.autostart != self.saved.autostart
            || self.draft.run_as_admin != self.saved.run_as_admin;
        if startup_changed {
            if let Err(e) =
                vnekey_win::autostart::set(self.draft.autostart, self.draft.run_as_admin)
            {
                problems.push(e);
                self.draft.autostart = self.saved.autostart; // phản ánh trạng thái thực tế
            }
        }
        match self.draft.save() {
            Ok(()) => self.saved = self.draft.clone(),
            Err(e) => problems.push(format!("Không ghi được file cấu hình: {e}")),
        }
        self.status = Some(if problems.is_empty() {
            Status { ok: true, text: "Đã lưu".to_owned() }
        } else {
            Status { ok: false, text: problems.join(" · ") }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dirty = self.is_dirty();

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("vnekey").strong().color(ACCENT));
                ui.label(RichText::new("Bộ gõ tiếng Việt").weak());
            });
            ui.add_space(6.0);
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| self.footer(ui, dirty));

        egui::CentralPanel::default().show(ctx, |ui| {
            self.controls(ui);
            self.tab_bar(ui);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                match self.tab {
                    Tab::Basic => self.tab_basic(ui),
                    Tab::Abbreviations => self.tab_abbreviations(ui),
                    Tab::Hotkeys => self.tab_hotkeys(ui),
                    Tab::System => self.tab_system(ui),
                }
            });
        });
    }
}

/// Khung nhóm có tiêu đề.
fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong());
        ui.add_space(2.0);
        add_contents(ui);
    });
    ui.add_space(6.0);
}

/// Dòng chú thích nhỏ, mờ.
fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

fn method_name(m: InputMethod) -> &'static str {
    match m {
        InputMethod::Telex => "Telex",
        InputMethod::Vni => "VNI",
    }
}

fn key_label(key: Option<char>) -> String {
    match key {
        None => "Không".to_owned(),
        Some(' ') => "Space".to_owned(),
        Some(c) => c.to_string(),
    }
}

/// Ví dụ "Ctrl + Shift" hoặc "Alt + Z".
fn hotkey_text(hk: &Hotkey) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (on, name) in [(hk.ctrl, "Ctrl"), (hk.shift, "Shift"), (hk.alt, "Alt"), (hk.win, "Win")] {
        if on {
            parts.push(name.to_owned());
        }
    }
    if let Some(key) = hk.key {
        parts.push(key_label(Some(key)));
    }
    parts.join(" + ")
}

/// Cỡ chữ, khoảng cách và màu nhấn.
fn apply_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.style_mut(|style| {
        use egui::TextStyle;
        style.text_styles.insert(TextStyle::Body, FontId::proportional(15.0));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(15.0));
        style.text_styles.insert(TextStyle::Heading, FontId::proportional(24.0));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(12.5));
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 6.0);
        style.spacing.interact_size.y = 26.0;
        style.visuals.selection.bg_fill = ACCENT;
    });
}

/// Font mặc định của egui hiển thị dấu tiếng Việt chưa đẹp; ưu tiên Segoe UI của Windows.
fn install_fonts(ctx: &egui::Context) {
    let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") else {
        return;
    };
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("segoe".to_owned(), FontData::from_owned(bytes));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".to_owned());
    ctx.set_fonts(fonts);
}
