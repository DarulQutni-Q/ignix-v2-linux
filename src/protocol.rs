//! Pure HID protocol for the Vortex Ignix V2 (Xenta F5V2, deviceId 174).
//! No I/O here; every function is unit-testable. Reverse-engineered from
//! BK-MouseHub `page-1ed75bcfb886c7bb.js` (see IGNIX_V2_RE_REPORT.md).

pub type Res<T> = Result<T, String>;

pub const VID: u16 = 0x1D57;
pub const PID_WIRED: u16 = 0x22AE;
pub const PID_DONGLE: u16 = 0xFA60;
pub const CONFIG_USAGE_PAGE: u16 = 0x0B; // config collection (feature reports)
pub const CONFIG_INTERFACE: u8 = 2;

pub const MIN_DPI: u16 = 50;
pub const MAX_DPI: u16 = 30000;
pub const DPI_STEP: u16 = 50;
pub const MAX_STAGES: usize = 6;

pub const R_DPI: u8 = 4;
pub const R_LIGHT: u8 = 5;
pub const R_RATE: u8 = 6;
pub const R_BUTTONS: u8 = 8;
pub const R_MACRO: u8 = 9;
pub const R_FIRMWARE: u8 = 11;
pub const R_PROFILE: u8 = 12;
pub const R_MAILBOX: u8 = 160;

pub const DPI_SEND: usize = 56;
pub const LIGHT_SEND: usize = 15;
pub const RATE_SEND: usize = 9;
pub const BUTTONS_SEND: usize = 59;
pub const MACRO_BUF: usize = 128;
pub const MAILBOX_SEND: usize = 8;

pub const DPI_READ: usize = 64;
pub const LIGHT_READ: usize = 64;
pub const RATE_READ: usize = 64;
pub const BUTTONS_READ: usize = 64;
pub const FIRMWARE_READ: usize = 6;
pub const FIRMWARE_READ_ALT: usize = 8;
pub const PROFILE_READ: usize = 6;
pub const PROFILE_READ_ALT: usize = 9;

pub const MAX_BUTTON_SLOTS: usize = 18;
pub const MAX_MACRO_SLOTS: usize = 32;
pub const MAX_MACRO_EVENTS: usize = 50;
pub const MAX_LIGHT_MODES: u8 = 7; // 0..7

pub const MOUSE_EVENT_CODES: [u8; 5] = [241, 243, 242, 244, 245]; // L, R, M, Back, Fwd

// Button function types (report 8)
pub const FN_DISABLED: u8 = 1;
pub const FN_BUTTON_LEFT: u8 = 2;
pub const FN_BUTTON_RIGHT: u8 = 3;
pub const FN_BUTTON_MIDDLE: u8 = 4;
pub const FN_BUTTON_BACK: u8 = 5;
pub const FN_BUTTON_FORWARD: u8 = 6;
pub const FN_DOUBLE_CLICK: u8 = 7;
pub const FN_WHEEL_UP: u8 = 9;
pub const FN_WHEEL_DOWN: u8 = 10;
pub const FN_WHEEL_LEFT: u8 = 11;
pub const FN_WHEEL_RIGHT: u8 = 12;
pub const FN_DPI_CYCLE: u8 = 13;
pub const FN_DPI_UP: u8 = 14;
pub const FN_DPI_DOWN: u8 = 15;
pub const FN_DPI_SHIFT: u8 = 16; // sniper
pub const FN_KEYBOARD: u8 = 17; // code1 = modifier mask, code2 = HID usage
pub const FN_MACRO: u8 = 18; // code2 = macro slot 1..=32
pub const FN_MEDIA_BASE: u8 = 21; // 21..=28 media keys
pub const FN_CALCULATOR: u8 = 29;
pub const FN_EMAIL: u8 = 30;

/// 16-bit big-endian checksum of the given bytes.
pub fn checksum(bytes: &[u8]) -> [u8; 2] {
    let sum: u32 = bytes.iter().map(|&b| b as u32).sum();
    [(sum >> 8) as u8 & 0xFF, sum as u8 & 0xFF]
}

/// Mailbox request: `[160, targetId, lenLo, lenHi, profile, 0, 0, 0]`.
pub fn build_mailbox(target: u8, send_len: u16, profile: u8) -> [u8; MAILBOX_SEND] {
    let mut m = [0u8; MAILBOX_SEND];
    m[0] = R_MAILBOX;
    m[1] = target;
    m[2] = send_len as u8 & 0xFF;
    m[3] = (send_len >> 8) as u8 & 0xFF;
    m[4] = profile;
    m
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DpiStage {
    pub dpi: u16,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct DpiConfig {
    pub stages: Vec<DpiStage>,
    pub active: usize, // 0-based
    pub angle_snap: bool,
    pub ripple: bool,
    pub indication: u8, // 1|2|3, default 2
}

impl Default for DpiConfig {
    fn default() -> Self {
        Self {
            stages: [
                (1200, [255, 0, 0]),
                (2400, [0, 255, 0]),
                (3600, [0, 0, 255]),
                (4800, [255, 255, 0]),
                (8000, [0, 255, 255]),
                (30000, [255, 0, 255]),
            ]
            .into_iter()
            .map(|(dpi, color)| DpiStage { dpi, color })
            .collect(),
            active: 0,
            angle_snap: false,
            ripple: false,
            indication: 2,
        }
    }
}

fn validate_stage(stage: &DpiStage) -> Res<()> {
    if stage.dpi < MIN_DPI || stage.dpi > MAX_DPI {
        return Err(format!("DPI {} outside {}..={}", stage.dpi, MIN_DPI, MAX_DPI));
    }
    if stage.dpi % DPI_STEP != 0 {
        return Err(format!("DPI {} not a multiple of {}", stage.dpi, DPI_STEP));
    }
    Ok(())
}

/// Report 4 (DPI), 56 bytes. F5V2 splits low bytes at [8..16] and high bytes
/// at [16..24]; active is 1-based at byte 24; checksum over bytes 3..=49.
pub fn build_dpi(profile: u8, cfg: &DpiConfig) -> Res<[u8; DPI_SEND]> {
    if cfg.stages.is_empty() || cfg.stages.len() > MAX_STAGES {
        return Err(format!("{}..=6 DPI stages required", MAX_STAGES));
    }
    for s in &cfg.stages {
        validate_stage(s)?;
    }
    let mut b = [0u8; DPI_SEND];
    b[0] = R_DPI;
    b[1] = DPI_SEND as u8;
    b[2] = profile;
    b[3] = cfg.angle_snap as u8;
    b[4] = cfg.ripple as u8;
    b[5] = [1u8, 3, 7, 15, 31, 63, 127, 255][cfg.stages.len() - 1];
    for (i, s) in cfg.stages.iter().enumerate() {
        b[8 + i] = s.dpi as u8 & 0xFF;
        b[16 + i] = (s.dpi >> 8) as u8 & 0xFF;
        b[25 + 3 * i..=27 + 3 * i].copy_from_slice(&s.color);
    }
    b[24] = cfg.active.clamp(0, cfg.stages.len() - 1) as u8 + 1;
    b[49] = if (1..=3).contains(&cfg.indication) {
        cfg.indication
    } else {
        2
    };
    let [hi, lo] = checksum(&b[3..50]);
    b[50] = hi;
    b[51] = lo;
    Ok(b)
}

/// Parse Report 4; expects first 52 bytes of a 64-byte read.
pub fn parse_dpi(data: &[u8]) -> Res<DpiConfig> {
    if data.len() < 52 {
        return Err(format!("DPI read short ({} bytes)", data.len()));
    }
    if data[0] != R_DPI {
        return Err(format!("DPI read: report id {} != {}", data[0], R_DPI));
    }
    let count = data[5].count_ones() as usize;
    if count == 0 || count > MAX_STAGES {
        return Err(format!("DPI stage count {} invalid", count));
    }
    let mut stages = Vec::with_capacity(count);
    for i in 0..count {
        stages.push(DpiStage {
            dpi: ((data[16 + i] as u16) << 8) | data[8 + i] as u16,
            color: [data[25 + 3 * i], data[26 + 3 * i], data[27 + 3 * i]],
        });
    }
    Ok(DpiConfig {
        stages,
        active: (data[24].saturating_sub(1) as usize).min(count - 1),
        angle_snap: data[3] != 0,
        ripple: data[4] != 0,
        indication: if data[49] == 1 || data[49] == 2 || data[49] == 3 {
            data[49]
        } else {
            2
        },
    })
}

/// Compare only fields within the advertised stage count (device zeroes the rest).
pub fn dpi_equal(a: &DpiConfig, b: &DpiConfig) -> bool {
    a.stages == b.stages
        && a.active.min(a.stages.len().saturating_sub(1)) == b.active.min(b.stages.len().saturating_sub(1))
        && a.angle_snap == b.angle_snap
        && a.ripple == b.ripple
        && a.indication == b.indication
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightConfig {
    pub mode: u8, // 0=Static 1=Breathing 2=SingleColor 3=Rainbow 4=Marquee 5=Flow 6=APM 7=Off
    pub color: [u8; 3],
    pub brightness: u8, // 4-bit
    pub speed: u8,      // 4-bit
    pub sleep1_min: u8,
    pub sleep2_min: u16,
    pub debounce_ms: u16, // stored as ms/2 on device, default 8ms
}

impl Default for LightConfig {
    fn default() -> Self {
        Self {
            mode: 0,
            color: [255, 0, 0],
            brightness: 8,
            speed: 2,
            sleep1_min: 1,
            sleep2_min: 10,
            debounce_ms: 8,
        }
    }
}

/// Report 5 (Lighting/Debounce/Sleep), 15 bytes.
pub fn build_light(profile: u8, cfg: &LightConfig) -> Res<[u8; LIGHT_SEND]> {
    if cfg.mode > MAX_LIGHT_MODES {
        return Err(format!("Light mode {} > {}", cfg.mode, MAX_LIGHT_MODES));
    }
    let sleep2 = cfg.sleep2_min.min(255);
    let mut b = [0u8; LIGHT_SEND];
    b[0] = R_LIGHT;
    b[1] = LIGHT_SEND as u8;
    b[2] = profile;
    b[3] = (cfg.mode << 4) & 0xF0;
    b[4] = (cfg.speed & 0x0F) | ((sleep2 / 16) as u8 * 16) & 0xF0;
    b[5] = (cfg.brightness & 0x0F) | ((sleep2 % 16) as u8) << 4;
    b[6..=8].copy_from_slice(&cfg.color);
    b[9] = cfg.sleep1_min;
    b[10] = (cfg.debounce_ms / 2) as u8;
    let [hi, lo] = checksum(&b[3..11]);
    b[11] = hi;
    b[12] = lo;
    Ok(b)
}

/// Parse Report 5; expects first 13 bytes.
pub fn parse_light(data: &[u8]) -> Res<LightConfig> {
    if data.len() < 13 {
        return Err(format!("Light read short ({} bytes)", data.len()));
    }
    if data[0] != R_LIGHT {
        return Err(format!("Light read: report id {} != {}", data[0], R_LIGHT));
    }
    // Official decode: sleep2 = ((hi nibble of b4) << 4) | (hi nibble of b5). The
    // `<< 4` is load-bearing (regression: sleep2 >= 16 broke without it).
    let sleep2 = (((data[4] >> 4) & 0x0F) as u16) << 4 | ((data[5] >> 4) & 0x0F) as u16;
    Ok(LightConfig {
        mode: (data[3] >> 4) & 0x0F,
        speed: data[4] & 0x0F,
        brightness: data[5] & 0x0F,
        color: [data[6], data[7], data[8]],
        sleep1_min: data[9],
        sleep2_min: sleep2,
        debounce_ms: data[10] as u16 * 2,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct RateConfig(pub u8); // 8=125Hz, 4=250Hz, 2=500Hz, 1=1000Hz

/// Report 6 (Polling rate), 9 bytes, `[4] = !code`.
pub fn build_rate(profile: u8, code: u8) -> Res<[u8; RATE_SEND]> {
    if !matches!(code, 8 | 4 | 2 | 1) {
        return Err(format!("Polling code {} invalid", code));
    }
    let mut b = [0u8; RATE_SEND];
    b[0] = R_RATE;
    b[1] = RATE_SEND as u8;
    b[2] = profile;
    b[3] = code;
    b[4] = !code;
    let [hi, lo] = checksum(&b[3..5]);
    b[5] = hi;
    b[6] = lo;
    Ok(b)
}

/// Parse Report 6; expects first 9 bytes.
pub fn parse_rate(data: &[u8]) -> Res<RateConfig> {
    if data.len() < 9 {
        return Err(format!("Rate read short ({} bytes)", data.len()));
    }
    if data[0] != R_RATE {
        return Err(format!("Rate read: report id {} != {}", data[0], R_RATE));
    }
    let code = data[3];
    if !matches!(code, 8 | 4 | 2 | 1) {
        return Err(format!("Polling code {} invalid", code));
    }
    Ok(RateConfig(code))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BtnFunc {
    pub ty: u8,
    pub code1: u8,
    pub code2: u8,
}

impl BtnFunc {
    pub fn disabled() -> Self {
        Self { ty: FN_DISABLED, code1: 0, code2: 0 }
    }
}

/// F5V2 factory defaults (slots 0..4 physical, 8 = DPI button, 16/17 = wheel).
pub fn default_buttons() -> [BtnFunc; MAX_BUTTON_SLOTS] {
    let mut slots = [BtnFunc::disabled(); MAX_BUTTON_SLOTS];
    for (i, ty) in [
        FN_BUTTON_LEFT,
        FN_BUTTON_RIGHT,
        FN_BUTTON_MIDDLE,
        FN_BUTTON_BACK,
        FN_BUTTON_FORWARD,
    ]
    .into_iter()
    .enumerate()
    {
        slots[i] = BtnFunc { ty, code1: 0, code2: 0 };
    }
    slots[8] = BtnFunc { ty: FN_DPI_CYCLE, code1: 0, code2: 0 };
    slots[16] = BtnFunc { ty: FN_WHEEL_UP, code1: 0, code2: 0 };
    slots[17] = BtnFunc { ty: FN_WHEEL_DOWN, code1: 0, code2: 0 };
    slots
}

/// Report 8 (Button remap), 59 bytes = 18 slots x 3 bytes + checksum(3..=56).
pub fn build_buttons(profile: u8, slots: &[BtnFunc; MAX_BUTTON_SLOTS]) -> Res<[u8; BUTTONS_SEND]> {
    let mut b = [0u8; BUTTONS_SEND];
    b[0] = R_BUTTONS;
    b[1] = BUTTONS_SEND as u8;
    b[2] = profile;
    for (i, s) in slots.iter().enumerate() {
        b[3 + 3 * i] = s.ty;
        b[4 + 3 * i] = s.code1;
        b[5 + 3 * i] = s.code2;
    }
    let [hi, lo] = checksum(&b[3..57]);
    b[57] = hi;
    b[58] = lo;
    Ok(b)
}

/// Parse Report 8; expects first 57 bytes.
pub fn parse_buttons(data: &[u8]) -> Res<Vec<BtnFunc>> {
    if data.len() < 57 {
        return Err(format!("Buttons read short ({} bytes)", data.len()));
    }
    if data[0] != R_BUTTONS {
        return Err(format!("Buttons read: report id {} != {}", data[0], R_BUTTONS));
    }
    Ok((0..MAX_BUTTON_SLOTS)
        .map(|i| BtnFunc {
            ty: data[3 + 3 * i],
            code1: data[4 + 3 * i],
            code2: data[5 + 3 * i],
        })
        .collect())
}

pub const MACRO_LOOP_TOGGLE: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MacroKind {
    /// Keyboard key down; HID usage id in `code`.
    Press(u8),
    /// Keyboard key up.
    Release(u8),
    /// Mouse button click (index 0..=4); emits down + up pairs.
    Click(u8),
    /// Pure wait, folded onto the next (or last) event's delay.
    Wait,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MacroEvent {
    pub kind: MacroKind,
    /// Per-event delay in ms (min 10). For Wait rows this is the wait duration.
    pub delay_ms: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Macro {
    pub slot: u8,      // 1..=32 as reported in code2
    pub loop_type: u8, // 0 once, 1 while held, 2 toggle
    pub repeat: u8,    // device repeat count, official default 1
    pub rgb: [u8; 3],
    pub events: Vec<MacroEvent>,
}

/// Split a delay into the event byte and the `[s, 3]` extension value.
/// >1270ms => extension of `s*200ms` plus the remainder on the event byte.
/// Mirrors the official algorithm incl. the 52270ms cap.
pub fn split_delay(delay_ms: u32) -> (u32, Option<u8>) {
    if delay_ms > 1270 {
        let capped = delay_ms.min(52270);
        let s = (capped / 200).min(255);
        let mut rem = capped - 200 * s;
        if rem < 1 {
            rem = 1;
        }
        (rem, Some(s as u8))
    } else {
        (delay_ms.max(10), None)
    }
}

/// One encoded event pair: `(delay_byte, keycode)`; 0x80 lives in the delay
/// byte (keyup), keycodes are raw HID usage ids (0xE0 modifiers stay safe).
fn push_event(pairs: &mut Vec<(u8, u8)>, count: &mut usize, up: bool, code: u8, delay_ms: u32) -> Res<()> {
    let (rem, ext) = split_delay(delay_ms);
    let d = ((rem as f64 / 10.0).round().min(127.0) as u8) & 0x7F;
    pairs.push((if up { d | 0x80 } else { d }, code));
    *count += 1;
    if let Some(s) = ext {
        pairs.push((s, 0x03)); // long-delay extension, appended after the pair
        *count += 1;
    }
    if *count > MAX_MACRO_EVENTS {
        return Err(format!("Macro exceeds {} events", MAX_MACRO_EVENTS));
    }
    Ok(())
}

/// Encode macro event rows. `Wait` rows fold their ms into the next event's
/// delay (or the final event's extension when nothing follows). Matching the
/// official minimum delay of 10ms per event pair.
pub fn encode_macro_events(events: &[MacroEvent]) -> Res<Vec<(u8, u8)>> {
    let mut pairs = Vec::new();
    let mut count = 0usize;
    let mut pending = 0u32;
    let mut rows = events.iter().peekable();
    while let Some(row) = rows.next() {
        match row.kind {
            MacroKind::Wait => pending += row.delay_ms,
            MacroKind::Press(code) => {
                push_event(&mut pairs, &mut count, false, code, pending + row.delay_ms)?;
                pending = 0;
            }
            MacroKind::Release(code) => {
                push_event(&mut pairs, &mut count, true, code, pending + row.delay_ms)?;
                pending = 0;
            }
            MacroKind::Click(btn) => {
                if btn as usize >= MOUSE_EVENT_CODES.len() {
                    return Err(format!("Mouse button {} unknown", btn));
                }
                let code = MOUSE_EVENT_CODES[btn as usize];
                push_event(&mut pairs, &mut count, false, code, pending + row.delay_ms)?;
                pending = 0;
                push_event(&mut pairs, &mut count, true, code, 10)?; // short gap between down and up
            }
        }
    }
    if pending > 0 {
        return Err(unsupported_trailing_wait_msg());
    }    Ok(pairs)
}

fn unsupported_trailing_wait_msg() -> String {
    "Trailing delay has no event to attach to; add an event after the delay".into()
}

/// Full 128-byte macro buffer (report 9):
/// b0 = loop type, b1..3 = RGB, b4 = repeat, b24/25 = event count BE,
/// events from b26, checksum = sum(0..126) BE at b126/127.
pub fn build_macro(m: &Macro) -> Res<[u8; MACRO_BUF]> {
    if m.slot == 0 || m.slot as usize > MAX_MACRO_SLOTS {
        return Err(format!("Macro slot {} outside 1..=32", m.slot));
    }
    if m.loop_type > MACRO_LOOP_TOGGLE {
        return Err(format!("Loop type {} invalid", m.loop_type));
    }
    let pairs = encode_macro_events(&m.events)?;
    let cited: usize = pairs.len();
    let mut b = [0u8; MACRO_BUF];
    b[0] = m.loop_type;
    b[1..=3].copy_from_slice(&m.rgb);
    b[4] = m.repeat;
    b[24] = (cited >> 8) as u8;
    b[25] = cited as u8;
    for (i, (d, code)) in pairs.iter().enumerate() {
        b[26 + 2 * i] = *d;
        b[27 + 2 * i] = *code;
    }
    let [hi, lo] = checksum(&b[..126]);
    b[126] = hi;
    b[127] = lo;
    Ok(b)
}

/// Three 64-byte feature-report chunks for one macro (666ms delays between
/// them are the caller's responsibility).
pub fn build_macro_chunks(buffer: &[u8; MACRO_BUF], slot: u8) -> Res<Vec<[u8; 64]>> {
    if slot == 0 || slot as usize > MAX_MACRO_SLOTS {
        return Err(format!("Macro slot {} outside 1..=32", slot));
    }
    let mut chunks = Vec::with_capacity(3);
    for (i, (hdr, data)) in [
        ([9u8, 64u8], 59usize),
        ([9u8, 64u8], 59usize),
        ([9u8, 12u8], 8usize),
    ]
    .into_iter()
    .enumerate()
    {
        let mut c = [0u8; 64];
        c[0] = R_MACRO;
        c[1] = hdr[1];
        c[2] = slot;
        c[3] = i as u8;
        let base = i * 60;
        c[4..4 + data].copy_from_slice(&buffer[base..base + data]);
        chunks.push(c);
    }
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_known_vectors() {
        assert_eq!(checksum(&[]), [0, 0]);
        assert_eq!(checksum(&[3, 4]), [0, 7]);
        assert_eq!(checksum(&[255, 255]), [1, 254]); // 510 = 0x01FE
        assert_eq!(checksum(&[1, 1, 1]), [0, 3]);
    }

    #[test]
    fn mailbox_layout() {
        let m = build_mailbox(R_DPI, DPI_SEND as u16, 3);
        assert_eq!(m, [160, 4, 56, 0, 3, 0, 0, 0]);
        let m = build_mailbox(R_LIGHT, LIGHT_SEND as u16, 1);
        assert_eq!(m, [160, 5, 15, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn dpi_split_bytes_layout() {
        let cfg = DpiConfig {
            stages: vec![
                DpiStage { dpi: 1200, color: [255, 0, 0] },
                DpiStage { dpi: 30000, color: [0, 255, 0] },
            ],
            active: 1,
            angle_snap: true,
            ripple: true,
            indication: 2,
        };
        let b = build_dpi(2, &cfg).unwrap();
        assert_eq!(b[0], 4);
        assert_eq!(b[1], 56);
        assert_eq!(b[2], 2);
        assert_eq!(b[3], 1);
        assert_eq!(b[4], 1);
        assert_eq!(b[5], 3);
        // 1200 = 0x04B0, 30000 = 0x7530: low bytes at 8..16, high at 16..24
        assert_eq!(b[8], (1200 & 0xFF) as u8);
        assert_eq!(b[16], (1200 >> 8) as u8);
        assert_eq!(b[9], (30000 & 0xFF) as u8);
        assert_eq!(b[17], (30000 >> 8) as u8);
        assert_eq!(b[18], 0); // stage 3 not present
        assert_eq!(b[24], 2); // active 1-based
        assert_eq!(&b[25..28], &[255, 0, 0]);
        assert_eq!(&b[28..31], &[0, 255, 0]);
        assert_eq!(b[49], 2);
        let expect: u32 = b[3..50].iter().map(|&x| x as u32).sum();
        assert_eq!(b[50], (expect >> 8) as u8);
        assert_eq!(b[51], (expect & 0xFF) as u8);
        // parse roundtrip
        let back = parse_dpi(&b).unwrap();
        assert_eq!(back.stages, cfg.stages);
        assert_eq!(back.active, 1);
        assert!(dpi_equal(&cfg, &back));
    }

    #[test]
    fn dpi_validation() {
        let mut cfg = DpiConfig { stages: vec![DpiStage { dpi: 50, color: [1, 2, 3] }], ..Default::default() };
        assert!(build_dpi(1, &cfg).is_ok());
        cfg.stages[0].dpi = 49;
        assert!(build_dpi(1, &cfg).is_err());
        cfg.stages[0].dpi = 50;
        assert!(build_dpi(1, &cfg).is_ok());
        cfg.stages[0].dpi = 30000;
        assert!(build_dpi(1, &cfg).is_ok());
        cfg.stages[0].dpi = 30050;
        assert!(build_dpi(1, &cfg).is_err());
        cfg.stages[0].dpi = 29975; // not step 50
        assert!(build_dpi(1, &cfg).is_err());
        cfg.stages.clear();
        assert!(build_dpi(1, &cfg).is_err());
        let mut ok = DpiConfig::default(); // 6 stages
        ok.stages.push(DpiStage { dpi: 50, color: [0, 0, 0] });
        assert!(build_dpi(1, &ok).is_err()); // 7 > 6
        assert_eq!(ok.stages.len(), 7);
    }

    #[test]
    fn light_roundtrip() {
        let cfg = LightConfig { mode: 3, color: [10, 20, 30], brightness: 12, speed: 5, sleep1_min: 2, sleep2_min: 171, debounce_ms: 8 };
        let b = build_light(1, &cfg).unwrap();
        assert_eq!(b[0], 5);
        assert_eq!(b[1], 15);
        assert_eq!(b[2], 1);
        assert_eq!(b[3], 0x30);
        assert_eq!(b[4], 0xA5); // sleep2 hi nibble 0xA in b4, speed 5
        assert_eq!(b[5], 0xBC); // sleep2 lo nibble (0xB) in b5 hi, brightness 12 (0xC) low
        assert_eq!(&b[6..9], &[10, 20, 30]);
        assert_eq!(b[9], 2);
        assert_eq!(b[10], 4);
        let back = parse_light(&b).unwrap();
        assert_eq!(back, cfg);
    }

    /// Regression: missing shift-left 4 corrupts sleep2 >= 16.
    #[test]
    fn sleep2_regressions() {
        for v in [10_u16, 16, 20, 30, 171, 255] {
            let cfg = LightConfig { sleep2_min: v, ..Default::default() };
            let b = build_light(1, &cfg).unwrap();
            let back = parse_light(&b).unwrap();
            assert_eq!(back.sleep2_min, v, "sleep2 {}", v);
        }
    }

    #[test]
    fn rate_layout() {
        for (hz, code) in [(125_u16, 8_u8), (250, 4), (500, 2), (1000, 1)] {
            let b = build_rate(1, code).unwrap();
            assert_eq!(b[0], 6);
            assert_eq!(b[1], 9);
            assert_eq!(b[2], 1);
            assert_eq!(b[3], code);
            assert_eq!(b[4], !code);
            assert_eq!(parse_rate(&b).unwrap(), RateConfig(code), "{}Hz", hz);
        }
        assert!(build_rate(1, 5).is_err());
    }

    #[test]
    fn buttons_layout() {
        let slots = default_buttons();
        let b = build_buttons(3, &slots).unwrap();
        assert_eq!(b[0], 8);
        assert_eq!(b[1], 59);
        assert_eq!(b[2], 3);
        // slot 0 = left = type 2
        assert_eq!(b[3], 2);
        assert_eq!(&b[3 + 8 * 3..3 + 8 * 3 + 3], &[13, 0, 0]); // DPI cycle at slot 8
        assert_eq!(b[3 + 6 * 3], 1); // unused slot disabled
        let expect: u32 = b[3..57].iter().map(|&x| x as u32).sum();
        assert_eq!(b[57], (expect >> 8) as u8);
        assert_eq!(b[58], (expect & 0xFF) as u8);
        assert_eq!(parse_buttons(&b).unwrap(), slots.to_vec());
    }

    #[test]
    fn macro_simple_events() {
        // 100ms keydown + keyup: delay byte = 10
        let rows = [MacroEvent { kind: MacroKind::Press(0x11), delay_ms: 100 }, MacroEvent { kind: MacroKind::Release(0x11), delay_ms: 100 }];
        let pairs = encode_macro_events(&rows).unwrap();
        assert_eq!(pairs, &[(10, 0x11), (10 | 0x80, 0x11)]);
    }

    #[test]
    fn macro_long_delays() {
        // 1280ms: s = 6, remainder 80ms -> byte 8, extension appended AFTER the pair
        let rows = [MacroEvent { kind: MacroKind::Press(0x04), delay_ms: 1280 }];
        assert_eq!(encode_macro_events(&rows).unwrap(), &[(8, 0x04), (6, 3)]);
        // 2000ms: s = 10, remainder 0 -> nudged to 1 -> delay byte 0; keyup has 0x80 flag
        let rows = [MacroEvent { kind: MacroKind::Release(0x04), delay_ms: 2000 }];
        assert_eq!(encode_macro_events(&rows).unwrap(), &[(0x80, 0x04), (10, 3)]);
    }

    #[test]
    fn macro_cap_52270() {
        let rows = [MacroEvent { kind: MacroKind::Press(0x04), delay_ms: 60000 }];
        let pairs = encode_macro_events(&rows).unwrap();
        // capped at 52270 => s = 255 + 127ms remainder byte 12/13
        assert_eq!(pairs[1], (255, 3));
        assert_eq!(split_delay(60000), (1270, Some(255))); // 200*255 + 1270 = 52270
        assert_eq!(split_delay(1270), (1270, None));
        assert_eq!(split_delay(2000), (1, Some(10)));
    }

    #[test]
    fn macro_wait_folding() {
        // Wait 100 + Press delay 40 => 140ms total
        let rows = [
            MacroEvent { kind: MacroKind::Wait, delay_ms: 100 },
            MacroEvent { kind: MacroKind::Press(0x07), delay_ms: 40 },
        ];
        assert_eq!(encode_macro_events(&rows).unwrap(), &[(14, 0x07)]);
        // trailing wait has no event to ride on
        let rows = [MacroEvent { kind: MacroKind::Press(0x07), delay_ms: 10 }, MacroEvent { kind: MacroKind::Wait, delay_ms: 500 }];
        assert!(encode_macro_events(&rows).is_err());
    }

    #[test]
    fn macro_buffer_and_chunks() {
        let m = Macro {
            slot: 7,
            loop_type: 1,
            repeat: 1,
            rgb: [0, 0, 0],
            events: vec![
                MacroEvent { kind: MacroKind::Click(0), delay_ms: 40 },
                MacroEvent { kind: MacroKind::Press(0x1A), delay_ms: 20 },
                MacroEvent { kind: MacroKind::Release(0x1A), delay_ms: 20 },
            ],
        };
        let buf = build_macro(&m).unwrap();
        assert_eq!(buf[0], 1);
        assert_eq!(buf[4], 1);
        // count BE at 24/25: click(2 pairs) + press + release = 4
        assert_eq!(buf[24], 0);
        assert_eq!(buf[25], 4);
        // click = down(241) + up(241); 40ms => byte 4, up gap 10ms => 1 | 0x80
        assert_eq!(&buf[26..34], &[4, 241, 0x80 | 1, 241, 2, 0x1A, 0x80 | 2, 0x1A]);
        let expect: u32 = buf[..126].iter().map(|&x| x as u32).sum();
        assert_eq!(buf[126], (expect >> 8) as u8);
        assert_eq!(buf[127], (expect & 0xFF) as u8);

        let chunks = build_macro_chunks(&buf, 7).unwrap();
        assert_eq!(chunks.len(), 3);
        assert_eq!(&chunks[0][..4], &[9, 64, 7, 0]);
        assert_eq!(chunks[0][4], buf[0]);
        assert_eq!(&chunks[1][..4], &[9, 64, 7, 1]);
        assert_eq!(chunks[1][4], buf[60]);
        assert_eq!(&chunks[2][..4], &[9, 12, 7, 2]);
        assert_eq!(chunks[2][4], buf[120]);
        assert_eq!(chunks[0].len(), 64);
    }

    #[test]
    fn macro_slot_validation() {
        let m = Macro { slot: 0, loop_type: 0, repeat: 1, rgb: [0; 3], events: vec![] };
        assert!(build_macro(&m).is_err());
        let m = Macro { slot: 33, loop_type: 0, repeat: 1, rgb: [0; 3], events: vec![] };
        assert!(build_macro(&m).is_err());
        let m = Macro { slot: 1, loop_type: 3, repeat: 1, rgb: [0; 3], events: vec![] };
        assert!(build_macro(&m).is_err());
        let m = Macro { slot: 1, loop_type: 2, repeat: 1, rgb: [0; 3], events: vec![MacroEvent { kind: MacroKind::Press(0x04), delay_ms: 10 }] };
        assert!(build_macro(&m).is_ok());
    }

    #[test]
    fn macro_event_cap() {
        // 25 clicks = 50 pairs, one more -> error
        let mut events: Vec<MacroEvent> = (0..25).map(|_| MacroEvent { kind: MacroKind::Click(0), delay_ms: 10 }).collect();
        assert!(build_macro(&Macro { slot: 1, loop_type: 0, repeat: 1, rgb: [0; 3], events: events.clone() }).is_ok());
        events.push(MacroEvent { kind: MacroKind::Press(0x04), delay_ms: 10 });
        assert!(encode_macro_events(&events).is_err());
        assert!(build_macro(&Macro { slot: 1, loop_type: 0, repeat: 1, rgb: [0; 3], events }).is_err());
    }
}

