//! HID protocol for Vortex Ignix V2 (Xenta IGNIX F5V2, USB 1d57:22ae)
//!
//! Reverse-engineered from BK-MouseHub's Electron app.
//! All config uses HID Feature Reports on USB interface 2.

use std::time::Duration;

// ---------------------------------------------------------------- constants

pub const VID: u16 = 0x1D57;
pub const PID: u16 = 0x22AE;        // wired (USB cable)
pub const PID_DONGLE: u16 = 0xFA60; // 2.4G wireless dongle
pub const PROFILE: u8 = 1;

/// (send_len, read_len) per report ID.
/// GET_REPORT length must be exact or the device STALLs.
pub fn report_lens(report_id: u8) -> (usize, usize) {
    match report_id {
        4 => (56, 64),   // DPI
        5 => (15, 64),   // lighting/debounce/sleep
        6 => (9, 64),    // polling rate
        8 => (59, 64),   // button remap
        9 => (64, 64),   // macro upload (write-only)
        11 => (8, 6),    // firmware version (read-only)
        12 => (9, 6),    // profile id (read-only)
        160 => (8, 8),   // mailbox
        _ => panic!("unknown report {report_id}"),
    }
}

// ---------------------------------------------------------------- checksum

/// 16-bit checksum: sum of data[start..end], big-endian output.
pub fn checksum(data: &[u8], start: usize, end: usize) -> (u8, u8) {
    let s: u32 = data[start..end].iter().map(|&b| b as u32).sum();
    let s = s & 0xFFFF;
    (((s >> 8) & 0xFF) as u8, (s & 0xFF) as u8)
}

// ---------------------------------------------------------------- DPI (report 4)

pub const LEVEL_MASKS: [u8; 8] = [1, 3, 7, 15, 31, 63, 127, 255];
pub const MAX_DPI_LEVELS: usize = 6;
pub const DPI_MIN: u16 = 50;
pub const DPI_MAX: u16 = 30000;

#[derive(Debug, Clone)]
pub struct DpiConfig {
    pub dpis: Vec<u16>,       // active levels only
    pub colors: Vec<[u8; 3]>, // per-level RGB
    pub active: usize,        // 0-based index into dpis
    pub angle_snapping: u8,
    pub ripple: u8,
    pub indication: u8,
}

impl Default for DpiConfig {
    fn default() -> Self {
        Self {
            dpis: vec![1200, 2400, 3600, 4800, 8000, 30000],
            colors: vec![
                [255, 0, 0], [0, 255, 0], [0, 0, 255],
                [255, 255, 0], [0, 255, 255], [255, 0, 255],
            ],
            active: 0,
            angle_snapping: 0,
            ripple: 1,
            indication: 2,
        }
    }
}

/// Decode a 64-byte DPI report (parse first 52 bytes).
pub fn parse_dpi(d: &[u8]) -> Result<DpiConfig, String> {
    if d.len() < 52 || d[0] != 4 {
        return Err("Bad DPI report".into());
    }
    let count = d[5].count_ones() as usize;
    let mut dpis = Vec::with_capacity(count);
    let mut colors = Vec::with_capacity(count);
    for i in 0..8 {
        let raw = ((d[16 + i] as u16) << 8) | d[8 + i] as u16;
        if i < count {
            dpis.push(raw);
            colors.push([d[25 + 3 * i], d[26 + 3 * i], d[27 + 3 * i]]);
        }
    }
    Ok(DpiConfig {
        dpis,
        colors,
        active: d[24].saturating_sub(1) as usize,
        angle_snapping: d[3],
        ripple: d[4],
        indication: d[49],
    })
}

/// Encode DPI config into a 56-byte report.
/// `base` = existing raw report to preserve colors/flags (safer).
pub fn build_dpi(cfg: &DpiConfig, base: Option<&[u8]>, profile: u8) -> Result<Vec<u8>, String> {
    for &v in &cfg.dpis {
        if !(DPI_MIN..=DPI_MAX).contains(&v) {
            return Err(format!("DPI {v} out of range (50-30000)"));
        }
    }
    if cfg.dpis.is_empty() || cfg.dpis.len() > MAX_DPI_LEVELS {
        return Err(format!("Need 1-{} DPI levels", MAX_DPI_LEVELS));
    }
    if cfg.active >= cfg.dpis.len() {
        return Err("--active out of range".into());
    }

    let mut u = vec![0u8; 56];
    // Preserve from base if available
    if let Some(b) = base {
        let n = b.len().min(56);
        u[..n].copy_from_slice(&b[..n]);
    } else {
        u[3] = cfg.angle_snapping;
        u[4] = cfg.ripple;
        u[49] = cfg.indication;
        for i in 0..8 {
            let c = cfg.colors.get(i).copied().unwrap_or([255, 0, 0]);
            u[25 + 3 * i] = c[0];
            u[26 + 3 * i] = c[1];
            u[27 + 3 * i] = c[2];
        }
    }

    let count = cfg.dpis.len();
    u[0] = 4;
    u[1] = 56;
    u[2] = profile;
    u[3] = cfg.angle_snapping;
    u[4] = cfg.ripple;
    u[5] = LEVEL_MASKS[count - 1];
    u[6] = 0;
    u[7] = 0;
    for i in 0..8 {
        let v = cfg.dpis.get(i).copied().unwrap_or(0);
        u[8 + i] = (v & 0xFF) as u8;
        u[16 + i] = ((v >> 8) & 0xFF) as u8;
    }
    u[24] = (cfg.active + 1) as u8;
    // Preserve colors from cfg (in case base didn't have them)
    for (i, c) in cfg.colors.iter().enumerate().take(8) {
        u[25 + 3 * i] = c[0];
        u[26 + 3 * i] = c[1];
        u[27 + 3 * i] = c[2];
    }
    let (hi, lo) = checksum(&u, 3, 50);
    u[50] = hi;
    u[51] = lo;
    u[52] = 0; u[53] = 0; u[54] = 0; u[55] = 0;
    Ok(u)
}

// ---------------------------------------------------------------- polling rate (report 6)

/// Map Hz -> device code.
pub fn rate_to_code(hz: u32) -> Option<u8> {
    match hz {
        125 => Some(8),
        250 => Some(4),
        500 => Some(2),
        1000 => Some(1),
        _ => None,
    }
}

pub fn rate_from_code(code: u8) -> Option<u32> {
    match code {
        8 => Some(125),
        4 => Some(250),
        2 => Some(500),
        1 => Some(1000),
        _ => None,
    }
}

pub fn build_rate(hz: u32, profile: u8) -> Result<Vec<u8>, String> {
    let v = rate_to_code(hz).ok_or("Rate must be 125, 250, 500, or 1000")?;
    Ok(vec![6, 9, profile, v, (!v) & 0xFF, 0, 0, 0, 0])
}

pub fn parse_rate(d: &[u8]) -> Option<u32> {
    if d.len() >= 4 && d[0] == 6 {
        rate_from_code(d[3])
    } else {
        None
    }
}

// ---------------------------------------------------------------- lighting (report 5)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LightMode {
    Static = 0,
    Breathing = 1,
    Single = 2,
    Rainbow = 3,
    Marquee = 4,
    Flow = 5,
    Apm = 6,
    Off = 7,
}

impl LightMode {
    pub fn all() -> &'static [(&'static str, LightMode)] {
        &[
            ("Static", LightMode::Static),
            ("Breathing", LightMode::Breathing),
            ("Single Color", LightMode::Single),
            ("Rainbow", LightMode::Rainbow),
            ("Marquee", LightMode::Marquee),
            ("Flow", LightMode::Flow),
            ("APM", LightMode::Apm),
            ("Off", LightMode::Off),
        ]
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(LightMode::Static),
            1 => Some(LightMode::Breathing),
            2 => Some(LightMode::Single),
            3 => Some(LightMode::Rainbow),
            4 => Some(LightMode::Marquee),
            5 => Some(LightMode::Flow),
            6 => Some(LightMode::Apm),
            7 => Some(LightMode::Off),
            _ => None,
        }
    }

    pub fn uses_color(self) -> bool {
        matches!(self, LightMode::Static | LightMode::Breathing)
    }
    pub fn uses_brightness(self) -> bool {
        !matches!(self, LightMode::Off)
    }
    pub fn uses_speed(self) -> bool {
        matches!(
            self,
            LightMode::Breathing | LightMode::Single | LightMode::Rainbow
                | LightMode::Marquee | LightMode::Flow
        )
    }
}

#[derive(Debug, Clone)]
pub struct LightConfig {
    pub mode: LightMode,
    pub color: [u8; 3],
    pub brightness: u8, // 0-15
    pub speed: u8,      // 0-15
    pub sleep1_min: u8,
    pub sleep2_min: u8,
    pub debounce_ms: u8, // keyRespond in ms
}

impl Default for LightConfig {
    fn default() -> Self {
        Self {
            mode: LightMode::Static,
            color: [0, 255, 0],
            brightness: 8,
            speed: 2,
            sleep1_min: 1,
            sleep2_min: 10,
            debounce_ms: 8,
        }
    }
}

/// Decode a 64-byte light report (parse first 13 bytes).
pub fn parse_light(d: &[u8]) -> Result<LightConfig, String> {
    if d.len() < 13 || d[0] != 5 {
        return Err("Bad light report".into());
    }
    let mode = LightMode::from_u8((d[3] >> 4) & 0x0F).unwrap_or(LightMode::Off);
    let sleep2 = ((d[5] >> 4) & 0x0F) | ((d[4] >> 4) & 0x0F);
    Ok(LightConfig {
        mode,
        color: [d[6], d[7], d[8]],
        brightness: d[5] & 0x0F,
        speed: d[4] & 0x0F,
        sleep1_min: d[9],
        sleep2_min: sleep2,
        debounce_ms: d[10] * 2,
    })
}

/// Encode into a 15-byte report. `base` preserves sleep/debounce if None fields.
pub fn build_light(cfg: &LightConfig, profile: u8) -> Vec<u8> {
    let mut c = vec![0u8; 15];
    c[0] = 5;
    c[1] = 15;
    c[2] = profile;
    c[3] = ((cfg.mode as u8) << 4) & 0xF0;
    // sleep2 packing (from official app):
    //   c[4] = (speed & 0x0F) | (0xF0 & sleep2)
    //   c[5] = (brightness & 0x0F) | ((sleep2 & 0x0F) << 4)
    c[4] = (cfg.speed & 0x0F) | (0xF0 & cfg.sleep2_min);
    c[5] = (cfg.brightness & 0x0F) | ((cfg.sleep2_min & 0x0F) << 4);
    c[6] = cfg.color[0];
    c[7] = cfg.color[1];
    c[8] = cfg.color[2];
    c[9] = cfg.sleep1_min;
    c[10] = cfg.debounce_ms / 2;
    let (hi, lo) = checksum(&c, 3, 11);
    c[11] = hi;
    c[12] = lo;
    c[13] = 0;
    c[14] = 0;
    c
}

// ---------------------------------------------------------------- button remap (report 8)

pub const NUM_BUTTONS: usize = 18;

/// Button function types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonFunc(pub u8);

impl ButtonFunc {
    pub const DISABLED: u8 = 1;
    pub const LEFT: u8 = 2;
    pub const RIGHT: u8 = 3;
    pub const MIDDLE: u8 = 4;
    pub const BACK: u8 = 5;
    pub const FORWARD: u8 = 6;
    pub const DOUBLE_CLICK: u8 = 7;
    pub const WHEEL_UP: u8 = 9;
    pub const WHEEL_DOWN: u8 = 10;
    pub const WHEEL_LEFT: u8 = 11;
    pub const WHEEL_RIGHT: u8 = 12;
    pub const DPI_CYCLE: u8 = 13;
    pub const DPI_UP: u8 = 14;
    pub const DPI_DOWN: u8 = 15;
    pub const SNIPER: u8 = 16;
    pub const KEYBOARD: u8 = 17;
    pub const MACRO: u8 = 18;
    // 21-38: media keys
    pub const MEDIA_SELECT: u8 = 21;
    pub const MEDIA_PREV: u8 = 22;
    pub const MEDIA_NEXT: u8 = 23;
    pub const MEDIA_PLAY: u8 = 24;
    pub const MEDIA_STOP: u8 = 25;
    pub const MEDIA_MUTE: u8 = 26;
    pub const VOL_UP: u8 = 27;
    pub const VOL_DOWN: u8 = 28;

    pub fn name(t: u8) -> &'static str {
        match t {
            1 => "Disabled",
            2 => "Left Click",
            3 => "Right Click",
            4 => "Middle Click",
            5 => "Back",
            6 => "Forward",
            7 => "Double Click",
            9 => "Wheel Up",
            10 => "Wheel Down",
            11 => "Wheel Left",
            12 => "Wheel Right",
            13 => "DPI Cycle",
            14 => "DPI +",
            15 => "DPI -",
            16 => "Sniper",
            17 => "Keyboard Key",
            18 => "Macro",
            21 => "Media Player",
            22 => "Prev Track",
            23 => "Next Track",
            24 => "Play/Pause",
            25 => "Stop",
            26 => "Mute",
            27 => "Volume +",
            28 => "Volume -",
            29 => "Calculator",
            30 => "Email",
            _ => "Unknown",
        }
    }
}

/// F5V2 physical button indices and labels.
pub const F5V2_BUTTONS: [(usize, &str); 8] = [
    (0, "Left Click"),
    (1, "Right Click"),
    (2, "Middle Click"),
    (3, "Back"),
    (4, "Forward"),
    (8, "DPI Button"),
    (16, "Wheel Up"),
    (17, "Wheel Down"),
];

#[derive(Debug, Clone)]
pub struct ButtonMap {
    /// 18 slots × (type, code1, code2)
    pub slots: [(u8, u8, u8); NUM_BUTTONS],
}

impl Default for ButtonMap {
    fn default() -> Self {
        let mut slots = [(ButtonFunc::DISABLED, 0, 0); NUM_BUTTONS];
        slots[0] = (ButtonFunc::LEFT, 0, 0);
        slots[1] = (ButtonFunc::RIGHT, 0, 0);
        slots[2] = (ButtonFunc::MIDDLE, 0, 0);
        slots[3] = (ButtonFunc::BACK, 0, 0);
        slots[4] = (ButtonFunc::FORWARD, 0, 0);
        slots[8] = (ButtonFunc::DPI_CYCLE, 0, 0);
        Self { slots }
    }
}

pub fn parse_buttons(d: &[u8]) -> Result<ButtonMap, String> {
    if d.len() < 59 || d[0] != 8 {
        return Err("Bad button report".into());
    }
    let mut slots = [(0u8, 0u8, 0u8); NUM_BUTTONS];
    for i in 0..NUM_BUTTONS {
        slots[i] = (d[3 + 3 * i], d[4 + 3 * i], d[5 + 3 * i]);
    }
    Ok(ButtonMap { slots })
}

pub fn build_buttons(map: &ButtonMap, profile: u8) -> Vec<u8> {
    let mut u = vec![0u8; 59];
    u[0] = 8;
    u[1] = 59;
    u[2] = profile;
    for (i, &(t, c1, c2)) in map.slots.iter().enumerate() {
        u[3 + 3 * i] = t;
        u[4 + 3 * i] = c1;
        u[5 + 3 * i] = c2;
    }
    let (hi, lo) = checksum(&u, 3, 57);
    u[57] = hi;
    u[58] = lo;
    u
}

// ---------------------------------------------------------------- mailbox read helper (for device.rs)

/// Build the mailbox request packet for reading `report_id`.
pub fn mailbox_request(report_id: u8, profile: u8) -> Vec<u8> {
    let (send_len, _) = report_lens(report_id);
    vec![
        160,
        report_id,
        (send_len & 0xFF) as u8,
        ((send_len >> 8) & 0xFF) as u8,
        profile,
        0, 0, 0,
    ]
}

/// Sleep durations mirroring the proven Python tool.
pub fn mailbox_delay() -> Duration {
    Duration::from_millis(500)
}
pub fn retry_delay() -> Duration {
    Duration::from_millis(300)
}

// ---------------------------------------------------------------- Macro (report 9)

/// Mouse button codes used inside macros (not the same as button remap types).
pub const MACRO_MOUSE_CODES: [(u8, &'static str); 5] = [
    (241, "Left Click"),
    (243, "Right Click"),
    (242, "Middle Click"),
    (244, "Back"),
    (245, "Forward"),
];

#[derive(Debug, Clone)]
pub struct MacroEvent {
    /// Delay before this event, in milliseconds (10–12700).
    pub delay_ms: u16,
    /// HID keycode, or 241–245 for mouse buttons.
    pub keycode: u8,
    /// true = key/button release, false = press.
    pub is_up: bool,
}

#[derive(Debug, Clone)]
pub struct Macro {
    pub name: String,
    /// 0 = play once, 1 = loop while held, 2 = toggle loop.
    pub loop_type: u8,
    pub events: Vec<MacroEvent>,
}

impl Default for Macro {
    fn default() -> Self {
        Self { name: String::new(), loop_type: 0, events: Vec::new() }
    }
}

/// Encode one macro event to 2 bytes.
/// Returns 2 or 4 bytes (long delays need an extension pair).
fn encode_event(ev: &MacroEvent) -> Vec<u8> {
    let mut out = Vec::new();
    let mut delay = ev.delay_ms.max(10);
    // Long delay extension: [s, 3] where s = delay/200, then remainder.
    if delay > 1270 {
        let s = (delay / 200).min(255) as u8;
        out.push(s);
        out.push(3);
        delay -= s as u16 * 200;
    }
    let d = (delay / 10).min(127) as u8;
    out.push(if ev.is_up { d | 0x80 } else { d & 0x7F });
    out.push(ev.keycode);
    out
}

/// Build the 128-byte macro buffer (see PROTOCOL.md).
pub fn build_macro_buffer(m: &Macro) -> Vec<u8> {
    let mut a = vec![0u8; 128];
    a[0] = m.loop_type;
    // bytes 1-3: RGB indicator (leave 0)
    // byte 4: repeat count (leave 0 = default)
    // bytes 5-23: reserved
    let mut pos = 26;
    let mut count = 0u16;
    for ev in m.events.iter().take(50) {
        let enc = encode_event(ev);
        if pos + enc.len() > 126 { break; }
        for b in enc { a[pos] = b; pos += 1; }
        // Count logical events (not extension pairs).
        count += 1;
    }
    a[24] = ((count >> 8) & 0xFF) as u8;
    a[25] = (count & 0xFF) as u8;
    let (hi, lo) = checksum(&a, 0, 126);
    a[126] = hi;
    a[127] = lo;
    a
}

/// Build the 3 report-9 packets for uploading macro `idx` (0-based).
/// Caller must sleep 666ms between sends.
pub fn build_macro_packets(m: &Macro, idx: u8) -> [Vec<u8>; 3] {
    let a = build_macro_buffer(m);
    let mut p0 = vec![9, 64, idx + 1, 0];
    p0.extend_from_slice(&a[0..59]);
    p0.resize(64, 0);
    let mut p1 = vec![9, 64, idx + 1, 1];
    p1.extend_from_slice(&a[60..119]);
    p1.resize(64, 0);
    let mut p2 = vec![9, 12, idx + 1, 2];
    p2.extend_from_slice(&a[120..128]);
    p2.resize(64, 0);
    [p0, p1, p2]
}

/// Auto-clicker preset: left click every `interval_ms`, loop while held.
pub fn autoclicker_preset(interval_ms: u16) -> Macro {
    Macro {
        name: "Auto Clicker".to_string(),
        loop_type: 1,
        events: vec![
            MacroEvent { delay_ms: 10, keycode: 241, is_up: false },
            MacroEvent { delay_ms: interval_ms, keycode: 241, is_up: true },
        ],
    }
}
