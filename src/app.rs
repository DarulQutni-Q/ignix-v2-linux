//! egui application for Ignix V2 — redesigned per DESIGN_REF.md
//! (Razer Synapse stage cards + G Hub patterns, single cyan accent).

use eframe::egui;
use std::sync::mpsc;
use std::thread;

use crate::device::Mouse;
use crate::protocol::{self, ButtonMap, DpiConfig, LightConfig, LightMode, F5V2_BUTTONS};

// ---------------------------------------------------------------- theme (Swiss poster)

pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x00, 0x00, 0xFF); // saturated blue
pub const PAPER: egui::Color32 = egui::Color32::from_rgb(0xF4, 0xF1, 0xEB); // warm off-white
pub const INK: egui::Color32 = egui::Color32::from_rgb(0x11, 0x11, 0x11);   // near black
pub const WHITE: egui::Color32 = egui::Color32::WHITE;
pub const DIM: egui::Color32 = egui::Color32::from_rgb(0x6E, 0x6A, 0x63);   // warm gray
pub const HAIRLINE: egui::Color32 = egui::Color32::from_rgb(0xD8, 0xD4, 0xC8);
pub const FAINT: egui::Color32 = egui::Color32::from_rgb(0x8A, 0x86, 0x7C);

// Mouse photo hotspot fractions (x, y, w, h) over the image rect.
// Tuned against mouse-white-hd.png (1250x2337, transparent bg).
// Order matches mockup numbering: 01 left, 02 right, 03 wheel, 04 back, 05 forward, 06 dpi.
const HOTSPOTS: [(usize, (f32, f32, f32, f32)); 6] = [
    (0, (0.10, 0.03, 0.36, 0.36)),    // 01 left click
    (1, (0.54, 0.03, 0.36, 0.36)),    // 02 right click
    (2, (0.44, 0.075, 0.12, 0.115)),  // 03 scroll wheel
    (3, (0.055, 0.235, 0.075, 0.075)),// 04 side back (upper)
    (4, (0.055, 0.325, 0.075, 0.095)),// 05 side forward (lower)
    (8, (0.445, 0.20, 0.11, 0.10)),   // 06 DPI button (pill below wheel)
];
// Callout label side per button: true = label on left, false = on right.
const CALLOUT_SIDE: [(usize, bool); 6] = [
    (0, true), (1, false), (2, false), (3, true), (4, true), (8, false),
];
// Display names for callouts (short, uppercase).
const CALLOUT_NAMES: [(usize, &str); 6] = [
    (0, "LEFT CLICK"), (1, "RIGHT CLICK"), (2, "WHEEL CLICK"),
    (3, "BACK"), (4, "FORWARD"), (8, "DPI CYCLE"),
];

// ---------------------------------------------------------------- tabs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab { Dpi, Buttons, Lighting, Performance, Macros }

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Dpi => "DPI",
            Tab::Buttons => "Buttons",
            Tab::Lighting => "Lighting",
            Tab::Performance => "Performance",
            Tab::Macros => "Macros",
        }
    }
    fn all() -> &'static [Tab] {
        &[Tab::Dpi, Tab::Buttons, Tab::Lighting, Tab::Performance, Tab::Macros]
    }
}

// ---------------------------------------------------------------- jobs

enum JobResult {
    Connected {
        dpi: DpiConfig, dpi_raw: Vec<u8>, rate: u32,
        light: LightConfig, buttons: ButtonMap,
        firmware: String, profile_id: u8,
    },
    Applied(String),
    Error(String),
}

// ---------------------------------------------------------------- app

pub struct IgnixApp {
    connected: bool,
    connecting: bool,
    applying: bool,
    status_msg: String,
    status_ok: bool,
    firmware: String,
    profile_id: u8,

    dpi: DpiConfig,
    dpi_raw: Vec<u8>,
    rate: u32,
    light: LightConfig,
    buttons: ButtonMap,
    macros: Vec<protocol::Macro>,
    dirty: bool,

    tab: Tab,
    selected_stage: usize,
    selected_button: Option<usize>,
    selected_macro: usize,
    show_button_picker: bool,
    // macro editor temp state
    macro_new_delay: String,
    macro_new_key: String,

    mouse_tex: Option<egui::TextureHandle>,
    rx: Option<mpsc::Receiver<JobResult>>,
}

impl IgnixApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::style(cc);
        let mouse_tex = Self::load_mouse_texture(cc);
        let mut app = Self {
            connected: false, connecting: true, applying: false,
            status_msg: "Connecting to mouse...".into(), status_ok: true,
            firmware: "?".into(), profile_id: 1,
            dpi: DpiConfig::default(), dpi_raw: Vec::new(),
            rate: 1000, light: LightConfig::default(),
            buttons: ButtonMap::default(), dirty: false,
            macros: (0..32).map(|i| protocol::Macro {
                name: format!("Macro {}", i + 1), ..Default::default()
            }).collect(),
            tab: Tab::Dpi, selected_stage: 0,
            selected_button: None, selected_macro: 0, show_button_picker: false,
            macro_new_delay: "100".into(), macro_new_key: String::new(),
            mouse_tex, rx: None,
        };
        // (debug hook removed)
        app.spawn_connect();
        app
    }

    fn style(cc: &eframe::CreationContext<'_>) {
        let mut s = (*cc.egui_ctx.style()).clone();
        // Swiss poster: radius 0 everywhere, no shadows.
        let zero = egui::Rounding::ZERO;
        s.visuals.dark_mode = false;
        s.visuals.panel_fill = PAPER;
        s.visuals.window_fill = PAPER;
        s.visuals.extreme_bg_color = PAPER;
        s.visuals.widgets.noninteractive.rounding = zero;
        s.visuals.widgets.inactive.rounding = zero;
        s.visuals.widgets.hovered.rounding = zero;
        s.visuals.widgets.active.rounding = zero;
        s.visuals.widgets.open.rounding = zero;
        s.visuals.window_rounding = zero;
        s.visuals.menu_rounding = zero;
        s.visuals.window_shadow = egui::epaint::Shadow::NONE;
        s.visuals.popup_shadow = egui::epaint::Shadow::NONE;
        s.visuals.widgets.noninteractive.bg_fill = PAPER;
        s.visuals.widgets.inactive.bg_fill = PAPER;
        s.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0xE8, 0xE4, 0xD8);
        s.visuals.widgets.active.bg_fill = INK;
        s.visuals.selection.bg_fill = ACCENT;
        s.visuals.selection.stroke = egui::Stroke::new(1.0, WHITE);
        s.visuals.hyperlink_color = ACCENT;
        // Hairline separators
        s.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, HAIRLINE);
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.button_padding = egui::vec2(14.0, 8.0);

        // Custom fonts: Archivo Black (display) + Space Mono (mono).
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert("display".into(),
            egui::FontData::from_owned(include_bytes!("../assets/fonts/ArchivoBlack-Regular.ttf").to_vec()));
        fonts.font_data.insert("mono".into(),
            egui::FontData::from_owned(include_bytes!("../assets/fonts/SpaceMono-Regular.ttf").to_vec()));
        fonts.font_data.insert("mono_bold".into(),
            egui::FontData::from_owned(include_bytes!("../assets/fonts/SpaceMono-Bold.ttf").to_vec()));
        fonts.families.insert(egui::FontFamily::Name("display".into()), vec!["display".into()]);
        fonts.families.insert(egui::FontFamily::Name("mono".into()), vec!["mono".into()]);
        fonts.families.insert(egui::FontFamily::Name("mono_bold".into()), vec!["mono_bold".into()]);
        // Make monospace the default for that typewriter feel on labels.
        fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "mono".into());
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.set_style(s);
    }

    fn display_font(size: f32) -> egui::FontId {
        egui::FontId::new(size, egui::FontFamily::Name("display".into()))
    }
    fn mono_font(size: f32) -> egui::FontId {
        egui::FontId::new(size, egui::FontFamily::Name("mono".into()))
    }
    fn mono_bold_font(size: f32) -> egui::FontId {
        egui::FontId::new(size, egui::FontFamily::Name("mono_bold".into()))
    }

    fn load_mouse_texture(cc: &eframe::CreationContext<'_>) -> Option<egui::TextureHandle> {
        let bytes = include_bytes!("../assets/mouse-white-hd.png");
        let img = image::load_from_memory(bytes).ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
        let ci = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_flat_samples().as_slice());
        Some(cc.egui_ctx.load_texture("mouse", ci, egui::TextureOptions::LINEAR))
    }

    // ---------------------------------------------------------- jobs

    fn spawn_connect(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.connecting = true;
        thread::spawn(move || {
            let res = (|| -> Result<JobResult, String> {
                let mut m = Mouse::open()?;
                let profile_id = m.read_profile_id().unwrap_or(1);
                m.set_profile(profile_id);
                let dpi_raw = m.read_dpi_raw()?;
                let dpi = protocol::parse_dpi(&dpi_raw).map_err(|e| e.to_string())?;
                let rate = m.read_rate()?;
                let light = m.read_light()?;
                let buttons = m.read_buttons().unwrap_or_default();
                let firmware = m.read_firmware().unwrap_or_else(|_| "?".into());
                Ok(JobResult::Connected { dpi, dpi_raw, rate, light, buttons, firmware, profile_id })
            })();
            let _ = tx.send(res.unwrap_or_else(JobResult::Error));
        });
    }

    fn spawn_apply(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.applying = true;
        let (dpi, dpi_raw, rate, light, buttons, profile_id) =
            (self.dpi.clone(), self.dpi_raw.clone(), self.rate, self.light.clone(), self.buttons.clone(), self.profile_id);
        thread::spawn(move || {
            let res: Result<String, String> = (|| {
                let mut m = Mouse::open()?;
                m.set_profile(profile_id);
                let base = if dpi_raw.is_empty() { None } else { Some(dpi_raw.as_slice()) };
                m.write_dpi(&dpi, base)?;
                m.write_rate(rate)?;
                m.write_light(&light)?;
                m.write_buttons(&buttons)?;
                Ok("Applied & verified.".to_string())
            })();
            let _ = tx.send(match res {
                Ok(x) => JobResult::Applied(x),
                Err(e) => JobResult::Error(e),
            });
        });
    }

    fn poll_jobs(&mut self) {
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    JobResult::Connected { dpi, dpi_raw, rate, light, buttons, firmware, profile_id } => {
                        self.connected = true; self.connecting = false;
                        self.dpi = dpi; self.dpi_raw = dpi_raw;
                        self.rate = rate; self.light = light;
                        self.buttons = buttons; self.firmware = firmware;
                        self.profile_id = profile_id;
                        self.selected_stage = self.dpi.active.min(self.dpi.dpis.len().saturating_sub(1));
                        self.dirty = false;
                        self.status_msg = "Connected.".into(); self.status_ok = true;
                    }
                    JobResult::Applied(m) => {
                        self.applying = false; self.dirty = false;
                        self.status_msg = m; self.status_ok = true;
                    }
                    JobResult::Error(e) => {
                        self.connecting = false; self.applying = false;
                        self.status_msg = e; self.status_ok = false;
                    }
                }
            }
        }
    }

    fn mark_dirty(&mut self) { self.dirty = true; }
}

// ---------------------------------------------------------------- eframe

impl eframe::App for IgnixApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs();

        // ---- left: blue panel, title, mouse photo, footer label
        egui::SidePanel::left("mouse")
            .resizable(false)
            .exact_width(340.0)
            .frame(egui::Frame::none().fill(ACCENT))
            .show(ctx, |ui| {
                // Override text colors for blue bg via local visuals
                let mut v = ui.style().visuals.clone();
                v.override_text_color = Some(WHITE);
                ui.style_mut().visuals = v;

                ui.add_space(28.0);
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(egui::RichText::new("IGNIX V2")
                        .font(Self::display_font(34.0)).color(WHITE));
                });
                ui.add_space(8.0);
                // Mouse photo with callouts fills the middle
                let avail_h = ui.available_height() - 80.0;
                let (r, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), avail_h.max(200.0)), egui::Sense::hover());
                let mut img_ui = ui.child_ui(r, *ui.layout(), None);
                // blue bg for the child too
                let mut v2 = img_ui.style().visuals.clone();
                v2.override_text_color = Some(WHITE);
                img_ui.style_mut().visuals = v2;
                self.ui_mouse_photo(&mut img_ui);

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        ui.add_space(28.0);
                        ui.label(egui::RichText::new("LINUX CONFIGURATOR")
                            .font(Self::mono_font(11.0)).color(WHITE));
                    });
                });
            });

        // ---- footer (must be before CentralPanel)
        egui::TopBottomPanel::bottom("bottom")
            .frame(egui::Frame::none().fill(PAPER)
                .inner_margin(egui::Margin { left: 28.0, right: 28.0, top: 12.0, bottom: 16.0 }))
            .show(ctx, |ui| {
                let r = ui.available_rect_before_wrap();
                ui.painter().hline(r.x_range(), r.top() - 12.0, egui::Stroke::new(1.0, INK));
                ui.horizontal(|ui| {
                    if self.dirty {
                        ui.label(egui::RichText::new("1 UNSAVED CHANGE")
                            .font(Self::mono_font(11.0)).color(ACCENT));
                    } else {
                        ui.label(egui::RichText::new("ALL CHANGES SAVED")
                            .font(Self::mono_font(11.0)).color(DIM));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let apply_txt = if self.applying { "APPLYING..." } else { "APPLY TO MOUSE →" };
                        let btn = egui::Button::new(
                            egui::RichText::new(apply_txt).font(Self::mono_bold_font(12.0)).color(WHITE)
                        ).fill(INK).min_size(egui::vec2(180.0, 40.0));
                        let resp = ui.add_enabled(
                            self.connected && self.dirty && !self.applying && !self.connecting, btn);
                        if resp.clicked() {
                            self.status_msg = "Writing to device...".into();
                            self.status_ok = true;
                            self.spawn_apply();
                        }
                        if self.dirty {
                            let rt = egui::RichText::new("Reset").font(Self::mono_font(12.0))
                                .color(INK).underline();
                            if ui.button(rt).clicked() {
                                self.spawn_connect(); // re-read from device = reset
                            }
                        }
                        if self.applying { ui.spinner(); }
                    });
                });
                if !self.status_ok {
                    ui.label(egui::RichText::new(self.status_msg.to_uppercase())
                        .font(Self::mono_font(10.0)).color(DIM));
                }
            });

        // ---- right: paper panel
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(PAPER).inner_margin(egui::Margin::same(28.0)))
            .show(ctx, |ui| {
                // Status line (mono, uppercase)
                let status = if self.connected {
                    format!("USB WIRED  /  FW {}  /  BATTERY --", self.firmware)
                } else if self.connecting {
                    "CONNECTING...".to_string()
                } else {
                    "NO DEVICE".to_string()
                };
                ui.label(egui::RichText::new(status)
                    .font(Self::mono_font(11.0)).color(DIM));
                ui.add_space(14.0);

                // Tabs: flat, active = black fill
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                    for &t in Tab::all() {
                        let sel = self.tab == t;
                        let txt = egui::RichText::new(t.label().to_uppercase())
                            .font(Self::mono_font(12.0))
                            .color(if sel { WHITE } else { INK });
                        let btn = egui::Button::new(txt)
                            .fill(if sel { INK } else { PAPER })
                            .stroke(egui::Stroke::NONE)
                            .min_size(egui::vec2(0.0, 34.0));
                        if ui.add(btn).clicked() { self.tab = t; }
                    }
                });
                // Hairline under tabs
                let r = ui.available_rect_before_wrap();
                ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0, INK));
                ui.add_space(20.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if !self.connected && !self.connecting {
                        self.ui_empty(ui);
                        return;
                    }
                    match self.tab {
                        Tab::Dpi => self.ui_dpi(ui),
                        Tab::Buttons => self.ui_buttons(ui),
                        Tab::Lighting => self.ui_lighting(ui),
                        Tab::Performance => self.ui_performance(ui),
                        Tab::Macros => self.ui_macros(ui),
                    }
                });
            });

        if self.show_button_picker { self.ui_button_picker(ctx); }
        ctx.request_repaint_after(std::time::Duration::from_millis(400));
    }
}

impl IgnixApp {
    fn ui_empty(&mut self, ui: &mut egui::Ui) {
        ui.add_space(40.0);
        ui.label(egui::RichText::new("NO DEVICE FOUND")
            .font(Self::display_font(28.0)).color(INK));
        ui.add_space(12.0);
        ui.label(egui::RichText::new("TROUBLESHOOTING")
            .font(Self::mono_bold_font(11.0)).color(ACCENT));
        ui.add_space(6.0);
        for tip in [
            "01  PLUG IN VIA USB CABLE (WIRED MODE)",
            "02  INSTALL UDEV RULE — SEE README",
            "03  OR RUN WITH SUDO (NOT RECOMMENDED)",
        ] {
            ui.label(egui::RichText::new(tip).font(Self::mono_font(11.0)).color(DIM));
        }
        ui.add_space(20.0);
        let btn = egui::Button::new(
            egui::RichText::new("RETRY CONNECTION").font(Self::mono_bold_font(12.0)).color(WHITE)
        ).fill(INK).min_size(egui::vec2(200.0, 40.0));
        if ui.add(btn).clicked() { self.spawn_connect(); }
    }

    /// Thin horizontal rule.
    fn hairline(ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(r.x_range(), r.center().y, egui::Stroke::new(1.0, HAIRLINE));
        ui.add_space(8.0);
    }

    /// Section heading: mono uppercase.
    fn section(ui: &mut egui::Ui, title: &str) {
        ui.label(egui::RichText::new(title).font(Self::mono_bold_font(11.0)).color(DIM));
        ui.add_space(6.0);
    }

    /// Secondary button: paper fill, 1px ink outline, ink text. Visible on PAPER.
    fn outline_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
        let txt = egui::RichText::new(label).font(Self::mono_font(11.0)).color(INK);
        ui.add(egui::Button::new(txt)
            .fill(PAPER)
            .stroke(egui::Stroke::new(1.0, INK)))
    }

    /// Text field with mono font on PAPER.
    fn bordered_text_edit(ui: &mut egui::Ui, text: &mut String) -> egui::Response {
        ui.add(egui::TextEdit::singleline(text)
            .font(Self::mono_font(11.0))
            .text_color(INK))
    }

    /// Stepper: [-] VALUE [+] — Swiss flat style, fully visible on PAPER.
    /// Returns the new value if changed.
    fn stepper(ui: &mut egui::Ui, value: i32, min: i32, max: i32, step: i32) -> Option<i32> {
        let mut v = value;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
            if Self::outline_button(ui, "−").clicked() {
                v = (v - step).max(min);
                changed = true;
            }
            // Value in black box, white mono text
            let (r, _) = ui.allocate_exact_size(egui::vec2(52.0, 30.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 0.0, INK);
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, format!("{v}"),
                Self::mono_bold_font(13.0), WHITE);
            if Self::outline_button(ui, "+").clicked() {
                v = (v + step).min(max);
                changed = true;
            }
        });
        if changed { Some(v) } else { None }
    }
}

// ---------------------------------------------------------------- mouse photo

impl IgnixApp {
    fn ui_mouse_photo(&mut self, ui: &mut egui::Ui) {
        let tex = match &self.mouse_tex {
            Some(t) => t.clone(),
            None => { ui.label("no image"); return; }
        };
        // Fit image, leave margins for callout labels.
        let (iw, ih) = (tex.size()[0] as f32, tex.size()[1] as f32);
        let avail = ui.available_size();
        let margin = 78.0; // room for callouts on each side
        let avail_w = (avail.x - margin * 2.0).max(120.0);
        let scale = (avail_w / iw).min(avail.y / ih).min(1.0);
        let w = iw * scale;
        let h = ih * scale;
        let x_off = (avail.x - w) / 2.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(avail.x, h), egui::Sense::hover());
        let img_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(x_off, 0.0),
            egui::vec2(w, h),
        );
        ui.painter().image(tex.id(), img_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE);

        let mut clicked: Option<usize> = None;
        let p = ui.painter().clone();
        for (n, (idx, (fx, fy, fw, fh))) in HOTSPOTS.iter().enumerate() {
            let idx = *idx;
            let r = egui::Rect::from_min_size(
                img_rect.min + egui::vec2(fx * img_rect.width(), fy * img_rect.height()),
                egui::vec2(fw * img_rect.width(), fh * img_rect.height()),
            );
            let btn_name = F5V2_BUTTONS.iter().find(|(i,_)| *i==idx).map(|(_,n)|*n).unwrap_or("?");
            let func_name = protocol::ButtonFunc::name(self.buttons.slots[idx].0);
            let resp = ui.allocate_rect(r, egui::Sense::click())
                .on_hover_text(format!("{btn_name} — {func_name}"));
            let hovered = resp.hovered();
            let sel = self.selected_button == Some(idx);

            // Flat highlight: 2px white outline, no glow, no rounding.
            if hovered || sel {
                p.rect_stroke(r, 0.0, egui::Stroke::new(2.0, WHITE));
            }

            // Callout: number + name with hairline leader.
            // Stagger right-side labels vertically to avoid overlap.
            let on_left = CALLOUT_SIDE.iter().find(|(i,_)| *i==idx).map(|(_,s)| *s).unwrap_or(true);
            let cy = r.center().y;
            // Manual y-offset per button for clean staggering (in pixels).
            let y_off: f32 = match idx {
                0 => -30.0,  // left click: a bit above center
                1 => -50.0,  // right click: upper
                2 => -95.0,  // wheel: topmost on right
                3 => 0.0,    // back
                4 => 0.0,    // forward
                8 => 45.0,   // dpi: below wheel label
                _ => 0.0,
            };
            let ly = cy + y_off;
            let num_txt = format!("{:02}", n + 1);
            let name = CALLOUT_NAMES.iter().find(|(i,_)| *i==idx).map(|(_,s)| *s).unwrap_or("?");
            let label_x = if on_left { rect.min.x + 6.0 } else { rect.max.x - 72.0 };
            let anchor_x = if on_left { r.min.x } else { r.max.x };
            // Leader hairline: horizontal from label to hotspot, then vertical jog if offset.
            let line_start_x = if on_left { label_x + 64.0 } else { label_x };
            if y_off.abs() > 1.0 {
                // Elbow: horizontal from label, vertical to hotspot y, horizontal to hotspot.
                let mid_x = if on_left { line_start_x + 14.0 } else { line_start_x - 14.0 };
                p.line_segment([egui::pos2(line_start_x, ly), egui::pos2(mid_x, ly)],
                    egui::Stroke::new(1.0, WHITE));
                p.line_segment([egui::pos2(mid_x, ly), egui::pos2(mid_x, cy)],
                    egui::Stroke::new(1.0, WHITE));
                p.line_segment([egui::pos2(mid_x, cy), egui::pos2(anchor_x, cy)],
                    egui::Stroke::new(1.0, WHITE));
            } else {
                p.line_segment([egui::pos2(line_start_x, cy), egui::pos2(anchor_x, cy)],
                    egui::Stroke::new(1.0, WHITE));
            }
            // Number (mono bold) and name below, left-aligned in margin.
            p.text(egui::pos2(label_x, ly - 20.0), egui::Align2::LEFT_TOP,
                num_txt, Self::mono_bold_font(11.0), WHITE);
            p.text(egui::pos2(label_x, ly - 6.0), egui::Align2::LEFT_TOP,
                name, Self::mono_font(9.0), WHITE);

            if resp.clicked() { clicked = Some(idx); }
        }
        if let Some(i) = clicked {
            self.selected_button = Some(i);
            self.show_button_picker = true;
            self.tab = Tab::Buttons;
        }
    }
}

// ---------------------------------------------------------------- DPI tab (Razer stage cards)

impl IgnixApp {
    fn ui_dpi(&mut self, ui: &mut egui::Ui) {
        if self.dpi.dpis.is_empty() { return; }
        let i = self.selected_stage.min(self.dpi.dpis.len() - 1);
        let v = self.dpi.dpis[i];

        // Giant number: scale font to fit available width.
        let avail_w = ui.available_width();
        let num_size = (avail_w / 5.5).clamp(48.0, 110.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{v}"))
                .font(Self::display_font(num_size)).color(INK));
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.add_space(num_size * 0.45);
                ui.label(egui::RichText::new("DPI").font(Self::mono_bold_font(13.0)).color(INK));
                ui.label(egui::RichText::new(format!("STAGE {} / {}", i + 1, self.dpi.dpis.len()))
                    .font(Self::mono_font(12.0)).color(ACCENT));
            });
        });
        ui.add_space(16.0);

        // Ruler-style stage selector
        let n = self.dpi.dpis.len();
        let (ruler_rect, ruler_resp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().min(640.0), 64.0), egui::Sense::click());
        {
            let p = ui.painter().clone();
            let y0 = ruler_rect.top();
            let y1 = ruler_rect.top() + 30.0;
            // Baseline hairline
            p.hline(ruler_rect.x_range(), y1, egui::Stroke::new(1.0, INK));
            for s in 0..n {
                let x = ruler_rect.min.x + (s as f32) * (ruler_rect.width() / (n.max(1) as f32 - 1.0).max(1.0));
                let is_active = s == self.dpi.active;
                let is_sel = s == self.selected_stage;
                // Tick
                let tick_top = if is_active { y0 } else { y0 + 12.0 };
                p.vline(x, tick_top..=y1, egui::Stroke::new(if is_active { 3.0 } else { 1.0 },
                    if is_active { ACCENT } else { INK }));
                // Stage number above
                p.text(egui::pos2(x, y0 - 2.0), egui::Align2::CENTER_BOTTOM,
                    format!("{}", s + 1), Self::mono_font(10.0),
                    if is_active { ACCENT } else { DIM });
                // DPI value below
                p.text(egui::pos2(x, y1 + 4.0), egui::Align2::CENTER_TOP,
                    format!("{}", self.dpi.dpis[s]), Self::mono_font(10.0),
                    if is_active { ACCENT } else if is_sel { INK } else { DIM });
            }
        }
        // Click/dblclick: map x position to nearest stage (single response, no extra allocates)
        if ruler_resp.clicked() || ruler_resp.double_clicked() {
            if let Some(pos) = ruler_resp.interact_pointer_pos() {
                // Snap to nearest tick
                let mut best = 0;
                let mut best_d = f32::MAX;
                for i in 0..n {
                    let x = ruler_rect.min.x + (i as f32) * (ruler_rect.width() / (n.max(1) as f32 - 1.0).max(1.0));
                    let d = (pos.x - x).abs();
                    if d < best_d { best_d = d; best = i; }
                }
                self.selected_stage = best;
                if ruler_resp.double_clicked() {
                    self.dpi.active = best;
                    self.mark_dirty();
                }
            }
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new("CLICK SELECTS — DOUBLE-CLICK SETS ACTIVE")
            .font(Self::mono_font(10.0)).color(FAINT));

        Self::hairline(ui);

        // Stage editor: flat rows
        Self::section(ui, &format!("STAGE {}", i + 1));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("DPI").font(Self::mono_font(12.0)).color(DIM));
            let mut dv = v as i32;
            if ui.add_sized(egui::vec2(90.0, 24.0),
                egui::DragValue::new(&mut dv).range(50..=30000)
                .custom_formatter(|n, _| format!("{n:.0}"))
            ).changed() {
                self.dpi.dpis[i] = ((dv + 25) / 50 * 50).clamp(50, 30000) as u16;
                self.mark_dirty();
            }
            let mut sv = v as i32;
            ui.add(egui::Slider::new(&mut sv, 50..=30000).show_value(false));
            if sv != v as i32 {
                self.dpi.dpis[i] = ((sv + 25) / 50 * 50).clamp(50, 30000) as u16;
                self.mark_dirty();
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("COLOR").font(Self::mono_font(12.0)).color(DIM));
            let mut c = self.dpi.colors[i];
            // Flat square swatch button
            let (sr, _) = ui.allocate_exact_size(egui::vec2(28.0, 20.0), egui::Sense::click());
            ui.painter().rect_filled(sr, 0.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
            ui.painter().rect_stroke(sr, 0.0, egui::Stroke::new(1.0, INK));
            if ui.interact(sr, ui.next_auto_id(), egui::Sense::click()).clicked() {
                // open native color picker via invisible button trick: use color_edit_button
            }
            if ui.color_edit_button_srgb(&mut c).changed() {
                self.dpi.colors[i] = c;
                self.mark_dirty();
            }
            ui.add_space(16.0);
            if self.dpi.active == i {
                ui.label(egui::RichText::new("ACTIVE").font(Self::mono_bold_font(11.0)).color(ACCENT));
            } else if Self::outline_button(ui, "SET ACTIVE").clicked() {
                self.dpi.active = i;
                self.mark_dirty();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.dpi.dpis.len() > 1 {
                    if Self::outline_button(ui, "REMOVE").clicked() {
                        self.dpi.dpis.remove(i);
                        self.dpi.colors.remove(i);
                        self.dpi.active = self.dpi.active.min(self.dpi.dpis.len() - 1);
                        self.selected_stage = self.selected_stage.min(self.dpi.dpis.len() - 1);
                        self.mark_dirty();
                    }
                }
                if self.dpi.dpis.len() < 6 {
                    if Self::outline_button(ui, "+ ADD STAGE").clicked() {
                        let last = *self.dpi.dpis.last().unwrap_or(&800);
                        self.dpi.dpis.push((last + 400).min(30000));
                        self.dpi.colors.push([255, 255, 255]);
                        self.mark_dirty();
                    }
                }
            });
        });

        Self::hairline(ui);
        Self::section(ui, "SENSOR");
        let mut snap = self.dpi.angle_snapping != 0;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("ANGLE SNAPPING").font(Self::mono_font(12.0)).color(INK));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let txt = if snap { "ON" } else { "OFF" };
                let btxt = egui::RichText::new(txt).font(Self::mono_bold_font(11.0))
                    .color(if snap { WHITE } else { INK });
                let btn = egui::Button::new(btxt)
                    .fill(if snap { INK } else { PAPER })
                    .stroke(egui::Stroke::new(1.0, INK))
                    .min_size(egui::vec2(60.0, 26.0));
                if ui.add(btn).clicked() {
                    snap = !snap;
                    self.dpi.angle_snapping = if snap { 1 } else { 0 };
                    self.mark_dirty();
                }
            });
        });
        let mut rip = self.dpi.ripple != 0;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("RIPPLE CONTROL").font(Self::mono_font(12.0)).color(INK));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let txt = if rip { "ON" } else { "OFF" };
                let btxt = egui::RichText::new(txt).font(Self::mono_bold_font(11.0))
                    .color(if rip { WHITE } else { INK });
                let btn = egui::Button::new(btxt)
                    .fill(if rip { INK } else { PAPER })
                    .stroke(egui::Stroke::new(1.0, INK))
                    .min_size(egui::vec2(60.0, 26.0));
                if ui.add(btn).clicked() {
                    rip = !rip;
                    self.dpi.ripple = if rip { 1 } else { 0 };
                    self.mark_dirty();
                }
            });
        });
    }
}

// ---------------------------------------------------------------- Buttons tab

impl IgnixApp {
    fn ui_buttons(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "BUTTON MAPPING");
        ui.label(egui::RichText::new("CLICK A ZONE ON THE MOUSE, OR A ROW BELOW")
            .font(Self::mono_font(10.0)).color(FAINT));
        ui.add_space(8.0);

        for (n, &(idx, name)) in F5V2_BUTTONS.iter().enumerate() {
            let (t, _, c2) = self.buttons.slots[idx];
            let desc = match t {
                protocol::ButtonFunc::KEYBOARD => format!("KEY HID {c2}"),
                protocol::ButtonFunc::MACRO => format!("MACRO {c2}"),
                _ => protocol::ButtonFunc::name(t).to_uppercase(),
            };
            let sel = self.selected_button == Some(idx);
            // Hairline top
            let r = ui.available_rect_before_wrap();
            ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0, HAIRLINE));
            let row_resp = ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{:02}", n + 1))
                    .font(Self::mono_bold_font(12.0)).color(if sel { ACCENT } else { DIM }));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(name.to_uppercase())
                    .font(Self::mono_font(12.0)).color(INK));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("EDIT →").font(Self::mono_font(11.0)).color(DIM));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(desc)
                        .font(Self::mono_bold_font(12.0)).color(if sel { ACCENT } else { INK }));
                });
            }).response;
            let click = ui.interact(row_resp.rect, ui.next_auto_id(), egui::Sense::click());
            if click.clicked() {
                self.selected_button = Some(idx);
                self.show_button_picker = true;
            }
        }
        // Bottom hairline
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0, HAIRLINE));
    }

    fn ui_button_picker(&mut self, ctx: &egui::Context) {
        let idx = match self.selected_button { Some(i) => i, None => return };
        let name = F5V2_BUTTONS.iter().find(|(i,_)| *i==idx).map(|(_,n)|*n).unwrap_or("?");
        let (cur_t, _, cur_c2) = self.buttons.slots[idx];

        egui::Window::new(format!("REMAP — {}", name.to_uppercase()))
            .collapsible(false).resizable(false)
            .frame(egui::Frame::none().fill(PAPER)
                .stroke(egui::Stroke::new(2.0, INK))
                .inner_margin(egui::Margin::same(20.0)))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(340.0);
                let groups: &[(&str, &[u8], &[&str])] = &[
                    ("MOUSE", &[2,3,4,5,6,7,9,10,11,12],
                     &["Left click","Right click","Middle click","Back","Forward","Double click",
                       "Wheel up","Wheel down","Wheel left","Wheel right"]),
                    ("DPI", &[13,14,15,16], &["DPI cycle","DPI +","DPI −","Sniper (hold)"]),
                    ("MEDIA", &[24,22,23,26,27,28,21,29,30],
                     &["Play/pause","Prev track","Next track","Mute","Volume +","Volume −",
                       "Media player","Calculator","Email"]),
                ];
                for (gname, types, hints) in groups {
                    ui.label(egui::RichText::new(*gname)
                        .font(Self::mono_bold_font(11.0)).color(DIM));
                    for (t, hint) in types.iter().zip(hints.iter()) {
                        let lbl = egui::RichText::new(protocol::ButtonFunc::name(*t).to_uppercase())
                            .font(Self::mono_font(12.0));
                        if ui.radio(cur_t == *t, lbl).on_hover_text(*hint).clicked() {
                            self.buttons.slots[idx] = (*t, 0, 0);
                            self.mark_dirty();
                        }
                    }
                    ui.add_space(6.0);
                }
                Self::hairline(ui);
                if ui.radio(cur_t == protocol::ButtonFunc::KEYBOARD,
                    egui::RichText::new("KEYBOARD KEY").font(Self::mono_font(12.0)))
                    .on_hover_text("Send a keyboard key press.").clicked() {
                    self.buttons.slots[idx] = (protocol::ButtonFunc::KEYBOARD, 0, 4);
                    self.mark_dirty();
                }
                if cur_t == protocol::ButtonFunc::KEYBOARD {
                    let mut hid = cur_c2.max(4) as i32;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("HID CODE").font(Self::mono_font(11.0)).color(DIM));
                        if ui.add(egui::DragValue::new(&mut hid).range(4..=231)).changed() {
                            self.buttons.slots[idx].2 = hid as u8;
                            self.mark_dirty();
                        }
                    });
                    ui.label(egui::RichText::new("4=A · 29=Z · 30=1 · 39=0 · 40=ENTER · 41=ESC · 44=SPACE")
                        .font(Self::mono_font(10.0)).color(FAINT));
                }
                if ui.radio(cur_t == protocol::ButtonFunc::MACRO,
                    egui::RichText::new("MACRO").font(Self::mono_font(12.0)))
                    .on_hover_text("Trigger a macro (see Macros tab).").clicked() {
                    self.buttons.slots[idx] = (protocol::ButtonFunc::MACRO, 0, 1);
                    self.mark_dirty();
                }
                if cur_t == protocol::ButtonFunc::MACRO {
                    let mut mid = cur_c2.clamp(1, 32) as i32;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("MACRO SLOT").font(Self::mono_font(11.0)).color(DIM));
                        if ui.add(egui::DragValue::new(&mut mid).range(1..=32)).changed() {
                            self.buttons.slots[idx].2 = mid as u8;
                            self.mark_dirty();
                        }
                    });
                }
                if ui.radio(cur_t == protocol::ButtonFunc::DISABLED,
                    egui::RichText::new("DISABLED").font(Self::mono_font(12.0))).clicked() {
                    self.buttons.slots[idx] = (protocol::ButtonFunc::DISABLED, 0, 0);
                    self.mark_dirty();
                }
                ui.add_space(12.0);
                let btn = egui::Button::new(
                    egui::RichText::new("DONE").font(Self::mono_bold_font(12.0)).color(WHITE)
                ).fill(INK).min_size(egui::vec2(120.0, 36.0));
                if ui.add(btn).clicked() {
                    self.show_button_picker = false;
                }
            });
    }
}

// ---------------------------------------------------------------- Lighting tab

impl IgnixApp {
    fn ui_lighting(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "LIGHTING");
        ui.label(egui::RichText::new("MODE").font(Self::mono_font(12.0)).color(DIM));
        ui.add_space(4.0);
        // Mode selector: wrapped row of flat buttons, selected = black fill
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for (n, m) in LightMode::all() {
                let sel = self.light.mode == *m;
                let txt = egui::RichText::new(n.to_uppercase())
                    .font(Self::mono_font(11.0))
                    .color(if sel { WHITE } else { INK });
                let btn = egui::Button::new(txt)
                    .fill(if sel { INK } else { PAPER })
                    .stroke(egui::Stroke::new(1.0, INK))
                    .min_size(egui::vec2(0.0, 30.0));
                if ui.add(btn).clicked() {
                    self.light.mode = *m;
                    self.mark_dirty();
                }
            }
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("COLOR").font(Self::mono_font(12.0)).color(DIM));
            if self.light.mode.uses_color() {
                let mut c = self.light.color;
                let (sr, _) = ui.allocate_exact_size(egui::vec2(36.0, 22.0), egui::Sense::hover());
                ui.painter().rect_filled(sr, 0.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                ui.painter().rect_stroke(sr, 0.0, egui::Stroke::new(1.0, INK));
                if ui.color_edit_button_srgb(&mut c).changed() {
                    self.light.color = c; self.mark_dirty();
                }
            } else {
                ui.label(egui::RichText::new("N/A FOR THIS MODE").font(Self::mono_font(11.0)).color(FAINT));
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("BRIGHTNESS").font(Self::mono_font(12.0)).color(DIM));
            if self.light.mode.uses_brightness() {
                if let Some(b) = Self::stepper(ui, self.light.brightness as i32, 0, 15, 1) {
                    self.light.brightness = b as u8; self.mark_dirty();
                }
            } else {
                ui.label(egui::RichText::new("N/A").font(Self::mono_font(11.0)).color(FAINT));
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("SPEED").font(Self::mono_font(12.0)).color(DIM));
            if self.light.mode.uses_speed() {
                if let Some(s) = Self::stepper(ui, self.light.speed as i32, 0, 15, 1) {
                    self.light.speed = s as u8; self.mark_dirty();
                }
            } else {
                ui.label(egui::RichText::new("N/A").font(Self::mono_font(11.0)).color(FAINT));
            }
        });

        Self::hairline(ui);
        Self::section(ui, "POWER");
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("DIM AFTER").font(Self::mono_font(12.0)).color(DIM));
            if let Some(s1) = Self::stepper(ui, self.light.sleep1_min as i32, 0, 60, 1) {
                self.light.sleep1_min = s1 as u8; self.mark_dirty();
            }
            ui.label(egui::RichText::new("MIN").font(Self::mono_font(10.0)).color(FAINT));
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("SLEEP AFTER").font(Self::mono_font(12.0)).color(DIM));
            if let Some(s2) = Self::stepper(ui, self.light.sleep2_min as i32, 0, 60, 1) {
                self.light.sleep2_min = s2 as u8; self.mark_dirty();
            }
            ui.label(egui::RichText::new("MIN").font(Self::mono_font(10.0)).color(FAINT));
        });
    }
}

// ---------------------------------------------------------------- Performance tab

impl IgnixApp {
    fn ui_performance(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "POLLING RATE");
        ui.label(egui::RichText::new("HIGHER = LOWER LATENCY, MORE CPU")
            .font(Self::mono_font(10.0)).color(FAINT));
        ui.add_space(8.0);

        // Segmented control: flat boxes, selected = black fill
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            for hz in [125u32, 250, 500, 1000] {
                let sel = self.rate == hz;
                let txt = egui::RichText::new(format!("{hz}"))
                    .font(Self::mono_bold_font(13.0))
                    .color(if sel { WHITE } else { INK });
                let mut btn = egui::Button::new(txt).min_size(egui::vec2(72.0, 40.0));
                btn = btn.fill(if sel { INK } else { PAPER });
                // Hairline border via stroke override: draw manually after
                let resp = ui.add(btn);
                let r = resp.rect;
                ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0, INK));
                if resp.clicked() {
                    self.rate = hz; self.mark_dirty();
                }
            }
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("HZ").font(Self::mono_font(10.0)).color(FAINT));

        Self::hairline(ui);
        Self::section(ui, "CLICK RESPONSE");
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("DEBOUNCE").font(Self::mono_font(12.0)).color(DIM));
            if let Some(db) = Self::stepper(ui, self.light.debounce_ms as i32, 2, 20, 1) {
                self.light.debounce_ms = db as u8; self.mark_dirty();
            }
            ui.label(egui::RichText::new("MS — LOWER = FASTER, HIGHER DBL-CLICK RISK")
                .font(Self::mono_font(10.0)).color(FAINT));
        });
        ui.label(egui::RichText::new("DEFAULT 8 MS").font(Self::mono_font(10.0)).color(FAINT));
    }
}

// ---------------------------------------------------------------- Macros tab

impl IgnixApp {
    fn spawn_upload_macro(&mut self, idx: usize) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.applying = true;
        let m = self.macros[idx].clone();
        thread::spawn(move || {
            let res: Result<String, String> = (|| {
                let dev = Mouse::open()?;
                dev.write_macro(&m, idx as u8)?;
                Ok(format!("Macro {} uploaded.", idx + 1))
            })();
            let _ = tx.send(match res {
                Ok(x) => JobResult::Applied(x),
                Err(e) => JobResult::Error(e),
            });
        });
    }

    fn ui_macros(&mut self, ui: &mut egui::Ui) {
        Self::section(ui, "MACROS");
        ui.label(egui::RichText::new("32 SLOTS — ASSIGN VIA BUTTONS TAB")
            .font(Self::mono_font(10.0)).color(FAINT));
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            // ---- slot list (flat rows)
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(200.0);
                    for i in 0..32 {
                        let m = &self.macros[i];
                        let sel = self.selected_macro == i;
                        let r = ui.available_rect_before_wrap();
                        ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0, HAIRLINE));
                        let label = if m.events.is_empty() {
                            format!("{:02}  {}", i + 1, m.name.to_uppercase())
                        } else {
                            format!("{:02}  {} [{}]", i + 1, m.name.to_uppercase(), m.events.len())
                        };
                        // Swiss style: selected = black fill + white text (no default blue selection)
                        let txt = egui::RichText::new(label).font(Self::mono_font(11.0))
                            .color(if sel { WHITE } else { INK });
                        let btn = egui::Button::new(txt)
                            .fill(if sel { INK } else { PAPER })
                            .stroke(egui::Stroke::NONE)
                            .min_size(egui::vec2(200.0, 26.0));
                        if ui.add(btn).clicked() { self.selected_macro = i; }
                    }
                    let r = ui.available_rect_before_wrap();
                    ui.painter().hline(r.x_range(), r.top(), egui::Stroke::new(1.0, HAIRLINE));
                });
            });
            ui.add_space(20.0);

            // ---- editor
            let idx = self.selected_macro;
            let mut new_delay = self.macro_new_delay.clone();
            let mut new_key = self.macro_new_key.clone();
            let connected = self.connected;
            let applying = self.applying;
            ui.vertical(|ui| {
                ui.set_min_width(400.0);
                let m = &mut self.macros[idx];

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("SLOT {:02}", idx + 1))
                        .font(Self::display_font(22.0)).color(INK));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if Self::outline_button(ui, "AUTO-CLICKER PRESET").clicked() {
                            *m = protocol::autoclicker_preset(100);
                            m.name = format!("Auto Clicker {}", idx + 1);
                        }
                    });
                });
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("NAME").font(Self::mono_font(11.0)).color(DIM));
                    Self::bordered_text_edit(ui, &mut m.name);
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("LOOP").font(Self::mono_font(11.0)).color(DIM));
                    for (ti, tn) in ["ONCE", "WHILE HELD", "TOGGLE"].iter().enumerate() {
                        let sel = m.loop_type as usize == ti;
                        let txt = egui::RichText::new(*tn).font(Self::mono_font(11.0))
                            .color(if sel { WHITE } else { INK });
                        let btn = egui::Button::new(txt)
                            .fill(if sel { INK } else { PAPER })
                            .stroke(egui::Stroke::new(1.0, INK))
                            .min_size(egui::vec2(0.0, 26.0));
                        if ui.add(btn).clicked() {
                            m.loop_type = ti as u8;
                        }
                    }
                });
                ui.add_space(8.0);

                ui.label(egui::RichText::new("EVENTS").font(Self::mono_bold_font(11.0)).color(DIM));
                let mut del: Option<usize> = None;
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    let rows: Vec<String> = m.events.iter().map(|e| {
                        let what = if e.keycode >= 241 && e.keycode <= 245 {
                            protocol::MACRO_MOUSE_CODES.iter()
                                .find(|(c, _)| *c == e.keycode)
                                .map(|(_, n)| n.to_uppercase()).unwrap_or("MOUSE".into())
                        } else {
                            "KEY".to_string()
                        };
                        let dir = if e.is_up { "UP  " } else { "DOWN" };
                        format!("{:>5}MS  {} {} 0x{:02X}", e.delay_ms, what, dir, e.keycode)
                    }).collect();
                    for (ei, row) in rows.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("{:02}", ei + 1))
                                .font(Self::mono_font(10.0)).color(FAINT));
                            ui.label(egui::RichText::new(row).font(egui::FontId::monospace(11.0)).color(INK));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if Self::outline_button(ui, "×").clicked() {
                                    del = Some(ei);
                                }
                            });
                        });
                    }
                    if m.events.is_empty() {
                        ui.label(egui::RichText::new("NO EVENTS YET")
                            .font(Self::mono_font(10.0)).color(FAINT));
                    }
                });
                if let Some(ei) = del { m.events.remove(ei); }

                ui.add_space(8.0);
                ui.label(egui::RichText::new("ADD EVENT").font(Self::mono_bold_font(11.0)).color(DIM));
                ui.horizontal_wrapped(|ui| {
                    for (code, name) in protocol::MACRO_MOUSE_CODES {
                        if Self::outline_button(ui, &format!("+ {}", name.to_uppercase())).clicked() {
                            let d: u16 = new_delay.parse().unwrap_or(100);
                            m.events.push(protocol::MacroEvent { delay_ms: 10, keycode: code, is_up: false });
                            m.events.push(protocol::MacroEvent { delay_ms: d, keycode: code, is_up: true });
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("DELAY MS").font(Self::mono_font(10.0)).color(DIM));
                    Self::bordered_text_edit(ui, &mut new_delay);
                    if Self::outline_button(ui, "+ DELAY").clicked() {
                        let d: u16 = new_delay.parse().unwrap_or(100).clamp(10, 12700);
                        m.events.push(protocol::MacroEvent { delay_ms: d, keycode: 0, is_up: false });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("KEY HEX").font(Self::mono_font(10.0)).color(DIM));
                    Self::bordered_text_edit(ui, &mut new_key);
                    if Self::outline_button(ui, "+ KEY").clicked() {
                        let kc = u8::from_str_radix(new_key.trim_start_matches("0x"), 16).unwrap_or(0);
                        if kc != 0 {
                            let d: u16 = new_delay.parse().unwrap_or(100);
                            m.events.push(protocol::MacroEvent { delay_ms: 10, keycode: kc, is_up: false });
                            m.events.push(protocol::MacroEvent { delay_ms: d, keycode: kc, is_up: true });
                        }
                    }
                });

                ui.add_space(12.0);
                let can_upload = connected && !applying && !m.events.is_empty();
                let event_count = m.events.len();
                let _ = m;
                let btn = egui::Button::new(
                    egui::RichText::new("UPLOAD TO MOUSE").font(Self::mono_bold_font(12.0)).color(WHITE)
                ).fill(INK).min_size(egui::vec2(180.0, 38.0));
                if ui.add_enabled(can_upload, btn).clicked() {
                    self.spawn_upload_macro(idx);
                }
                if event_count > 50 {
                    ui.label(egui::RichText::new("WARNING: >50 EVENTS, WILL BE TRUNCATED")
                        .font(Self::mono_font(10.0)).color(INK));
                }
            });
            self.macro_new_delay = new_delay;
            self.macro_new_key = new_key;
        });
    }
}
