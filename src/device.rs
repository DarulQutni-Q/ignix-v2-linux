//! USB HID device access for the Ignix V2.

use hidapi::{HidApi, HidDevice};
use std::thread;
use std::time::Duration;

use crate::protocol::{self, mailbox_delay, mailbox_request, report_lens};

pub struct Mouse {
    dev: HidDevice,
    profile: u8,
}

impl Mouse {
    /// Open the mouse's config interface (interface 2, usage_page 0x0B).
    pub fn open() -> Result<Self, String> {
        let api = HidApi::new().map_err(|e| format!("HID init failed: {e}"))?;
        let target = api
            .device_list()
            .find(|d| {
                d.vendor_id() == protocol::VID
                    && d.product_id() == protocol::PID
                    && d.interface_number() == 2
            })
            .ok_or_else(|| {
                "Mouse not found. Plug it in via USB cable (wired mode).\n\
                 If the device is plugged in, you may need a udev rule or run as root."
                    .to_string()
            })?;

        let path = target.path().to_owned();
        let dev = api.open_path(&path).map_err(|e| {
            format!(
                "Cannot open device: {e}\n\
                 Try: sudo setfacl -m u:$USER:rw /dev/hidraw*  (temporary)\n\
                 Or install the udev rule (see README)."
            )
        })?;

        Ok(Self {
            dev,
            profile: protocol::PROFILE,
        })
    }

    pub fn set_profile(&mut self, p: u8) {
        self.profile = p.clamp(1, 5);
    }
    pub fn profile(&self) -> u8 {
        self.profile
    }

    fn send_feature(&self, packet: &[u8]) -> Result<(), String> {
        self.dev
            .send_feature_report(packet)
            .map_err(|e| format!("send_feature_report failed: {e}"))?;
        Ok(())
    }

    fn get_feature(&self, report_id: u8, len: usize) -> Result<Vec<u8>, String> {
        let mut buf = vec![0u8; len];
        buf[0] = report_id;
        let n = self
            .dev
            .get_feature_report(&mut buf)
            .map_err(|e| format!("get_feature_report({report_id}) failed: {e}"))?;
        buf.truncate(n.max(len.min(buf.len())));
        // hidapi returns the full buffer; ensure we have `len` bytes
        if buf.len() < len {
            buf.resize(len, 0);
        }
        Ok(buf)
    }

    /// Mailbox read: send request via report 160, wait, read directly.
    /// Retries up to 6× like the official app.
    pub fn read_config(&self, report_id: u8) -> Result<Vec<u8>, String> {
        let (_, read_len) = report_lens(report_id);
        let mut last_err = String::new();
        for _ in 0..6 {
            let req = mailbox_request(report_id, self.profile);
            match self.send_feature(&req) {
                Ok(()) => {
                    thread::sleep(mailbox_delay());
                    match self.get_feature(report_id, read_len) {
                        Ok(d) => {
                            if d.len() >= read_len && d[0] == report_id {
                                return Ok(d);
                            } else {
                                last_err = format!(
                                    "bad response (len={}, id={})",
                                    d.len(),
                                    d.get(0).copied().unwrap_or(0)
                                );
                            }
                        }
                        Err(e) => last_err = e,
                    }
                }
                Err(e) => last_err = e,
            }
            thread::sleep(protocol::retry_delay());
        }
        Err(format!(
            "Failed to read report {report_id}: {last_err}"
        ))
    }

    /// Direct feature-report write (no mailbox).
    pub fn write_config(&self, packet: &[u8]) -> Result<(), String> {
        // DPI writes get retries per the official app's behavior.
        let mut last_err = String::new();
        for _ in 0..3 {
            match self.send_feature(packet) {
                Ok(()) => return Ok(()),
                Err(e) => last_err = e,
            }
            thread::sleep(Duration::from_millis(180));
        }
        Err(format!("Write failed: {last_err}"))
    }

    // ------------------------------------------------------------ typed access

    pub fn read_dpi(&self) -> Result<protocol::DpiConfig, String> {
        let d = self.read_config(4)?;
        protocol::parse_dpi(&d)
    }

    pub fn read_dpi_raw(&self) -> Result<Vec<u8>, String> {
        self.read_config(4)
    }

    pub fn write_dpi(
        &self,
        cfg: &protocol::DpiConfig,
        base: Option<&[u8]>,
    ) -> Result<(), String> {
        let pkt = protocol::build_dpi(cfg, base, self.profile)?;
        self.write_config(&pkt)?;
        thread::sleep(Duration::from_millis(400));
        // Verify by reading back.
        let back = self.read_dpi()?;
        if back.dpis != cfg.dpis || back.active != cfg.active {
            return Err(format!(
                "Verify failed: expected {:?}, got {:?}",
                cfg.dpis, back.dpis
            ));
        }
        Ok(())
    }

    pub fn read_rate(&self) -> Result<u32, String> {
        let d = self.read_config(6)?;
        protocol::parse_rate(&d).ok_or_else(|| "Bad rate report".to_string())
    }

    pub fn write_rate(&self, hz: u32) -> Result<(), String> {
        let pkt = protocol::build_rate(hz, self.profile)?;
        self.write_config(&pkt)?;
        thread::sleep(Duration::from_millis(400));
        let back = self.read_rate()?;
        if back != hz {
            return Err(format!("Verify failed: expected {hz}, got {back}"));
        }
        Ok(())
    }

    pub fn read_light(&self) -> Result<protocol::LightConfig, String> {
        let d = self.read_config(5)?;
        protocol::parse_light(&d)
    }

    pub fn write_light(&self, cfg: &protocol::LightConfig) -> Result<(), String> {
        let pkt = protocol::build_light(cfg, self.profile);
        self.write_config(&pkt)?;
        thread::sleep(Duration::from_millis(400));
        let back = self.read_light()?;
        if back.mode != cfg.mode {
            return Err(format!(
                "Verify failed: expected {:?}, got {:?}",
                cfg.mode, back.mode
            ));
        }
        Ok(())
    }

    pub fn read_buttons(&self) -> Result<protocol::ButtonMap, String> {
        let d = self.read_config(8)?;
        protocol::parse_buttons(&d)
    }

    pub fn write_buttons(&self, map: &protocol::ButtonMap) -> Result<(), String> {
        let pkt = protocol::build_buttons(map, self.profile);
        self.write_config(&pkt)?;
        thread::sleep(Duration::from_millis(400));
        Ok(())
    }

    pub fn read_firmware(&self) -> Result<String, String> {
        // Report 11 does NOT use the mailbox polling; direct pattern.
        let req = mailbox_request(11, self.profile);
        self.send_feature(&req)?;
        thread::sleep(Duration::from_millis(500));
        let d = self.get_feature(11, 6)?;
        if d.len() >= 4 && d[0] == 11 {
            Ok(format!("{}.{:02}", d[2], d[3]))
        } else {
            Err("Bad version report".into())
        }
    }

    pub fn read_profile_id(&self) -> Result<u8, String> {
        let d = self.read_config(12)?;
        if d.len() >= 4 && d[0] == 12 {
            Ok(d[3])
        } else {
            Err("Bad profile report".into())
        }
    }
}
