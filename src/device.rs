//! HID device I/O for the Ignix V2 (Xenta F5V2). Wraps hidapi; all protocol
//! math lives in `protocol.rs`. Feature reports carry the report id in byte 0
//! (same wire layout as the official WebHID app), so every buffer here
//! starts with the id.

use crate::protocol::*;
use hidapi::{HidApi, HidDevice};
use std::thread;
use std::time::Duration;

const READY_POLL_MS: u64 = 160;
const READY_POLLS: u64 = 6; // official polls GET(160) up to 6x160ms
const READY_FALLBACK_MS: u64 = 500; // fixed budget when the device never flags ready
const POST_READY_MS: u64 = 120;
const LENGTH_FALLBACK_MS: u64 = 400; // widens short waits to the ~500ms budget
const WRITE_SETTLE_MS: u64 = 60;
const WRITE_RETRY_MS: u64 = 180;
const MACRO_CHUNK_MS: u64 = 666; // official setMacroData delay between chunks
const SET_STALL_RETRY_MS: u64 = 300; // wired firmware rejects rapid mailbox SETs (EPIPE=busy)
const SET_STALL_RETRY_ATTEMPTS: usize = 6;

fn err_str(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn wired_msg() -> String {
    "USB-cable (wired) mode: the config interface stays silent on this firmware (fw reads back 0.00). Plug the 2.4G dongle to configure the mouse.".to_string()
}

#[derive(Clone, Debug)]
pub struct DeviceState {
    pub profile: u8,
    pub firmware: String,
    pub dpi: DpiConfig,
    pub light: LightConfig,
    pub rate: RateConfig,
    pub buttons: [BtnFunc; MAX_BUTTON_SLOTS],
}

pub struct Mouse {
    #[allow(dead_code)]
    api: HidApi,
    dev: HidDevice,
    /// Target profile (report byte 2) for every subsequent read/write.
    pub active_profile: u8,
}

impl Mouse {
    /// Find and open the mouse: prefer the config collection (usage page 0x0B),
    /// then interface 2, then the first 1D57 match (0x22AE wired / 0xFA60 dongle).
    pub fn open() -> Result<Self, String> {
        let api = HidApi::new().map_err(err_str)?;
        let matches: Vec<_> = api
            .device_list()
            .filter(|d| d.vendor_id() == VID && (d.product_id() == PID_WIRED || d.product_id() == PID_DONGLE))
            .collect();
        let info = matches
            .iter()
            .find(|d| d.usage_page() == CONFIG_USAGE_PAGE)
            .or_else(|| matches.iter().find(|d| d.interface_number() == CONFIG_INTERFACE as i32))
            .or_else(|| matches.first())
            .copied()
            .ok_or_else(|| {
                "No Ignix V2 / F5V2 device found (VID 1D57, PID 22AE or FA60)".to_string()
            })?;
        let dev = info.open_device(&api).map_err(|e| {
            let s = err_str(e);
            if s.contains("denied") || s.contains("Permission") {
                format!("{s} — fix with: sudo tee /etc/udev/rules.d/99-ignix.rules")
            } else {
                s
            }
        })?;
        let mouse = Self { api, dev, active_profile: 1 };
        // Wired firmware (22AE) keeps its config silent: feature reads come
        // back zeroed and some SETs stall with EPIPE. Fail early with guidance
        // instead of dying on every write later.
        let fw = mouse.read_firmware().map_err(|e| {
            if e.contains("Broken pipe") || e.contains("No such device") {
                wired_msg()
            } else {
                e
            }
        })?;
        if fw == "0.00" || fw.is_empty() {
            return Err(wired_msg());
        }
        Ok(mouse)
    }

    /// Switch which profile subsequent reads/writes address (report byte 2).
    /// There is no separate profile-switch report on this firmware.
    pub fn set_profile(&mut self, profile: u8) -> Res<()> {
        if !(1..=5).contains(&profile) {
            return Err(format!("Profile {} outside 1..=5", profile));
        }
        self.active_profile = profile;
        Ok(())
    }

    fn send_feature(&self, report: &[u8]) -> Res<()> {
        match self.dev.send_feature_report(report) {
            Ok(()) => Ok(()),
            Err(e) => {
                // Wired firmware (22AE) stalls SET_REPORT for a while after a
                // mailbox exchange (EPIPE); the dongle never stalls. Retry on
                // that error only, generous spacing.
                let s = err_str(e);
                if !s.contains("Broken pipe") {
                    return Err(s);
                }
                for i in 1..=SET_STALL_RETRY_ATTEMPTS {
                    thread::sleep(Duration::from_millis(SET_STALL_RETRY_MS));
                    match self.dev.send_feature_report(report) {
                        Ok(()) => return Ok(()),
                        Err(e2) => {
                            let s2 = err_str(e2);
                            if !s2.contains("Broken pipe") || i == SET_STALL_RETRY_ATTEMPTS {
                                return Err(s2);
                            }
                        }
                    }
                }
                Err(s)
            }
        }
    }

    /// `buf[0]` is the report id (hidapi contract); returns the response
    /// including the id byte, like the official JS buffers.
    fn get_feature(&self, id: u8, len: usize) -> Res<Vec<u8>> {
        let mut buf = vec![0u8; len];
        buf[0] = id;
        let n = self.dev.get_feature_report(&mut buf).map_err(err_str)?;
        buf.truncate(n.max(1));
        Ok(buf)
    }

    /// Mailbox handshake + target read. Polls GET(160) for the ready flag
    /// (official 6x160ms); if the device never signals ready, waits the fixed
    /// 500ms budget and reads anyway. `alt_len` retries a longer read when the
    /// first length stalls (firmware/profile reports).
    fn read_report(&self, target: u8, send_len: u16, read_len: usize, alt_len: Option<usize>) -> Res<Vec<u8>> {
        self.send_feature(&build_mailbox(target, send_len, self.active_profile))?;
        let mut ready = false;
        for _ in 0..READY_POLLS {
            thread::sleep(Duration::from_millis(READY_POLL_MS));
            // Ready-poll errors are swallowed, like the official app.
            if let Ok(buf) = self.get_feature(R_MAILBOX, MAILBOX_SEND) {
                if buf.len() >= 2 && buf[0] == R_MAILBOX && buf[1] == 1 {
                    ready = true;
                    break;
                }
            }
        }
        if !ready {
            thread::sleep(Duration::from_millis(READY_FALLBACK_MS));
        }
        thread::sleep(Duration::from_millis(POST_READY_MS));
        let buf = match self.get_feature(target, read_len) {
            Ok(b) => b,
            Err(e) => match alt_len {
                None => return Err(e),
                Some(alt) => {
                    thread::sleep(Duration::from_millis(LENGTH_FALLBACK_MS));
                    self.get_feature(target, alt)?
                }
            },
        };
        if buf.first() != Some(&target) {
            return Err(format!("Report {}: device returned {:?}", target, buf.first()));
        }
        Ok(buf)
    }

    /// Report 11: no ready-poll in the official app (mailbox, ~100ms, read);
    /// we keep that and fall back to 8 bytes if the 6-byte read stalls.
    pub fn read_firmware(&self) -> Res<String> {
        // Official sends the firmware mailbox with profile slot zeroed.
        self.send_feature(&build_mailbox(R_FIRMWARE, 8, 0))?;
        thread::sleep(Duration::from_millis(100));
        let buf = self
            .get_feature(R_FIRMWARE, FIRMWARE_READ)
            .or_else(|_| {
                thread::sleep(Duration::from_millis(LENGTH_FALLBACK_MS));
                self.get_feature(R_FIRMWARE, FIRMWARE_READ_ALT)
            })
            .map_err(err_str)?;
        if buf.first() != Some(&R_FIRMWARE) || buf.len() < 4 {
            return Err(format!("Firmware: unexpected response ({} bytes)", buf.len()));
        }
        Ok(format!("{}.{:02}", buf[2], buf[3]))
    }

    pub fn read_profile(&mut self) -> Res<u8> {
        let buf = self.read_report(R_PROFILE, 9, PROFILE_READ, Some(PROFILE_READ_ALT))?;
        let profile = buf.get(3).copied().ok_or_else(|| "Profile byte missing".to_string())?;
        if !(1..=5).contains(&profile) {
            return Err(format!("Device reports profile {} outside 1..=5", profile));
        }
        self.active_profile = profile;
        Ok(profile)
    }

    pub fn read_dpi(&self) -> Res<DpiConfig> {
        parse_dpi(&self.read_report(R_DPI, DPI_SEND as u16, DPI_READ, None)?)
    }

    pub fn read_light(&self) -> Res<LightConfig> {
        parse_light(&self.read_report(R_LIGHT, LIGHT_SEND as u16, LIGHT_READ, None)?)
    }

    pub fn read_rate(&self) -> Res<RateConfig> {
        parse_rate(&self.read_report(R_RATE, RATE_SEND as u16, RATE_READ, None)?)
    }

    pub fn read_buttons(&self) -> Res<Vec<BtnFunc>> {
        parse_buttons(&self.read_report(R_BUTTONS, BUTTONS_SEND as u16, BUTTONS_READ, None)?)
    }

    /// Read everything the UI needs (connect / refresh). Order mirrors the
    /// official startup: profile id first (fixes report byte 2), then the rest.
    pub fn read_all(&mut self) -> Res<DeviceState> {
        let profile = self.read_profile()?;
        let firmware = self.read_firmware()?;
        let dpi = self.read_dpi().map_err(|e| format!("DPI: {}", e))?;
        let light = self.read_light().map_err(|e| format!("Lighting: {}", e))?;
        let rate = self.read_rate().map_err(|e| format!("Polling rate: {}", e))?;
        let buttons = self
            .read_buttons()?
            .try_into()
            .map_err(|v: Vec<BtnFunc>| format!("Buttons: got {} slots", v.len()))?;
        Ok(DeviceState { profile, firmware, dpi, light, rate, buttons })
    }

    /// Generic write + readback-verify. `attempts` includes the first write;
    /// DPI spec mandates 3 (180ms apart), the official app tolerates 2 for
    /// reports 5/6/8 on first write.
    fn write_verify<T, P, V>(&self, buffer: &[u8], target: u8, send_len: u16, attempts: usize, parse: P, matches: V, want: &T) -> Res<()>
    where
        P: Fn(&[u8]) -> Option<T>,
        V: Fn(&T, &T) -> bool,
    {
        for attempt in 0..attempts {
            if attempt > 0 {
                thread::sleep(Duration::from_millis(WRITE_RETRY_MS));
            }
            self.send_feature(buffer)
                .map_err(|e| format!("Report {}: write failed: {}", target, e))?;
            thread::sleep(Duration::from_millis(WRITE_SETTLE_MS));
            let ok = self
                .read_report(target, send_len, DPI_READ, None)
                .ok()
                .and_then(|d| parse(&d))
                .map_or(false, |p| matches(&p, want));
            if ok {
                return Ok(());
            }
        }
        Err(format!("Report {}: readback verification failed after {} attempts", target, attempts))
    }

    pub fn write_dpi(&self, cfg: &DpiConfig) -> Res<()> {
        let buffer = build_dpi(self.active_profile, cfg)?;
        self.write_verify(&buffer, R_DPI, DPI_SEND as u16, 3, |d| parse_dpi(d).ok(), |p, w| dpi_equal(p, w), cfg)
    }

    pub fn write_light(&self, cfg: &LightConfig) -> Res<()> {
        let buffer = build_light(self.active_profile, cfg)?;
        self.write_verify(&buffer, R_LIGHT, LIGHT_SEND as u16, 2, |d| parse_light(d).ok(), |p, w| p == w, cfg)
    }

    pub fn write_rate(&self, rate: &RateConfig) -> Res<()> {
        let buffer = build_rate(self.active_profile, rate.0)?;
        self.write_verify(&buffer, R_RATE, RATE_SEND as u16, 2, |d| parse_rate(d).ok(), |p, w| p == w, rate)
    }

    pub fn write_buttons(&self, slots: &[BtnFunc; MAX_BUTTON_SLOTS]) -> Res<()> {
        let buffer = build_buttons(self.active_profile, slots)?;
        self.write_verify(
            &buffer,
            R_BUTTONS,
            BUTTONS_SEND as u16,
            2,
            |d| parse_buttons(d).ok(),
            |p, w: &Vec<BtnFunc>| p.to_vec() == *w,
            &slots.to_vec(),
        )
    }

    /// Report 9 is write-only: no readback verification exists. The official
    /// app sends the 3 chunks with 666ms gaps and trusts the transport.
    pub fn write_macro(&self, m: &Macro) -> Res<()> {
        let buffer = build_macro(m)?;
        let chunks = build_macro_chunks(&buffer, m.slot)?;
        for c in &chunks {
            thread::sleep(Duration::from_millis(MACRO_CHUNK_MS));
            self.send_feature(c)
                .map_err(|e| format!("Macro {}: write failed: {}", m.slot, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Physical device required; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_read_all() {
        let mut m = Mouse::open().unwrap();
        let state = m.read_all().unwrap();
        assert!((1..=5).contains(&state.profile));
        assert!(!state.firmware.is_empty() && state.firmware.chars().all(|c| c.is_ascii_digit() || c == '.'));
        assert!(!state.dpi.stages.is_empty() && state.dpi.active < state.dpi.stages.len());
        for s in &state.dpi.stages {
            assert!(s.dpi >= MIN_DPI && s.dpi <= MAX_DPI);
        }
        assert!(matches!(state.rate, RateConfig(8 | 4 | 2 | 1)));
        assert_eq!(state.buttons.len(), MAX_BUTTON_SLOTS);
        println!("profile={} fw={} dpi={:#?} rate={:?} btn0={:?}", state.profile, state.firmware, state.dpi, state.rate, state.buttons[0]);
        println!("light={:#?} buttons={:#?}", state.light, state.buttons);
    }

    /// Writes the exact values just read back: verifies the write path without
    /// changing any setting.
    #[test]
    #[ignore]
    fn live_write_same_values() {
        let m = Mouse::open().unwrap();
        let dpi = m.read_dpi().unwrap();
        m.write_dpi(&dpi).unwrap();
        let light = m.read_light().unwrap();
        m.write_light(&light).unwrap();
        let rate = m.read_rate().unwrap();
        m.write_rate(&rate).unwrap();
        let buttons: [BtnFunc; MAX_BUTTON_SLOTS] = m.read_buttons().unwrap().try_into().unwrap();
        m.write_buttons(&buttons).unwrap();
    }

    /// Writes the click-spam macro (hold = 10 clicks/s) into slot 1.
    /// This is the hardware-verified macro write: fill ONLY slot 1.
    #[test]
    #[ignore]
    fn live_write_macro_autoclick() {
        let m = Mouse::open().unwrap();
        let mac = Macro {
            slot: 1,
            loop_type: 1, // repeat while held
            repeat: 1,
            rgb: [0, 229, 156],
            events: vec![MacroEvent { kind: MacroKind::Click(0), delay_ms: 100 }],
        };
        m.write_macro(&mac).unwrap();
    }

    /// Manual dump to understand hidraw feature-report framing on this firmware.
    #[test]
    #[ignore]
    fn debug_dump() {
        let m = Mouse::open().unwrap();
        let hex = |b: &[u8]| b.iter().map(|x| format!("{:02X}", x)).collect::<Vec<_>>().join(" ");
        println!("PATH {:?}", m.dev.get_device_info().unwrap());
        m.send_feature(&build_mailbox(R_PROFILE, 9, 1)).unwrap();
        for i in 0..4 {
            thread::sleep(Duration::from_millis(160));
            match m.get_feature(R_MAILBOX, 8) {
                Ok(b) => println!("poll{} GET160 len={}: {}", i, b.len(), hex(&b)),
                Err(e) => println!("poll{} GET160 ERR: {}", i, e),
            }
        }
        for len in [6usize, 8, 9, 64] {
            match m.get_feature(R_PROFILE, len) {
                Ok(b) => println!("GET12 len-request={}: len={} {}", len, b.len(), hex(&b)),
                Err(e) => println!("GET12 len-request={} ERR: {}", len, e),
            }
        }
        match m.get_feature(R_DPI, DPI_READ) {
            Ok(b) => println!("GET4 len={} {}", b.len(), hex(&b)),
            Err(e) => println!("GET4 ERR: {}", e),
        }
    }

    /// Delay-vs-response experiment.
    #[test]
    #[ignore]
    fn debug_delay_matrix() {
        let m = Mouse::open().unwrap();
        let hex = |b: &[u8]| b.iter().map(|x| format!("{:02X}", x)).collect::<Vec<_>>().join(" ");
        for (target, send_len) in [(R_PROFILE, 9u16), (R_DPI, 56u16)] {
            m.send_feature(&build_mailbox(target, send_len, 1)).unwrap();
            for delay in [0u64, 60, 120, 280, 500, 1000] {
                thread::sleep(Duration::from_millis(delay));
                let mb = m.get_feature(R_MAILBOX, 8).map(|b| hex(&b)).unwrap_or_else(|e| e);
                let tg = m.get_feature(target, 64).map(|b| hex(&b[..12])).unwrap_or_else(|e| e);
                println!("target={} delay={:>4}  GET160=[{}]  GET{}=[{}]", target, delay, mb, target, tg);
                // reset: resend mailbox before next delay sample
                if delay < 1000 {
                    m.send_feature(&build_mailbox(target, send_len, 1)).unwrap();
                }
            }
        }
    }
}
