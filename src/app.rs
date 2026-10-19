//! egui app: worker thread owns the `Mouse`, UI talks via mpsc and never
//! blocks on device I/O.

use crate::device::{DeviceState, Mouse};
use crate::protocol::*;
use eframe::egui;
use std::sync::mpsc;
use std::time::Duration;

pub enum Cmd {
    /// Re-detect the device (open failed earlier) / full re-read.
    Refresh,
    /// Switch the profile that all subsequent ops address (report byte 2).
    SetProfile(u8),
    /// Write every tab back to the mouse; sends per-part status messages.
    Apply {
        dpi: DpiConfig,
        light: LightConfig,
        rate: RateConfig,
        buttons: [BtnFunc; MAX_BUTTON_SLOTS],
        macros: Vec<Macro>,
    },
    WriteMacro(Macro),
}

pub enum Msg {
    Ready(Res<DeviceState>),
    /// (part name, detail, ok) entries after a write batch.
    Done(Vec<(String, String, bool)>),
}

fn with_mouse<'a>(
    slot: &'a mut Option<Mouse>,
    tx: &mpsc::Sender<Msg>,
) -> Option<&'a mut Mouse> {
    if slot.is_none() {
        match Mouse::open() {
            Ok(m) => *slot = Some(m),
            Err(e) => {
                let _ = tx.send(Msg::Ready(Err(e)));
                return None;
            }
        }
    }
    slot.as_mut()
}

fn device_gone(e: &str) -> bool {
    e.contains("No such device") || e.contains("Broken pipe")
}

fn refresh_hint(e: &str) -> String {
    if e.contains("Broken pipe") {
        "Device config stalled (wired mode is silent on this firmware) — plug the 2.4G dongle, then press Refresh.".to_string()
    } else {
        "Device disappeared — reconnected? Press Refresh.".to_string()
    }
}

fn worker_loop(rx: mpsc::Receiver<Cmd>, tx: mpsc::Sender<Msg>) {
    let mut mouse: Option<Mouse> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Refresh => {
                if let Some(m) = with_mouse(&mut mouse, &tx) {
                    match m.read_all() {
                        Ok(s) => {
                            let _ = tx.send(Msg::Ready(Ok(s)));
                        }
                        Err(e) => {
                            if device_gone(&e) {
                                let _ = tx.send(Msg::Ready(Err(refresh_hint(&e))));
                                mouse = None;
                                continue;
                            }
                            let _ = tx.send(Msg::Ready(Err(e)));
                        }
                    }
                }
            }
            Cmd::SetProfile(p) => {
                if let Some(m) = with_mouse(&mut mouse, &tx) {
                    let res = m.set_profile(p).and_then(|_| m.read_all());
                    if let Err(e) = &res {
                        if device_gone(e) {
                            mouse = None;
                            let _ = tx.send(Msg::Ready(Err(refresh_hint(e))));
                            continue;
                        }
                    }
                    let _ = tx.send(Msg::Ready(res));
                }
            }
            Cmd::Apply { dpi, light, rate, buttons, macros } => {
                if let Some(m) = with_mouse(&mut mouse, &tx) {
                    let mut results: Vec<(String, Res<()>)> = Vec::new();
                    // Same ordering as the official data sync: keys, macros,
                    // DPI, rate, lighting.
                    results.push(("Buttons".into(), m.write_buttons(&buttons)));
                    for mac in &macros {
                        results.push((format!("Macro slot {}", mac.slot), m.write_macro(mac)));
                    }
                    results.push(("DPI".into(), m.write_dpi(&dpi)));
                    results.push(("Polling rate".into(), m.write_rate(&rate)));
                    results.push(("Lighting".into(), m.write_light(&light)));
                    let mut gone = false;
                    let done = results
                        .into_iter()
                        .map(|(name, r)| match r {
                            Ok(()) => (name, "written and verified".to_string(), true),
                            Err(e) => {
                                gone |= device_gone(&e);
                                (name, e, false)
                            }
                        })
                        .collect();
                    if gone {
                        mouse = None;
                    }
                    let _ = tx.send(Msg::Done(done));
                    if gone {
                        let _ = tx.send(Msg::Ready(Err("Config exchanges failed mid-write (wired mode is silent) — plug the 2.4G dongle, then press Refresh.".into())));
                    }
                }
            }
            Cmd::WriteMacro(mac) => {
                if let Some(m) = with_mouse(&mut mouse, &tx) {
                    let name = format!("Macro slot {}", mac.slot);
                    let done = vec![match m.write_macro(&mac) {
                        Ok(()) => (name, "uploaded".to_string(), true),
                        Err(e) => {
                            if device_gone(&e) {
                                mouse = None;
                            }
                            (name, e, false)
                        }
                    }];
                    let _ = tx.send(Msg::Done(done));
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tab {
    Dpi,
    Buttons,
    Macros,
    Lighting,
    Performance,
}

pub const LIGHT_MODES: [(u8, &str); 8] = [
    (7, "Off"),
    (0, "Static"),
    (1, "Breathing"),
    (2, "Single Color Cycle"),
    (3, "Rainbow Cycle"),
    (4, "Marquee"),
    (5, "Flow"),
    (6, "APM"),
];

const FN_OPTIONS: [(&str, u8); 21] = [
    ("Disabled", FN_DISABLED),
    ("Left Click", FN_BUTTON_LEFT),
    ("Right Click", FN_BUTTON_RIGHT),
    ("Middle Click", FN_BUTTON_MIDDLE),
    ("Back", FN_BUTTON_BACK),
    ("Forward", FN_BUTTON_FORWARD),
    ("Double Click", FN_DOUBLE_CLICK),
    ("Wheel Up", FN_WHEEL_UP),
    ("Wheel Down", FN_WHEEL_DOWN),
    ("Wheel Left", FN_WHEEL_LEFT),
    ("Wheel Right", FN_WHEEL_RIGHT),
    ("DPI Cycle", FN_DPI_CYCLE),
    ("DPI Up", FN_DPI_UP),
    ("DPI Down", FN_DPI_DOWN),
    ("Sniper (DPI Shift)", FN_DPI_SHIFT),
    ("Keyboard Key", FN_KEYBOARD),
    ("Macro", FN_MACRO),
    ("Media 1", FN_MEDIA_BASE),
    ("Media 2", FN_MEDIA_BASE + 1),
    ("Calculator", FN_CALCULATOR),
    ("Email", FN_EMAIL),
];

fn fn_name(ty: u8, c2: u8) -> String {
    match ty {
        FN_KEYBOARD => format!("Keyboard ({})", hid_label(c2)),
        FN_MACRO => format!("Macro {}", c2.max(1).min(MAX_MACRO_SLOTS as u8)),
        _ => FN_OPTIONS
            .iter()
            .find(|(_, t)| *t == ty)
            .map(|(n, _)| n.to_string())
            .unwrap_or_else(|| format!("Type {}", ty)),
    }
}

/// The 6 physical button slots this app exposes (0..4 + the DPI button).
const PHYSICAL_SLOTS: [usize; 6] = [0, 1, 2, 3, 4, 8];

fn slot_label(idx: usize) -> &'static str {
    match idx {
        0 => "Left button",
        1 => "Right button",
        2 => "Middle button",
        3 => "Side button 3",
        4 => "Side button 4",
        8 => "DPI button",
        _ => "Button",
    }
}

fn hid_label(code: u8) -> String {
    match code {
        4..=29 => ((b'A' + code - 4) as char).to_string(),
        30..=38 => ((b'1' + code - 30) as char).to_string(),
        39 => "0".to_string(),
        40 => "Enter".into(),
        42 => "Backspace".into(),
        43 => "Tab".into(),
        44 => "Space".into(),
        79 => "Right".into(),
        80 => "Left".into(),
        81 => "Down".into(),
        82 => "Up".into(),
        0xE0..=0xE7 => MOD_NAMES[(code - 0xE0) as usize].to_string(),
        _ => format!("0x{:02X}", code),
    }
}

const MOD_NAMES: [&str; 8] = [
    "Left Ctrl", "Left Shift", "Left Alt", "Left GUI", "Right Ctrl", "Right Shift", "Right Alt", "Right GUI",
];

const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x00, 0xE5, 0x9C);
const CARD_BG: egui::Color32 = egui::Color32::from_rgb(0x1D, 0x20, 0x2A);
const CARD_STROKE: egui::Color32 = egui::Color32::from_rgb(0x2E, 0x33, 0x42);

/// In-session macro slots (slot 1..=32). Names live only on the PC side.
#[derive(Clone, Debug, Default)]
struct MacroTab {
    name: String,
    loop_type: u8,
    repeat: u8,
    rgb: [u8; 3],
    events: Vec<MacroEvent>,
}

/// Ready-made macros (device-verified encoding: delays ride the next event).
/// (name, loop_type, repeat, events)
const PRESETS: [(&str, u8, u8, &[MacroEvent]); 5] = [
    (
        "Double click (hold button)",
        1, // repeat while held
        1,
        &[
            MacroEvent { kind: MacroKind::Click(0), delay_ms: 30 },
            MacroEvent { kind: MacroKind::Wait, delay_ms: 40 },
            MacroEvent { kind: MacroKind::Click(0), delay_ms: 30 },
        ],
    ),
    (
        "Click spam 10/s (hold)",
        1,
        1,
        &[MacroEvent { kind: MacroKind::Click(0), delay_ms: 100 }],
    ),
    (
        "Hold W (AFK walk)",
        1,
        1,
        &[
            MacroEvent { kind: MacroKind::Press(26), delay_ms: 10 },  // W
            MacroEvent { kind: MacroKind::Wait, delay_ms: 2000 },
            MacroEvent { kind: MacroKind::Release(26), delay_ms: 10 },
        ],
    ),
    (
        "Reload + quick switch",
        0,
        1,
        &[
            MacroEvent { kind: MacroKind::Press(21), delay_ms: 10 },  // R
            MacroEvent { kind: MacroKind::Release(21), delay_ms: 40 },
            MacroEvent { kind: MacroKind::Wait, delay_ms: 550 },
            MacroEvent { kind: MacroKind::Press(20), delay_ms: 10 },  // Q
            MacroEvent { kind: MacroKind::Release(20), delay_ms: 40 },
        ],
    ),
    (
        "Jump loop (bunny hop, hold)",
        1,
        1,
        &[
            MacroEvent { kind: MacroKind::Press(44), delay_ms: 10 },  // Space
            MacroEvent { kind: MacroKind::Wait, delay_ms: 160 },
            MacroEvent { kind: MacroKind::Release(44), delay_ms: 40 },
        ],
    ),
];

pub struct IgnixApp {
    tx: mpsc::Sender<Cmd>,
    rx: mpsc::Receiver<Msg>,
    state: Option<DeviceState>,
    error: Option<String>,
    statuses: Vec<(String, String, bool)>,
    busy: bool,
    tab: Tab,
    macros: [MacroTab; MAX_MACRO_SLOTS],
    selected_macro: usize,
}

impl IgnixApp {
    /// Worker thread is spawned at construction; device opens lazily.
    pub fn new() -> Self {
        let (tx, rx_cmd) = mpsc::channel::<Cmd>();
        let (msg_tx, rx) = mpsc::channel::<Msg>();
        std::thread::spawn(move || worker_loop(rx_cmd, msg_tx));
        let _ = tx.send(Cmd::Refresh);
        let macros: [MacroTab; MAX_MACRO_SLOTS] = std::array::from_fn(|_| MacroTab { repeat: 1, ..MacroTab::default() });
        Self {
            tx,
            rx,
            state: None,
            error: None,
            statuses: Vec::new(),
            busy: false,
            tab: Tab::Dpi,
            macros,
            selected_macro: 0,
        }
    }

    fn poll_msgs(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Ready(Ok(state)) => {
                    self.state = Some(state);
                    self.error = None;
                    self.busy = false;
                }
                Msg::Ready(Err(e)) => {
                    self.error = Some(e);
                    self.busy = false;
                }
                Msg::Done(list) => {
                    self.statuses = list;
                    self.busy = false;
                }
            }
        }
    }

    fn apply(&mut self) {
        let Some(state) = self.state.clone() else { return };
        let macros = self
            .macros
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.events.is_empty())
            .map(|(i, m)| Macro {
                slot: (i + 1) as u8,
                loop_type: m.loop_type,
                repeat: m.repeat.max(1),
                rgb: m.rgb,
                events: m.events.clone(),
            })
            .collect();
        self.busy = true;
        let _ = self.tx.send(Cmd::Apply {
            dpi: state.dpi.clone(),
            light: state.light.clone(),
            rate: state.rate.clone(),
            buttons: state.buttons.clone(),
            macros,
        });
    }
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.set_pixels_per_point(1.3);
    let mut st = (*ctx.style()).clone();
    st.visuals = egui::Visuals::dark();
    st.visuals.override_text_color = Some(egui::Color32::from_rgb(0xE8, 0xEA, 0xEF));
    st.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.32);
    st.visuals.selection.stroke = egui::Stroke::new(1.5_f32, ACCENT);
    st.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, ACCENT);
    st.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.8_f32, ACCENT);
    st.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x27, 0x2B, 0x36);
    st.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(0x17, 0x19, 0x20);
    for w in [
        &mut st.visuals.widgets.noninteractive,
        &mut st.visuals.widgets.inactive,
        &mut st.visuals.widgets.hovered,
        &mut st.visuals.widgets.active,
        &mut st.visuals.widgets.open,
    ] {
        w.rounding = egui::Rounding::same(6.0);
    }
    st.spacing.item_spacing = egui::vec2(10.0, 10.0);
    st.spacing.button_padding = egui::vec2(14.0, 7.0);
    st.spacing.menu_margin = egui::Margin::same(10.0);
    ctx.set_style(st);
}

fn card<R>(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui) -> R) {
    egui::Frame::group(ui.style())
        .fill(CARD_BG)
        .stroke(egui::Stroke::new(1.0_f32, CARD_STROKE))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::same(16.0))
        .outer_margin(egui::Margin::same(2.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .size(17.0)
                    .strong()
                    .color(ACCENT),
            );
            ui.add_space(8.0);
            body(ui);
        });
    ui.add_space(6.0);
}

impl eframe::App for IgnixApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_msgs();
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(150));
        }

        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(0x15, 0x17, 0x1E))
                    .inner_margin(egui::Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| {
                ui.columns(2, |cols| {
                    cols[0].horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("IGNIX V2")
                                .size(22.0)
                                .strong()
                                .color(ACCENT),
                        );
                        ui.add_space(8.0);
                        match (&self.state, &self.error) {
                            (Some(s), _) => {
                                ui.colored_label(egui::Color32::from_rgb(0x35, 0xD6, 0x77), "Connected");
                                ui.label(
                                    egui::RichText::new(format!("Firmware {}  ·  Profile {}", s.firmware, s.profile))
                                        .weak(),
                                );
                            }
                            (None, Some(e)) => {
                                ui.colored_label(egui::Color32::from_rgb(0xE5, 0x51, 0x4E), "Not connected");
                                ui.label(egui::RichText::new(e).weak());
                            }
                            (None, None) => {
                                ui.spinner();
                                ui.label("Opening device…");
                            }
                        }
                    });
                    cols[1].with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::widgets::Button::new(
                                egui::RichText::new("Refresh").strong(),
                            ))
                            .clicked()
                        {
                            self.busy = true;
                            let _ = self.tx.send(Cmd::Refresh);
                        }
                        ui.label(egui::RichText::new("Battery: --").weak());
                    });
                });
            });

        egui::TopBottomPanel::top("tabs")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(0x18, 0x1B, 0x23))
                    .inner_margin(egui::Margin::symmetric(16.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                ui.horizontal(|ui| {
                    for (tab, label) in
                        [(Tab::Dpi, "DPI"), (Tab::Buttons, "Buttons"), (Tab::Macros, "Macros"), (Tab::Lighting, "Lighting"), (Tab::Performance, "Performance")]
                    {
                        let selected = self.tab == tab;
                        let text = egui::RichText::new(label).size(17.0);
                        let text = if selected { text.strong().color(ACCENT) } else { text };
                        if ui.selectable_label(selected, text).clicked() {
                            self.tab = tab;
                        }
                    }
                });
            });

        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(0x15, 0x17, 0x1E))
                    .inner_margin(egui::Margin::symmetric(16.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let enabled = self.state.is_some() && !self.busy;
                    let apply_btn =
                        egui::widgets::Button::new(egui::RichText::new("Apply to Mouse").strong().color(
                            if enabled { egui::Color32::BLACK } else { egui::Color32::GRAY },
                        ))
                        .fill(if enabled { ACCENT } else { egui::Color32::from_rgb(0x2A, 0x2E, 0x39) })
                        .min_size(egui::vec2(170.0, 34.0))
                        .rounding(egui::Rounding::same(8.0));
                    if ui.add_enabled(enabled, apply_btn).clicked() {
                        self.apply();
                    }
                    if self.busy {
                        ui.spinner();
                        ui.label("Working… (device I/O on background thread)");
                    }
                    if ui
                        .add_enabled(self.state.is_some() && !self.busy, egui::widgets::Button::new("Refresh settings"))
                        .clicked()
                    {
                        self.busy = true;
                        let _ = self.tx.send(Cmd::Refresh);
                    }
                });
                if !self.statuses.is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(100.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for (part, detail, ok) in &self.statuses {
                                let (icon, color) = if *ok {
                                    ("OK", egui::Color32::from_rgb(0x35, 0xD6, 0x77))
                                } else {
                                    ("FAILED", egui::Color32::from_rgb(0xE5, 0x51, 0x4E))
                                };
                                ui.horizontal(|ui| {
                                    ui.colored_label(color, egui::RichText::new(icon).strong());
                                    ui.label(egui::RichText::new(part).strong());
                                    ui.label(egui::RichText::new(detail).weak());
                                });
                            }
                        });
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(0x1A, 0x1D, 0x24)).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| match self.tab {
                    Tab::Dpi => self.ui_dpi(ui),
                    Tab::Buttons => self.ui_buttons(ui),
                    Tab::Macros => self.ui_macros(ui),
                    Tab::Lighting => self.ui_lighting(ui),
                    Tab::Performance => self.ui_performance(ui),
                });
            });
    }
}

impl IgnixApp {
    fn ui_dpi(&mut self, ui: &mut egui::Ui) {
        if self.state.is_none() {
            self.no_device_msg(ui);
            return;
        }
        let mut new_active: Option<usize> = None;
        let mut remove: Option<usize> = None;
        let mut add_stage = false;
        card(ui, "DPI Stages", |ui| {
            let state = self.state.as_mut().unwrap();
            let dpi = &mut state.dpi;
            ui.label("Click a stage radio to make it the active step.");
            ui.add_space(6.0);
            egui::Grid::new("dpi_grid")
                .num_columns(4)
                .spacing([18.0, 10.0])
                .show(ui, |ui| {
                    for i in 0..dpi.stages.len() {
                        let stage = &mut dpi.stages[i];
                        if ui
                            .add(egui::widgets::RadioButton::new(dpi.active == i, format!("Stage {}", i + 1)))
                            .clicked()
                        {
                            new_active = Some(i);
                        }
                        ui.add(
                            egui::DragValue::new(&mut stage.dpi)
                                .range(MIN_DPI..=MAX_DPI)
                                .speed(DPI_STEP as f64)
                                .suffix(" dpi"),
                        );
                        ui.color_edit_button_srgb(&mut stage.color);
                        if dpi.stages.len() > 1 && ui.small_button("×").clicked() {
                            remove = Some(i);
                        } else if dpi.stages.len() <= 1 {
                            ui.label("");
                        }
                    }
                });
            ui.add_space(4.0);
            ui.add_enabled(
                dpi.stages.len() < MAX_STAGES,
                egui::widgets::Button::new("Add stage (copies last)"),
            )
            .clicked()
            .then(|| add_stage = true);
        });
        if let Some(i) = new_active {
            self.state.as_mut().unwrap().dpi.active = i;
        }
        if let Some(i) = remove {
            let dpi = &mut self.state.as_mut().unwrap().dpi;
            dpi.stages.remove(i);
            dpi.active = dpi.active.min(dpi.stages.len() - 1);
        }
        if add_stage {
            let dpi = &mut self.state.as_mut().unwrap().dpi;
            let last = dpi.stages.last().copied().unwrap_or(DpiStage { dpi: 1600, color: [255, 255, 255] });
            dpi.stages.push(last);
        }
        card(ui, "Sensor", |ui| {
            let state = self.state.as_mut().unwrap();
            let dpi = &mut state.dpi;
            ui.checkbox(&mut dpi.angle_snap, "Angle snapping");
            ui.label("Active-DPI indicator:");
            ui.horizontal(|ui| {
                for (v, n) in [(1u8, "Type 1"), (2, "Type 2"), (3, "Type 3")] {
                    ui.radio_value(&mut dpi.indication, v, n);
                }
            });
        });
    }

    fn no_device_msg(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("No device — connect the mouse and press Refresh.").weak());
    }

    fn ui_buttons(&mut self, ui: &mut egui::Ui) {
        if self.state.is_none() {
            self.no_device_msg(ui);
            return;
        };
        card(ui, "Button Remap", |ui| {
            ui.label("Only the 6 physical buttons are exposed; every other slot is written back unchanged.");
            ui.add_space(6.0);
            egui::Grid::new("btn_grid")
                .spacing([18.0, 10.0])
                .min_col_width(150.0)
                .show(ui, |ui| {
                    for idx in PHYSICAL_SLOTS {
                        let Some(f) = self.state.as_mut().unwrap().buttons.get_mut(idx) else { continue };
                        ui.label(egui::RichText::new(slot_label(idx)).strong());
                        ui.horizontal(|ui| {
                            let mut changed = false;
                            egui::ComboBox::from_id_source(format!("btn{}", idx))
                                .selected_text(fn_name(f.ty, f.code2))
                                .width(230.0)
                                .show_ui(ui, |ui| {
                                    for (name, ty) in FN_OPTIONS {
                                        changed |= ui.selectable_value(&mut f.ty, ty, name).changed();
                                    }
                                });
                            if changed && f.ty != FN_KEYBOARD && f.ty != FN_MACRO {
                                f.code1 = 0;
                                f.code2 = 0;
                            }
                            match f.ty {
                                FN_KEYBOARD => {
                                    ui.label("Modifier:");
                                    ui.add(egui::DragValue::new(&mut f.code1).range(0..=255));
                                    ui.label("HID usage:");
                                    ui.add(egui::DragValue::new(&mut f.code2).range(0..=255));
                                    ui.label(hid_label(f.code2));
                                }
                                FN_MACRO => {
                                    ui.label("Slot:");
                                    ui.add(egui::DragValue::new(&mut f.code2).range(1..=MAX_MACRO_SLOTS as u8));
                                }
                                _ => {}
                            }
                        });
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
            if ui.button("Reset all to F5V2 defaults").clicked() {
                self.state.as_mut().unwrap().buttons = default_buttons();
            }
        });
    }

    fn ui_macros(&mut self, ui: &mut egui::Ui) {
        card(ui, "Macro Slots", |ui| {
            ui.label("Filled slots are uploaded on Apply, or per-slot below.");
            ui.add_space(6.0);
            egui::Grid::new("macro_slots")
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    for i in 0..MAX_MACRO_SLOTS {
                        let filled = !self.macros[i].events.is_empty();
                        let label = if filled {
                            egui::RichText::new(format!("{}", i + 1)).strong()
                        } else {
                            egui::RichText::new(format!("{}", i + 1)).weak()
                        };
                        let btn = egui::widgets::Button::new(label)
                            .min_size(egui::vec2(52.0, 30.0))
                            .rounding(egui::Rounding::same(6.0));
                        let resp = ui.add(btn);
                        if filled {
                            let rect = resp.rect.expand2(egui::vec2(-4.0, -4.0));
                            ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.5_f32, ACCENT));
                        }
                        if resp.clicked() {
                            self.selected_macro = i;
                        }
                        if (i + 1) % 8 == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
        let i = self.selected_macro;
        let slot_no = (i + 1) as u8;
        let mut remove: Option<usize> = None;
        let mut upload: Option<Macro> = None;
        let mut clear_slot = false;
        let mut preset: Option<usize> = None;
        card(ui, format!("Slot {} — Editor", slot_no).as_str(), |ui| {
            let slot = &mut self.macros[i];
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut slot.name)
                        .desired_width(240.0)
                        .hint_text("Name (local only)"),
                );
                ui.label("Loop:");
                for (v, n) in [(0u8, "Once"), (1, "While held"), (2, "Toggle")] {
                    ui.radio_value(&mut slot.loop_type, v, n);
                }
                ui.label("Repeat:");
                ui.add(egui::DragValue::new(&mut slot.repeat).range(1..=255));
                ui.color_edit_button_srgb(&mut slot.rgb);
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Load preset:");
                egui::ComboBox::from_id_source("preset")
                    .selected_text("choose…")
                    .width(240.0)
                    .show_ui(ui, |ui| {
                        for (n, (_name, _loop, _rep, _ev)) in PRESETS.iter().enumerate() {
                            ui.selectable_value(&mut preset, Some(n), _name.to_string());
                        }
                    });
                if ui.button("Clear slot").clicked() {
                    clear_slot = true;
                }
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "Events {} / {} — delay rides on the next event; max {} pairs",
                    slot.events.len(),
                    MAX_MACRO_EVENTS,
                    MAX_MACRO_EVENTS
                ))
                .weak(),
            );
            ui.add_space(4.0);
            egui::Grid::new("events")
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    for (j, ev) in slot.events.iter_mut().enumerate() {
                        let cur_key = match ev.kind {
                            MacroKind::Press(c) | MacroKind::Release(c) => c,
                            MacroKind::Click(_) | MacroKind::Wait => 4,
                        };
                        let cur_mouse = match ev.kind { MacroKind::Click(c) => c, _ => 0 };
                        egui::ComboBox::from_id_source(format!("evkind{}", j))
                            .selected_text(kind_text(ev.kind))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut ev.kind, MacroKind::Click(cur_mouse), kind_text(MacroKind::Click(cur_mouse)));
                                ui.selectable_value(&mut ev.kind, MacroKind::Press(cur_key), format!("Key press: {}", hid_label(cur_key)));
                                ui.selectable_value(&mut ev.kind, MacroKind::Release(cur_key), format!("Key release: {}", hid_label(cur_key)));
                                ui.selectable_value(&mut ev.kind, MacroKind::Wait, "Delay".to_string());
                            });
                        match &mut ev.kind {
                            MacroKind::Press(code) | MacroKind::Release(code) => {
                                ui.add(egui::DragValue::new(code).range(0..=255));
                                ui.colored_label(egui::Color32::GRAY, hid_label(*code));
                            }
                            MacroKind::Click(btn) => {
                                ui.add(egui::DragValue::new(btn).range(0..=4));
                                ui.colored_label(egui::Color32::GRAY, mouse_btn_code_text(*btn));
                            }
                            MacroKind::Wait => {
                                ui.label("");
                            }
                        }
                        ui.add(
                            egui::DragValue::new(&mut ev.delay_ms)
                                .range(10..=52270)
                                .speed(10.0)
                                .suffix(" ms"),
                        );
                        if ui.small_button("×").clicked() {
                            remove = Some(j);
                        }
                        ui.end_row();
                    }
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let mut add = |ev: MacroEvent| {
                    if slot.events.len() < MAX_MACRO_EVENTS {
                        slot.events.push(ev);
                    }
                };
                if ui.button("Add Key Press").clicked() {
                    add(MacroEvent { kind: MacroKind::Press(4), delay_ms: 10 });
                }
                if ui.button("Add Key Release").clicked() {
                    add(MacroEvent { kind: MacroKind::Release(4), delay_ms: 10 });
                }
                if ui.button("Add Mouse Click").clicked() {
                    add(MacroEvent { kind: MacroKind::Click(0), delay_ms: 40 });
                }
                if ui.button("Add Delay").clicked() {
                    add(MacroEvent { kind: MacroKind::Wait, delay_ms: 100 });
                }
                let m = Macro {
                    slot: slot_no,
                    loop_type: slot.loop_type,
                    repeat: slot.repeat.max(1),
                    rgb: slot.rgb,
                    events: slot.events.clone(),
                };
                let filled = !slot.events.is_empty();
                if ui
                    .add_enabled(!slot.events.is_empty(), egui::widgets::Button::new(egui::RichText::new("Upload this slot").strong()))
                    .clicked()
                {
                    upload = Some(m);
                }
                if filled {
                    let _ = filled;
                }
            });
        });
        if let Some(j) = remove {
            self.macros[i].events.remove(j);
        }
        if clear_slot {
            let repeat = self.macros[i].repeat;
            self.macros[i] = MacroTab::default();
            self.macros[i].repeat = repeat.max(1);
        }
        if let Some(n) = preset {
            let (name, loop_type, repeat, events) = PRESETS[n];
            self.macros[i].name = name.to_string();
            self.macros[i].events = events.to_vec();
            self.macros[i].loop_type = loop_type;
            self.macros[i].repeat = repeat.max(1);
        }
        if let Some(m) = upload {
            self.busy = true;
            let _ = self.tx.send(Cmd::WriteMacro(m));
        }
    }

    fn ui_lighting(&mut self, ui: &mut egui::Ui) {
        if self.state.is_none() {
            self.no_device_msg(ui);
            return;
        };
        card(ui, "Lighting", |ui| {
            let state = self.state.as_mut().unwrap();
            let light = &mut state.light;
            egui::Grid::new("light_modes")
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    for (k, (v, name)) in LIGHT_MODES.iter().enumerate() {
                        ui.radio_value(&mut light.mode, *v, *name);
                        if (k + 1) % 4 == 0 {
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(6.0);
            let mode = light.mode;
            if mode == 0 || mode == 1 {
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    ui.color_edit_button_srgb(&mut light.color);
                });
            }
            if mode != 7 {
                ui.horizontal(|ui| {
                    ui.label("Brightness:");
                    ui.add_sized([300.0, 20.0], egui::Slider::new(&mut light.brightness, 0..=15));
                });
                if (1..=4).contains(&mode) {
                    ui.horizontal(|ui| {
                        ui.label("Speed:");
                        ui.add_sized([300.0, 20.0], egui::Slider::new(&mut light.speed, 0..=15));
                    });
                }
            }
        });
        card(ui, "Power & Debounce", |ui| {
            let state = self.state.as_mut().unwrap();
            let light = &mut state.light;
            egui::Grid::new("sleep")
                .spacing([18.0, 10.0])
                .show(ui, |ui| {
                    ui.label("Sleep level 1 (minutes)");
                    ui.add(egui::DragValue::new(&mut light.sleep1_min).range(0..=255));
                    ui.end_row();
                    ui.label("Sleep level 2 (minutes)");
                    ui.add(egui::DragValue::new(&mut light.sleep2_min).range(0..=255));
                    ui.end_row();
                    ui.label("Button debounce");
                    ui.add_sized([80.0, 22.0], egui::DragValue::new(&mut light.debounce_ms).range(0..=510).speed(2.0).suffix(" ms"));
                    ui.end_row();
                });
        });
    }

    fn ui_performance(&mut self, ui: &mut egui::Ui) {
        if self.state.is_none() {
            self.no_device_msg(ui);
            return;
        };
        card(ui, "Polling Rate", |ui| {
            let state = self.state.as_mut().unwrap();
            ui.horizontal(|ui| {
                for (hz, code) in [(125u16, 8u8), (250, 4), (500, 2), (1000, 1)] {
                    let selected = state.rate.0 == code;
                    let text = egui::RichText::new(format!("{} Hz", hz));
                    let text = if selected { text.strong().color(ACCENT) } else { text };
                    if ui.selectable_label(selected, text).clicked() {
                        state.rate.0 = code;
                    }
                }
            });
        });
        card(ui, "Profile", |ui| {
            let state = self.state.as_mut().unwrap();
            ui.label(format!("Editing profile {} (each profile stores its own DPI, lighting, etc.)", state.profile));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for p in 1..=5 {
                    let selected = state.profile == p;
                    let text = egui::RichText::new(format!("P{}", p));
                    let text = if selected { text.strong() } else { text };
                    if ui.selectable_label(selected, text).clicked() {
                        self.busy = true;
                        let _ = self.tx.send(Cmd::SetProfile(p));
                    }
                }
            });
            let _ = &state;
        });
        card(ui, "Device Info", |ui| {
            let state = self.state.as_mut().unwrap();
            egui::Grid::new("info")
                .spacing([24.0, 8.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Firmware").weak());
                    ui.label(format!("v{}", state.firmware));
                    ui.end_row();
                    ui.label(egui::RichText::new("Identity").weak());
                    ui.label("Xenta F5V2 (deviceId 174), VID 1D57");
                    ui.end_row();
                    ui.label(egui::RichText::new("Battery").weak());
                    ui.label("--");
                    ui.end_row();
                });
        });
    }
}

fn kind_text(k: MacroKind) -> String {
    match k {
        MacroKind::Press(c) => format!("Key press: {}", hid_label(c)),
        MacroKind::Release(c) => format!("Key release: {}", hid_label(c)),
        MacroKind::Click(b) => format!("Mouse click: {}", mouse_btn_code_name(b)),
        MacroKind::Wait => "Delay".to_string(),
    }
}

fn mouse_btn_code_text(b: u8) -> String {
    format!("Mouse {}", mouse_btn_code_name(b))
}

fn mouse_btn_code_name(b: u8) -> &'static str {
    ["Left", "Right", "Middle", "Back", "Forward"]
        .get(b as usize)
        .copied()
        .unwrap_or("button")
}
