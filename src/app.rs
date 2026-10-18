//! egui application for Ignix V2 — redesigned per DESIGN_REF.md
//! (Razer Synapse stage cards + G Hub patterns, single cyan accent).

use eframe::egui;
use std::sync::mpsc;
use std::thread;

use crate::device::Mouse;
use crate::protocol::{self, ButtonMap, DpiConfig, LightConfig, LightMode, F5V2_BUTTONS};

// ---------------------------------------------------------------- theme (DESIGN_REF §2)

pub const BG: egui::Color32 = egui::Color32::from_rgb(0x0e, 0x0e, 0x13);
pub const PANEL: egui::Color32 = egui::Color32::from_rgb(0x14, 0x14, 0x1c);
pub const CARD: egui::Color32 = egui::Color32::from_rgb(0x1b, 0x1b, 0x26);
pub const CARD_HOVER: egui::Color32 = egui::Color32::from_rgb(0x23, 0x23, 0x2f);
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(0x2a, 0x2a, 0x38);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x00, 0xd4, 0xff);
pub fn accent_dim() -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(0, 212, 255, 36)
}
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(0xec, 0xec, 0xf1);
pub const DIM: egui::Color32 = egui::Color32::from_rgb(0x8b, 0x8b, 0x98);
pub const FAINT: egui::Color32 = egui::Color32::from_rgb(0x55, 0x55, 0x5f);
pub const GOOD: egui::Color32 = egui::Color32::from_rgb(0x3d, 0xdc, 0x84);
pub const WARN: egui::Color32 = egui::Color32::from_rgb(0xff, 0xb0, 0x20);
pub const BAD: egui::Color32 = egui::Color32::from_rgb(0xff, 0x55, 0x66);

// Mouse photo hotspot fractions (x, y, w, h) over the image rect.
// Tuned against ignix-f5-black-official.webp (top-down, 1500x2606).
const HOTSPOTS: [(usize, (f32, f32, f32, f32)); 6] = [
    (0, (0.06, 0.02, 0.40, 0.38)),  // left click (left wing)
    (1, (0.54, 0.02, 0.40, 0.38)),  // right click (right wing)
    (2, (0.43, 0.12, 0.14, 0.16)),  // scroll wheel
    (8, (0.45, 0.31, 0.10, 0.05)),  // DPI button (in groove below wheel)
    (3, (0.015, 0.36, 0.075, 0.09)), // side back (upper)
    (4, (0.015, 0.47, 0.075, 0.11)), // side forward (lower)
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
    dirty: bool,

    tab: Tab,
    selected_stage: usize,
    selected_button: Option<usize>,
    show_button_picker: bool,

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
            tab: Tab::Dpi, selected_stage: 0,
            selected_button: None, show_button_picker: false,
            mouse_tex, rx: None,
        };
        app.spawn_connect();
        app
    }

    fn style(cc: &eframe::CreationContext<'_>) {
        let mut s = (*cc.egui_ctx.style()).clone();
        s.visuals.dark_mode = true;
        s.visuals.panel_fill = PANEL;
        s.visuals.window_fill = PANEL;
        s.visuals.extreme_bg_color = BG;
        s.visuals.widgets.noninteractive.bg_fill = CARD;
        s.visuals.widgets.inactive.bg_fill = CARD;
        s.visuals.widgets.hovered.bg_fill = CARD_HOVER;
        s.visuals.widgets.active.bg_fill = CARD_HOVER;
        s.visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
        s.visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
        s.visuals.widgets.active.rounding = egui::Rounding::same(8.0);
        s.visuals.selection.bg_fill = ACCENT;
        s.visuals.hyperlink_color = ACCENT;
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        cc.egui_ctx.set_style(s);
    }

    fn load_mouse_texture(cc: &eframe::CreationContext<'_>) -> Option<egui::TextureHandle> {
        let bytes = include_bytes!("../assets/mouse-top.png");
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
                let m = Mouse::open()?;
                let dpi_raw = m.read_dpi_raw()?;
                let dpi = protocol::parse_dpi(&dpi_raw).map_err(|e| e.to_string())?;
                let rate = m.read_rate()?;
                let light = m.read_light()?;
                let buttons = m.read_buttons().unwrap_or_default();
                let firmware = m.read_firmware().unwrap_or_else(|_| "?".into());
                let profile_id = m.read_profile_id().unwrap_or(1);
                Ok(JobResult::Connected { dpi, dpi_raw, rate, light, buttons, firmware, profile_id })
            })();
            let _ = tx.send(res.unwrap_or_else(JobResult::Error));
        });
    }

    fn spawn_apply(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.applying = true;
        let (dpi, dpi_raw, rate, light, buttons) =
            (self.dpi.clone(), self.dpi_raw.clone(), self.rate, self.light.clone(), self.buttons.clone());
        thread::spawn(move || {
            let res: Result<String, String> = (|| {
                let m = Mouse::open()?;
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

    fn card_frame(selected: bool) -> egui::Frame {
        let mut f = egui::Frame::none()
            .inner_margin(egui::Margin::same(10.0))
            .rounding(egui::Rounding::same(8.0))
            .fill(CARD)
            .stroke(egui::Stroke::new(1.0f32, BORDER));
        if selected {
            f = f.stroke(egui::Stroke::new(2.0f32, ACCENT)).fill(accent_dim());
        }
        f
    }
}

// ---------------------------------------------------------------- eframe

impl eframe::App for IgnixApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs();

        // ---- top bar
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("IGNIX V2").size(18.0).strong().color(ACCENT));
                ui.label(egui::RichText::new("Linux Configurator").size(12.0).color(DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.connected {
                        ui.label(egui::RichText::new(format!("FW {}", self.firmware)).size(11.0).color(DIM));
                        ui.label(egui::RichText::new(format!("Profile {}", self.profile_id)).size(11.0).color(DIM));
                        ui.colored_label(GOOD, "●");
                    } else if self.connecting {
                        ui.spinner(); ui.label("Connecting…");
                    } else {
                        ui.colored_label(BAD, "● Disconnected");
                        if ui.button("Retry").clicked() { self.spawn_connect(); }
                    }
                });
            });
        });

        // ---- bottom bar
        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&self.status_msg).size(11.0)
                    .color(if self.status_ok { DIM } else { BAD }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.applying { ui.spinner(); }
                    if self.dirty && !self.applying {
                        ui.colored_label(WARN, "● Unsaved");
                    }
                    let btn = egui::Button::new(
                        egui::RichText::new(if self.applying { "Applying…" } else { "Apply to Mouse" }).strong()
                    ).fill(if self.dirty && !self.applying { ACCENT } else { CARD });
                    let resp = ui.add_enabled(
                        self.connected && self.dirty && !self.applying && !self.connecting, btn
                    );
                    if resp.clicked() {
                        self.status_msg = "Writing to device…".into();
                        self.status_ok = true;
                        self.spawn_apply();
                    }
                });
            });
        });

        // ---- left: mouse photo + hotspots
        egui::SidePanel::left("mouse").resizable(false).exact_width(300.0).show(ctx, |ui| {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Mouse").size(15.0).strong());
            });
            ui.add_space(4.0);
            self.ui_mouse_photo(ui);
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            // Active DPI big display
            if let Some(&v) = self.dpi.dpis.get(self.dpi.active) {
                let c = self.dpi.colors.get(self.dpi.active).copied().unwrap_or([255,255,255]);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 6.0,
                        egui::Color32::from_rgb(c[0], c[1], c[2]));
                    ui.label(egui::RichText::new(format!("{v}")).size(28.0).strong());
                });
                ui.label(egui::RichText::new("DPI").size(11.0).color(DIM));
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("{} Hz", self.rate)).size(13.0).color(DIM));
        });

        // ---- center: tabs + content
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                for &t in Tab::all() {
                    let sel = self.tab == t;
                    let lbl = if sel {
                        egui::RichText::new(t.label()).size(14.0).color(TEXT).strong()
                    } else {
                        egui::RichText::new(t.label()).size(14.0).color(DIM)
                    };
                    if ui.selectable_label(sel, lbl).clicked() { self.tab = t; }
                }
            });
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(8.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Not connected? show empty state.
                if !self.connected && !self.connecting {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.label(egui::RichText::new("Mouse not found").size(18.0).strong());
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(
                            "Check the USB cable (wired mode) and the udev rule.\nSee README for setup."
                        ).size(13.0).color(DIM));
                        ui.add_space(12.0);
                        if ui.button("Retry").clicked() { self.spawn_connect(); }
                    });
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

// ---------------------------------------------------------------- mouse photo

impl IgnixApp {
    fn ui_mouse_photo(&mut self, ui: &mut egui::Ui) {
        let tex = match &self.mouse_tex {
            Some(t) => t.clone(),
            None => { ui.label("no image"); return; }
        };
        // Keep aspect, fit width 280.
        let avail_w = 280.0;
        let (iw, ih) = (tex.size()[0] as f32, tex.size()[1] as f32);
        let h = avail_w * ih / iw;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(avail_w, h.min(420.0)), egui::Sense::hover());
        // Draw cropped to fit (top portion contains the mouse).
        let uv = egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, (rect.height() / avail_w) * (iw / ih)),
        );
        ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);

        let mut clicked: Option<usize> = None;
        for (idx, (fx, fy, fw, fh)) in HOTSPOTS {
            let r = egui::Rect::from_min_size(
                rect.min + egui::vec2(fx * rect.width(), fy * rect.height()),
                egui::vec2(fw * rect.width(), fh * rect.height()),
            );
            let resp = ui.allocate_rect(r, egui::Sense::click())
                .on_hover_text(format!("{} — {}",
                    F5V2_BUTTONS.iter().find(|(i,_)| *i==idx).map(|(_,n)|*n).unwrap_or("?"),
                    protocol::ButtonFunc::name(self.buttons.slots[idx].0)));
            let hovered = resp.hovered();
            let sel = self.selected_button == Some(idx);
            if hovered || sel {
                // Glow effect: soft fill + bright ring
                let glow = if sel {
                    egui::Color32::from_rgba_unmultiplied(0, 212, 255, 45)
                } else {
                    egui::Color32::from_rgba_unmultiplied(0, 212, 255, 22)
                };
                ui.painter().rect_filled(r, 12.0, glow);
                ui.painter().rect_stroke(r, 12.0,
                    egui::Stroke::new(if sel { 2.5f32 } else { 1.5f32 },
                        if sel { ACCENT } else { egui::Color32::from_rgba_unmultiplied(0, 212, 255, 150) }));
                // Button number badge
                if let Some((pos, _)) = F5V2_BUTTONS.iter().enumerate().find(|(_, (i, _))| *i == idx) {
                    let badge_pos = r.left_top() + egui::vec2(6.0, 6.0);
                    ui.painter().circle_filled(badge_pos, 10.0, ACCENT);
                    ui.painter().text(badge_pos, egui::Align2::CENTER_CENTER,
                        format!("{}", pos + 1), egui::FontId::proportional(11.0),
                        egui::Color32::from_rgb(0x0e, 0x0e, 0x13));
                }
            }
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
        ui.label(egui::RichText::new("DPI Stages").size(15.0).strong());
        ui.label(egui::RichText::new(
            format!("{} levels · 50 – 30000 · step 50", self.dpi.dpis.len())
        ).size(11.0).color(DIM));
        ui.add_space(8.0);

        // ---- stage cards row
        ui.horizontal_wrapped(|ui| {
            for i in 0..self.dpi.dpis.len() {
                let active = self.dpi.active == i;
                let sel = self.selected_stage == i;
                let v = self.dpi.dpis[i];
                let c = self.dpi.colors[i];
                let frame = Self::card_frame(active);
                let resp = frame.show(ui, |ui| {
                    ui.set_min_size(egui::vec2(86.0, 74.0));
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new(format!("{}", i + 1)).size(11.0).color(DIM));
                        ui.label(egui::RichText::new(format!("{v}")).size(17.0).strong());
                        // color dot
                        let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 7.0,
                            egui::Color32::from_rgb(c[0], c[1], c[2]));
                    });
                }).response;
                let resp = ui.interact(resp.rect, resp.id, egui::Sense::click());
                if resp.clicked() {
                    self.selected_stage = i;
                }
                if resp.double_clicked() {
                    self.dpi.active = i;
                    self.mark_dirty();
                }
                let _ = sel;
            }
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Click to select · double-click to set active").size(11.0).color(FAINT));

        ui.add_space(12.0);
        // ---- selected stage editor
        if self.dpi.dpis.is_empty() { return; }
        let i = self.selected_stage.min(self.dpi.dpis.len() - 1);
        Self::card_frame(false).show(ui, |ui| {
            ui.label(egui::RichText::new(format!("Stage {}", i + 1)).size(13.0).strong());
            ui.add_space(4.0);
            let mut v = self.dpi.dpis[i] as i32;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("DPI").size(12.0).color(DIM));
                if ui.add(egui::Slider::new(&mut v, 50..=30000).show_value(false)).changed() {
                    self.dpi.dpis[i] = ((v + 25) / 50 * 50).clamp(50, 30000) as u16;
                    self.mark_dirty();
                }
                let mut nv = self.dpi.dpis[i] as i32;
                if ui.add(egui::DragValue::new(&mut nv).range(50..=30000)).changed() {
                    self.dpi.dpis[i] = ((nv + 25) / 50 * 50).clamp(50, 30000) as u16;
                    self.mark_dirty();
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Color").size(12.0).color(DIM));
                let mut c = self.dpi.colors[i];
                if ui.color_edit_button_srgb(&mut c).changed() {
                    self.dpi.colors[i] = c;
                    self.mark_dirty();
                }
                ui.add_space(12.0);
                if self.dpi.active != i {
                    if ui.small_button("Set as active").clicked() {
                        self.dpi.active = i;
                        self.mark_dirty();
                    }
                } else {
                    ui.label(egui::RichText::new("● Active").size(12.0).color(ACCENT));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.dpi.dpis.len() > 1 {
                        if ui.small_button("Remove").clicked() {
                            self.dpi.dpis.remove(i);
                            self.dpi.colors.remove(i);
                            self.dpi.active = self.dpi.active.min(self.dpi.dpis.len() - 1);
                            self.selected_stage = self.selected_stage.min(self.dpi.dpis.len() - 1);
                            self.mark_dirty();
                        }
                    }
                });
            });
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if self.dpi.dpis.len() < 6 {
                if ui.button("+ Add stage").clicked() {
                    let last = *self.dpi.dpis.last().unwrap_or(&800);
                    self.dpi.dpis.push((last + 400).min(30000));
                    self.dpi.colors.push([255, 255, 255]);
                    self.mark_dirty();
                }
            }
        });

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Sensor").size(15.0).strong());
        let mut snap = self.dpi.angle_snapping != 0;
        if ui.checkbox(&mut snap, "Angle snapping")
            .on_hover_text("Straightens lines — useful for drawing, bad for FPS.")
            .changed() {
            self.dpi.angle_snapping = if snap { 1 } else { 0 };
            self.mark_dirty();
        }
        let mut rip = self.dpi.ripple != 0;
        if ui.checkbox(&mut rip, "Ripple control")
            .on_hover_text("Reduces sensor jitter at high DPI.")
            .changed() {
            self.dpi.ripple = if rip { 1 } else { 0 };
            self.mark_dirty();
        }
    }
}

// ---------------------------------------------------------------- Buttons tab

impl IgnixApp {
    fn ui_buttons(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Button Mapping").size(15.0).strong());
        ui.label(egui::RichText::new(
            "Click a highlighted zone on the mouse, or a row below, then pick a function."
        ).size(11.0).color(DIM));
        ui.add_space(8.0);

        Self::card_frame(false).show(ui, |ui| {
            egui::Grid::new("btn_grid").num_columns(3).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
                ui.label(egui::RichText::new("Button").size(12.0).strong().color(DIM));
                ui.label(egui::RichText::new("Function").size(12.0).strong().color(DIM));
                ui.label(egui::RichText::new("").size(12.0));
                ui.end_row();
                for &(idx, name) in F5V2_BUTTONS.iter() {
                    let sel = self.selected_button == Some(idx);
                    if ui.selectable_label(sel, name).clicked() {
                        self.selected_button = Some(idx);
                        self.show_button_picker = true;
                    }
                    let (t, _, c2) = self.buttons.slots[idx];
                    let desc = match t {
                        protocol::ButtonFunc::KEYBOARD => format!("Key (HID {c2})"),
                        protocol::ButtonFunc::MACRO => format!("Macro {c2}"),
                        _ => protocol::ButtonFunc::name(t).to_string(),
                    };
                    ui.label(egui::RichText::new(desc).size(13.0)
                        .color(if sel { ACCENT } else { TEXT }));
                    if ui.small_button("Edit").clicked() {
                        self.selected_button = Some(idx);
                        self.show_button_picker = true;
                    }
                    ui.end_row();
                }
            });
        });
    }

    fn ui_button_picker(&mut self, ctx: &egui::Context) {
        let idx = match self.selected_button { Some(i) => i, None => return };
        let name = F5V2_BUTTONS.iter().find(|(i,_)| *i==idx).map(|(_,n)|*n).unwrap_or("?");
        let (cur_t, _, cur_c2) = self.buttons.slots[idx];

        egui::Window::new(format!("Remap — {name}"))
            .collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                let groups: &[(&str, &[u8], &[&str])] = &[
                    ("Mouse", &[2,3,4,5,6,7,9,10,11,12],
                     &["Left click","Right click","Middle click","Back","Forward","Double click",
                       "Wheel up","Wheel down","Wheel left","Wheel right"]),
                    ("DPI", &[13,14,15,16], &["DPI cycle","DPI +","DPI −","Sniper (hold)"]),
                    ("Media", &[24,22,23,26,27,28,21,29,30],
                     &["Play/pause","Prev track","Next track","Mute","Volume +","Volume −",
                       "Media player","Calculator","Email"]),
                ];
                for (gname, types, hints) in groups {
                    ui.label(egui::RichText::new(*gname).size(11.0).strong().color(DIM));
                    for (t, hint) in types.iter().zip(hints.iter()) {
                        if ui.radio(cur_t == *t, protocol::ButtonFunc::name(*t))
                            .on_hover_text(*hint).clicked() {
                            self.buttons.slots[idx] = (*t, 0, 0);
                            self.mark_dirty();
                        }
                    }
                    ui.add_space(4.0);
                }
                ui.separator();
                if ui.radio(cur_t == protocol::ButtonFunc::KEYBOARD, "Keyboard key")
                    .on_hover_text("Send a keyboard key press.").clicked() {
                    self.buttons.slots[idx] = (protocol::ButtonFunc::KEYBOARD, 0, 4);
                    self.mark_dirty();
                }
                if cur_t == protocol::ButtonFunc::KEYBOARD {
                    let mut hid = cur_c2.max(4) as i32;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("HID code").size(12.0).color(DIM));
                        if ui.add(egui::DragValue::new(&mut hid).range(4..=231)).changed() {
                            self.buttons.slots[idx].2 = hid as u8;
                            self.mark_dirty();
                        }
                    });
                    ui.label(egui::RichText::new("4=A … 29=Z · 30=1 … 39=0 · 40=Enter · 41=Esc · 44=Space")
                        .size(11.0).color(FAINT));
                }
                if ui.radio(cur_t == protocol::ButtonFunc::MACRO, "Macro")
                    .on_hover_text("Trigger a macro (see Macros tab).").clicked() {
                    self.buttons.slots[idx] = (protocol::ButtonFunc::MACRO, 0, 1);
                    self.mark_dirty();
                }
                if cur_t == protocol::ButtonFunc::MACRO {
                    let mut mid = cur_c2.clamp(1, 32) as i32;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Macro slot").size(12.0).color(DIM));
                        if ui.add(egui::DragValue::new(&mut mid).range(1..=32)).changed() {
                            self.buttons.slots[idx].2 = mid as u8;
                            self.mark_dirty();
                        }
                    });
                }
                if ui.radio(cur_t == protocol::ButtonFunc::DISABLED, "Disabled").clicked() {
                    self.buttons.slots[idx] = (protocol::ButtonFunc::DISABLED, 0, 0);
                    self.mark_dirty();
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("Done").strong()).clicked() {
                        self.show_button_picker = false;
                    }
                });
            });
    }
}

// ---------------------------------------------------------------- Lighting tab

impl IgnixApp {
    fn ui_lighting(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Lighting").size(15.0).strong());
        ui.add_space(8.0);

        Self::card_frame(false).show(ui, |ui| {
            let mut mode = self.light.mode;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Mode").size(12.0).color(DIM));
                egui::ComboBox::from_id_source("lightmode")
                    .selected_text(LightMode::all().iter()
                        .find(|(_,m)| *m==mode).map(|(n,_)|*n).unwrap_or("?"))
                    .show_ui(ui, |ui| {
                        for (n, m) in LightMode::all() {
                            ui.selectable_value(&mut mode, *m, *n);
                        }
                    });
            });
            if mode != self.light.mode { self.light.mode = mode; self.mark_dirty(); }
            ui.add_space(4.0);

            // color
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Color").size(12.0).color(DIM));
                if self.light.mode.uses_color() {
                    let mut c = self.light.color;
                    if ui.color_edit_button_srgb(&mut c).changed() {
                        self.light.color = c; self.mark_dirty();
                    }
                } else {
                    ui.label(egui::RichText::new("n/a for this mode").size(12.0).color(FAINT));
                }
            });
            // brightness
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Brightness").size(12.0).color(DIM));
                if self.light.mode.uses_brightness() {
                    let mut b = self.light.brightness as i32;
                    if ui.add(egui::Slider::new(&mut b, 0..=15).show_value(true)).changed() {
                        self.light.brightness = b as u8; self.mark_dirty();
                    }
                } else {
                    ui.label(egui::RichText::new("n/a").size(12.0).color(FAINT));
                }
            });
            // speed
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Speed").size(12.0).color(DIM));
                if self.light.mode.uses_speed() {
                    let mut s = self.light.speed as i32;
                    if ui.add(egui::Slider::new(&mut s, 0..=15).show_value(true)).changed() {
                        self.light.speed = s as u8; self.mark_dirty();
                    }
                } else {
                    ui.label(egui::RichText::new("n/a").size(12.0).color(FAINT));
                }
            });
        });

        ui.add_space(12.0);
        ui.label(egui::RichText::new("Power").size(15.0).strong());
        ui.add_space(4.0);
        Self::card_frame(false).show(ui, |ui| {
            let mut s1 = self.light.sleep1_min as i32;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Dim after").size(12.0).color(DIM));
                if ui.add(egui::Slider::new(&mut s1, 0..=60).text("min")).changed() {
                    self.light.sleep1_min = s1 as u8; self.mark_dirty();
                }
            });
            let mut s2 = self.light.sleep2_min as i32;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Sleep after").size(12.0).color(DIM));
                if ui.add(egui::Slider::new(&mut s2, 0..=60).text("min")).changed() {
                    self.light.sleep2_min = s2 as u8; self.mark_dirty();
                }
            });
        });
    }
}

// ---------------------------------------------------------------- Performance tab

impl IgnixApp {
    fn ui_performance(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Polling Rate").size(15.0).strong());
        ui.label(egui::RichText::new("Higher = lower latency, more CPU.").size(11.0).color(DIM));
        ui.add_space(8.0);
        Self::card_frame(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                for hz in [125u32, 250, 500, 1000] {
                    if ui.radio(self.rate == hz, format!("{hz} Hz")).clicked() {
                        self.rate = hz; self.mark_dirty();
                    }
                }
            });
        });

        ui.add_space(16.0);
        ui.label(egui::RichText::new("Click Response").size(15.0).strong());
        ui.add_space(4.0);
        Self::card_frame(false).show(ui, |ui| {
            let mut db = self.light.debounce_ms as i32;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Debounce").size(12.0).color(DIM));
                let resp = ui.add(egui::Slider::new(&mut db, 2..=20).text("ms"))
                    .on_hover_text("Lower = faster clicks, higher double-click risk.");
                if resp.changed() {
                    self.light.debounce_ms = db as u8; self.mark_dirty();
                }
            });
            ui.label(egui::RichText::new("Default 8 ms.").size(11.0).color(FAINT));
        });
    }
}

// ---------------------------------------------------------------- Macros tab

impl IgnixApp {
    fn ui_macros(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Macros").size(15.0).strong());
        ui.label(egui::RichText::new(
            "32 slots. Assign a macro to a button in the Buttons tab."
        ).size(11.0).color(DIM));
        ui.add_space(8.0);
        Self::card_frame(false).show(ui, |ui| {
            ui.label(egui::RichText::new(
                "Macro recording/upload (report 9) is implemented in the protocol\n\
                 but the recorder UI is not finished yet."
            ).size(13.0).color(WARN));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(
                "Button → macro assignment works now. Full editor coming next update."
            ).size(11.0).color(DIM));
        });
    }
}
