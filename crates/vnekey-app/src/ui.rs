//! Cửa sổ cài đặt (egui) với 4 tab: Cơ bản, Gõ tắt, Phím tắt, Hệ thống.
//!
//! Chạy trong tiến trình riêng (`vnekey --settings`) nên đóng cửa sổ không ảnh hưởng đến
//! bộ gõ. Bấm "Lưu" ghi `config.toml`; tiến trình khay tự nạp lại trong vòng một giây.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, RichText};
use vnekey_config::{Config, format_abbreviations, parse_abbreviations, parse_exe_list};
use vnekey_core::{Charset, InputMethod};
use vnekey_win::process::SETTINGS_TITLE;

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(SETTINGS_TITLE)
            .with_inner_size([560.0, 620.0])
            .with_resizable(false),
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

struct App {
    /// Bản đang chỉnh; chỉ có hiệu lực sau khi bấm "Lưu".
    draft: Config,
    /// Bản đã lưu gần nhất, để biết cần bật/tắt khởi động cùng Windows hay không.
    saved: Config,
    abbreviations_text: String,
    excluded_text: String,
    tab: Tab,
    status: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let config = Config::load();
        Self {
            abbreviations_text: format_abbreviations(&config.abbreviations),
            excluded_text: config.excluded_apps.join("\n"),
            saved: config.clone(),
            draft: config,
            tab: Tab::Basic,
            status: String::new(),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Điều khiển").strong());
            egui::Grid::new("control").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                ui.label("Bảng mã");
                egui::ComboBox::from_id_salt("charset")
                    .selected_text(self.draft.charset.name())
                    .show_ui(ui, |ui| {
                        for charset in Charset::ALL {
                            ui.selectable_value(&mut self.draft.charset, charset, charset.name());
                        }
                    });
                ui.end_row();

                ui.label("Kiểu gõ");
                egui::ComboBox::from_id_salt("method")
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
        });
        ui.add_space(4.0);
        ui.weak(
            "Biểu tượng khay: bấm trái để đổi V/E, bấm đúp để mở cài đặt, bấm phải để mở menu.",
        );
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Basic, "Cơ bản");
            ui.selectable_value(&mut self.tab, Tab::Abbreviations, "Gõ tắt");
            ui.selectable_value(&mut self.tab, Tab::Hotkeys, "Phím tắt");
            ui.selectable_value(&mut self.tab, Tab::System, "Hệ thống");
        });
        ui.separator();
        match self.tab {
            Tab::Basic => self.tab_basic(ui),
            Tab::Abbreviations => self.tab_abbreviations(ui),
            Tab::Hotkeys => self.tab_hotkeys(ui),
            Tab::System => self.tab_system(ui),
        }
    }

    fn tab_basic(&mut self, ui: &mut egui::Ui) {
        let d = &mut self.draft;
        ui.checkbox(&mut d.spell_check, "Bật kiểm tra chính tả (tự khôi phục phím với từ sai)");
        ui.checkbox(&mut d.modern_tone, "Đặt dấu kiểu mới (hoà, thuý thay vì hòa, thúy)");
        ui.checkbox(&mut d.allow_fjwz, "Cho phép 'f j w z' làm phụ âm");
        ui.checkbox(&mut d.auto_capitalize, "Tự động viết hoa chữ đầu câu (sau . ! ? và Enter)");
        ui.checkbox(&mut d.bracket_horn, "Telex: dùng [ ] cho ơ ư");

        ui.add_space(10.0);
        ui.label(RichText::new("Cách gửi phím").strong());
        ui.checkbox(
            &mut d.use_clipboard,
            "Sử dụng Clipboard để gửi phím (cho ứng dụng không nhận được chữ Unicode)",
        );
        ui.checkbox(
            &mut d.select_with_shift,
            "Chọn chữ bằng Shift+← thay vì Backspace (sửa lỗi thanh địa chỉ Chrome/Edge)",
        );
        ui.add_space(6.0);
        ui.weak("Bảng mã cũ (TCVN3, VNI Windows) cần font tương ứng như .VnTime hoặc VNI-Times.");
    }

    fn tab_abbreviations(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.draft.abbreviations_enabled, "Cho phép gõ tắt");
        ui.add_enabled(
            self.draft.abbreviations_enabled,
            egui::Checkbox::new(
                &mut self.draft.abbreviations_in_english,
                "Vẫn cho phép gõ tắt khi đang tắt tiếng Việt (E)",
            ),
        );
        ui.add_space(6.0);
        ui.label("Bảng gõ tắt (mỗi dòng một mục: từ gõ = cụm mở rộng)");
        ui.add(
            egui::TextEdit::multiline(&mut self.abbreviations_text)
                .desired_rows(12)
                .desired_width(f32::INFINITY)
                .hint_text("ko = không\ndc = được\nvn = Việt Nam"),
        );
        ui.add_space(4.0);
        ui.weak(
            "Gõ tắt kích hoạt khi bấm Space, Enter hoặc dấu câu. \"Ko\" mở rộng thành \"Không\", \
             \"KO\" thành \"KHÔNG\". Dòng bắt đầu bằng # là ghi chú.",
        );
    }

    fn tab_hotkeys(&mut self, ui: &mut egui::Ui) {
        let hk = &mut self.draft.hotkey;
        ui.label("Phím chuyển Việt/Anh");
        ui.horizontal(|ui| {
            ui.checkbox(&mut hk.ctrl, "Ctrl");
            ui.checkbox(&mut hk.shift, "Shift");
            ui.checkbox(&mut hk.alt, "Alt");
            ui.checkbox(&mut hk.win, "Win");
            egui::ComboBox::from_id_salt("hotkey_key").selected_text(key_label(hk.key)).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(&mut hk.key, None, "Không");
                    ui.selectable_value(&mut hk.key, Some(' '), "Space");
                    for c in ('A'..='Z').chain('0'..='9') {
                        ui.selectable_value(&mut hk.key, Some(c), c.to_string());
                    }
                },
            );
        });
        ui.add_space(4.0);
        if !hk.has_modifier() {
            ui.colored_label(Color32::LIGHT_RED, "Cần chọn ít nhất một phím Ctrl/Shift/Alt/Win.");
        } else if hk.key.is_none() {
            ui.weak("Không chọn phím chính: nhấn rồi nhả tổ hợp để chuyển (như UniKey).");
        }
        ui.add_space(8.0);
        ui.weak("Các phím tắt F1-F12 như EVKey sẽ bổ sung ở bản sau.");
    }

    fn tab_system(&mut self, ui: &mut egui::Ui) {
        let d = &mut self.draft;
        ui.checkbox(&mut d.start_vietnamese, "Mở app ở chế độ tiếng Việt");
        ui.checkbox(&mut d.autostart, "Khởi động cùng Windows");
        ui.checkbox(&mut d.run_as_admin, "Chạy với quyền admin (hiện hộp thoại UAC khi mở)");
        ui.weak(
            "Quyền admin giúp gõ được trong ứng dụng đang chạy bằng quyền admin. Khi bật cùng \
             \"Khởi động cùng Windows\", vnekey dùng Task Scheduler; cần mở cài đặt từ bản đang \
             chạy bằng quyền admin để tạo tác vụ.",
        );

        ui.add_space(10.0);
        ui.label(RichText::new("Loại trừ ứng dụng").strong());
        ui.weak(
            "Mỗi dòng một tên file .exe. vnekey không can thiệp khi các ứng dụng này đang focus.",
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.excluded_text)
                .desired_rows(6)
                .desired_width(f32::INFINITY)
                .hint_text("game.exe"),
        );
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Thiết lập mặc định").clicked() {
                self.draft = Config::default();
                self.abbreviations_text.clear();
                self.excluded_text.clear();
                self.status.clear();
            }
            if ui.button("Lưu").clicked() {
                self.save();
            }
        });
        if !self.status.is_empty() {
            ui.add_space(4.0);
            ui.label(&self.status);
        }
    }

    fn save(&mut self) {
        if !self.draft.hotkey.has_modifier() {
            self.status = "Phím chuyển cần ít nhất một phím Ctrl/Shift/Alt/Win.".to_owned();
            return;
        }
        self.draft.abbreviations = parse_abbreviations(&self.abbreviations_text);
        self.draft.excluded_apps = parse_exe_list(&self.excluded_text);

        let mut notes: Vec<String> = Vec::new();
        let startup_changed = self.draft.autostart != self.saved.autostart
            || self.draft.run_as_admin != self.saved.run_as_admin;
        if startup_changed {
            if let Err(e) =
                vnekey_win::autostart::set(self.draft.autostart, self.draft.run_as_admin)
            {
                notes.push(e);
                self.draft.autostart = self.saved.autostart; // phản ánh trạng thái thực tế
            }
        }
        match self.draft.save() {
            Ok(()) => {
                self.saved = self.draft.clone();
                notes.insert(0, "Đã lưu".to_owned());
            }
            Err(e) => notes.push(format!("Không ghi được file cấu hình: {e}")),
        }
        self.status = notes.join(" · ");
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.header(ui);
            ui.add_space(10.0);
            self.tabs(ui);
            ui.add_space(14.0);
            ui.separator();
            self.footer(ui);
        });
    }
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