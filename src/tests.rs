//! Unit tests for the Ignix V2 protocol.
//! Run with: cargo test

#[cfg(test)]
mod tests {
    use crate::protocol::*;

    // ------------------------------------------------------------ checksum

    #[test]
    fn checksum_known_vector() {
        // From the Python tool: sum of bytes, big-endian.
        let data = vec![0u8, 0, 0, 1, 2, 3, 4, 5];
        let (hi, lo) = checksum(&data, 3, 8);
        // sum(1+2+3+4+5) = 15 = 0x000F
        assert_eq!((hi, lo), (0x00, 0x0F));
    }

    #[test]
    fn checksum_overflow() {
        let data = vec![0xFFu8; 100];
        let (hi, lo) = checksum(&data, 0, 100);
        // 255*100 = 25500 = 0x639C
        assert_eq!((hi, lo), (0x63, 0x9C));
    }

    // ------------------------------------------------------------ DPI

    #[test]
    fn dpi_build_parse_roundtrip() {
        let cfg = DpiConfig {
            dpis: vec![800, 1600, 3200],
            colors: vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]],
            active: 1,
            angle_snapping: 0,
            ripple: 1,
            indication: 2,
        };
        let pkt = build_dpi(&cfg, None, 1).unwrap();
        assert_eq!(pkt.len(), 56);
        assert_eq!(pkt[0], 4);
        assert_eq!(pkt[5], LEVEL_MASKS[2]); // 3 levels -> mask 7

        // Verify checksum
        let (hi, lo) = checksum(&pkt, 3, 50);
        assert_eq!((pkt[50], pkt[51]), (hi, lo));

        // Simulate a 64-byte device read (pad with zeros)
        let mut read = pkt.clone();
        read.resize(64, 0);
        let back = parse_dpi(&read).unwrap();
        assert_eq!(back.dpis, vec![800, 1600, 3200]);
        assert_eq!(back.active, 1);
        assert_eq!(back.colors[0], [255, 0, 0]);
    }

    #[test]
    fn dpi_validation() {
        let mut cfg = DpiConfig::default();
        cfg.dpis = vec![10]; // too low
        assert!(build_dpi(&cfg, None, 1).is_err());

        cfg.dpis = vec![40000]; // too high
        assert!(build_dpi(&cfg, None, 1).is_err());

        cfg.dpis = vec![800; 7]; // too many
        assert!(build_dpi(&cfg, None, 1).is_err());
    }

    #[test]
    fn dpi_factory_defaults() {
        // These are the factory defaults from the JS + verified on hardware.
        let cfg = DpiConfig::default();
        assert_eq!(cfg.dpis, vec![1200, 2400, 3600, 4800, 8000, 30000]);
    }

    // ------------------------------------------------------------ polling rate

    #[test]
    fn rate_codes() {
        assert_eq!(rate_to_code(125), Some(8));
        assert_eq!(rate_to_code(250), Some(4));
        assert_eq!(rate_to_code(500), Some(2));
        assert_eq!(rate_to_code(1000), Some(1));
        assert_eq!(rate_to_code(2000), None);

        assert_eq!(rate_from_code(8), Some(125));
        assert_eq!(rate_from_code(1), Some(1000));
        assert_eq!(rate_from_code(99), None);
    }

    #[test]
    fn rate_build() {
        let pkt = build_rate(1000, 1).unwrap();
        assert_eq!(pkt, vec![6, 9, 1, 1, 0xFE, 0, 0, 0, 0]);
        // 0xFE = ~1 & 0xFF

        let pkt = build_rate(125, 1).unwrap();
        assert_eq!(pkt[3], 8);
        assert_eq!(pkt[4], (!8u8) & 0xFF);
    }

    // ------------------------------------------------------------ lighting

    #[test]
    fn light_build_parse() {
        let cfg = LightConfig {
            mode: LightMode::Static,
            color: [255, 0, 0],
            brightness: 8,
            speed: 2,
            sleep1_min: 1,
            sleep2_min: 10,
            debounce_ms: 8,
        };
        let pkt = build_light(&cfg, 1);
        assert_eq!(pkt.len(), 15);
        assert_eq!(pkt[0], 5);
        assert_eq!(pkt[3], (LightMode::Static as u8) << 4);
        assert_eq!((pkt[6], pkt[7], pkt[8]), (255, 0, 0));

        // Verify checksum
        let (hi, lo) = checksum(&pkt, 3, 11);
        assert_eq!((pkt[11], pkt[12]), (hi, lo));

        // Parse back (pad to 64 like device)
        let mut read = pkt.clone();
        read.resize(64, 0);
        let back = parse_light(&read).unwrap();
        assert_eq!(back.mode, LightMode::Static);
        assert_eq!(back.color, [255, 0, 0]);
        assert_eq!(back.brightness, 8);
    }

    #[test]
    fn light_sleep2_roundtrip_high_nibble() {
        // Regression test: sleep2 >= 16 uses the high nibble of byte 4.
        // Old buggy decode dropped the <<4, returning 11 instead of 171.
        for &sleep2 in &[10u8, 16, 20, 30, 171, 255] {
            let cfg = LightConfig { sleep2_min: sleep2, ..Default::default() };
            let pkt = build_light(&cfg, 1);
            let mut read = pkt.clone();
            read.resize(64, 0);
            let back = parse_light(&read).unwrap();
            assert_eq!(back.sleep2_min, sleep2, "sleep2={}", sleep2);
        }
    }

    #[test]
    fn light_mode_properties() {
        assert!(LightMode::Static.uses_color());
        assert!(!LightMode::Rainbow.uses_color());
        assert!(!LightMode::Off.uses_brightness());
        assert!(LightMode::Breathing.uses_speed());
        assert!(!LightMode::Static.uses_speed());
    }

    // ------------------------------------------------------------ buttons

    #[test]
    fn buttons_build_parse() {
        let mut map = ButtonMap::default();
        // Remap DPI button (idx 8) to keyboard 'A' (HID 4)
        map.slots[8] = (ButtonFunc::KEYBOARD, 0, 4);

        let pkt = build_buttons(&map, 1);
        assert_eq!(pkt.len(), 59);
        assert_eq!(pkt[0], 8);
        assert_eq!(pkt[1], 59);

        // Verify checksum
        let (hi, lo) = checksum(&pkt, 3, 57);
        assert_eq!((pkt[57], pkt[58]), (hi, lo));

        // Parse back
        let mut read = pkt.clone();
        read.resize(64, 0);
        let back = parse_buttons(&read).unwrap();
        assert_eq!(back.slots[0], (ButtonFunc::LEFT, 0, 0));
        assert_eq!(back.slots[8], (ButtonFunc::KEYBOARD, 0, 4));
    }

    #[test]
    fn button_func_names() {
        assert_eq!(ButtonFunc::name(2), "Left Click");
        assert_eq!(ButtonFunc::name(13), "DPI Cycle");
        assert_eq!(ButtonFunc::name(17), "Keyboard Key");
        assert_eq!(ButtonFunc::name(18), "Macro");
        assert_eq!(ButtonFunc::name(99), "Unknown");
    }

    // ------------------------------------------------------------ mailbox

    #[test]
    fn mailbox_request_format() {
        let req = mailbox_request(4, 1);
        assert_eq!(req, vec![160, 4, 56, 0, 1, 0, 0, 0]);

        let req = mailbox_request(11, 1);
        assert_eq!(req, vec![160, 11, 8, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn report_lens_known() {
        assert_eq!(report_lens(4), (56, 64));
        assert_eq!(report_lens(5), (15, 64));
        assert_eq!(report_lens(6), (9, 64));
        assert_eq!(report_lens(11), (8, 6));
    }

    #[test]
    fn macro_buffer_layout() {
        let m = autoclicker_preset(100);
        let a = build_macro_buffer(&m);
        assert_eq!(a.len(), 128);
        assert_eq!(a[0], 1); // loop while held
        // event count = 2, big-endian at 24-25
        assert_eq!((a[24], a[25]), (0, 2));
        // first event: key down, 10ms delay -> 0x01, 241
        assert_eq!((a[26], a[27]), (1, 241));
        // second event: key up, 100ms delay -> 0x8A, 241
        assert_eq!((a[28], a[29]), (0x8A, 241));
        // checksum covers bytes 0..126
        let (hi, lo) = checksum(&a, 0, 126);
        assert_eq!((a[126], a[127]), (hi, lo));
    }

    #[test]
    fn macro_packets_shape() {
        let m = autoclicker_preset(100);
        let [p0, p1, p2] = build_macro_packets(&m, 0);
        assert_eq!(p0.len(), 64);
        assert_eq!(&p0[0..4], &[9, 64, 1, 0]);
        assert_eq!(&p1[0..4], &[9, 64, 1, 1]);
        assert_eq!(p2.len(), 64);
        assert_eq!(&p2[0..4], &[9, 12, 1, 2]);
    }

    #[test]
    fn macro_presets_encode() {
        // All presets must build valid 128-byte buffers with correct event counts.
        for (name, m) in macro_presets() {
            let buf = build_macro_buffer(&m);
            assert_eq!(buf.len(), 128, "preset {name}");
            let count = ((buf[24] as u16) << 8) | buf[25] as u16;
            assert_eq!(count as usize, m.events.len(), "preset {name} count");
            // checksum valid
            let (hi, lo) = checksum(&buf, 0, 126);
            assert_eq!((buf[126], buf[127]), (hi, lo), "preset {name} checksum");
        }
        // Spot check: COPY = Ctrl down, C down, C up, Ctrl up
        let (_, copy) = macro_presets().into_iter().find(|(n,_)| *n=="COPY").unwrap();
        assert_eq!(copy.events.len(), 4);
        assert_eq!(copy.events[0].keycode, HID_LCTRL);
        assert!(!copy.events[0].is_up);
        assert_eq!(copy.events[1].keycode, 0x06);
        assert!(!copy.events[1].is_up);
        assert_eq!(copy.events[2].keycode, 0x06);
        assert!(copy.events[2].is_up);
        assert_eq!(copy.events[3].keycode, HID_LCTRL);
        assert!(copy.events[3].is_up);
    }
}
