# VortexSeries IGNIX F5 / IGNIX V2 HID & WebHID Protocol Specification

> Comprehensive protocol reverse-engineered from the official OEM application (`IGNIX V2-Setup-1.0.0.exe` / BK-MouseHub / inputdrv.com) and verified against real hardware on Linux (firmware 1.04, wired cable `1d57:22ae` and 2.4G dongle `1d57:fa60`).

---

## 1. Overview & Architectural Stack

The **VortexSeries IGNIX F5** (part of the IGNIX V2 lineup powered by the PixArt PAW3315 optical sensor) uses a unified OEM platform developed on top of the **BK-MouseHub / inputdrv.com** infrastructure.

### Architecture Components:
- **Renderer Frontend:** Next.js static single-page application (`_next/static`, React, Chakra UI).
- **Driver Transport:** Browser-native **WebHID API** (`navigator.hid`).
- **Application Shell:** Electron wrapper (`main.js` + `preload.js`) that automatically grants WebHID access to vendor devices (`0x1D57`) without interactive browser prompts.
- **Official Web Driver Service:** Hosted online by VortexSeries / inputdrv at:
  - Official Portal: `https://vortexseries.net/support/webdriver`
  - Direct Web Driver URL: `https://inputdrv.com/IGNIX` (and OEM partner mirror `https://inputdrv.com/Attack-Shark`)

Because the driver communicates exclusively via standardized **WebHID reports**, it runs **100% natively on Linux** via Electron or Chromium-based browsers with zero emulation or Wine required once `udev` permissions are configured.

---

## 2. Hardware Identification & USB Descriptors

### USB Vendor & Product IDs:
| Mode | Vendor ID | Product ID | USB Product String | Device ID (`deviceId`) |
|---|---|---|---|---|
| **Wired USB Cable** | `0x1D57` | `0x22AE` | `"F5V2"` / `"IGNIX F5V2"` | `174` (`0xAE`) |
| **2.4 GHz Wireless Dongle** | `0x1D57` | `0xFA60` | `"LXDDZ 2.4G Wireless Device"` | `174` (`0xAE`) |

### Supported OEM Family Devices:
- `F1V2`: `173` (`0xAD`)
- `F3V2`: `175` (`0xAF`)
- `F5V2` / `IGNIX V2`: `174` (`0xAE`)
- `F7V2`: `211` (`0xD3`)
- `F7PRO V2`: `168` (`0xA8`)
- `R1V2`: `208` (`0xD0`)

---

## 3. WebHID Dual-Endpoint Discovery Model

The mouse and 2.4G dongle expose multiple HID collections on Linux (`/dev/hidraw1` to `/dev/hidraw4`). The driver filters and partitions them into two active endpoints:

1. **Communication Endpoint (`comm`):**
   - Filter criteria: `collection.usagePage === 11 (0x000B)` and `collection.usage === 0`.
   - Used for bidirectional configuration reading and writing via Output and Feature reports.
2. **Listener Endpoint (`listener`):**
   - Filter criteria: `collection.usagePage === 10 (0x000A)` and `collection.usage === 0`.
   - Used for asynchronous incoming notifications via `inputreport` (battery status, charging status, online presence, configuration ACKs).

---

## 4. Mailbox Handshake (Report 0xA0 / 160)

Before every configuration read, the host must arm the firmware mailbox register:

```
[0xA0, <targetReportId>, <lenLo>, <lenHi>, <profileId>, 0x00, 0x00, 0x00]
```

### Handshake Sequence:
1. Host sends arming packet to Feature Report `0xA0` (8 bytes).
2. Host polls `GET(0xA0)` (up to 6 iterations × 160 ms):
   - Firmware returns `[0xA0, 0x01, ...]` when data is ready.
3. Host waits 120 ms settling delay.
4. Host reads target report: `GET(targetReportId)` with buffer length 64 bytes.

---

## 5. Report Map & Packet Layouts

| Report ID | Name | Size (Bytes) | Description |
|---|---|---|---|
| `0x04` | **DPI Configuration** | 56 | 6 DPI stages (50–30,000 in steps of 50), per-stage RGB, active stage, angle snapping, ripple |
| `0x05` | **Lighting Configuration** | 15 | 8 modes (Off, Static, Breathing, Single Color, Rainbow, Marquee, Flow, APM), speed, brightness, sleep timers, debounce |
| `0x06` | **Polling Rate** | 9 | Report rate: `1` = 1000 Hz, `2` = 500 Hz, `4` = 250 Hz, `8` = 125 Hz |
| `0x08` | **Button Mapping** | 59 | 18 button slots (3 bytes per slot: kind, code1, code2) |
| `0x09` | **Macro Data** | Up to 128 | 3 chunks (64B, 64B, 12B) per macro slot, 50 event pairs max, 10ms timing unit |
| `0x0B` | **Firmware Version** | 8 | Major/minor version string |
| `0x0C` | **Profile Identifier** | 9 | Currently active hardware profile (1–5) |

---

## 6. Asynchronous Status & Battery Reporting

Received on the listener endpoint (`usagePage: 10`, `inputreport` event):
- `Byte 0`: Device ID (`0xAE` = 174 = `F5V2`).
- `Byte 1`: Event opcode (`0x40` or `0x41` for battery/power; `0x50` for configuration write ACK).
- `Byte 2`: Battery status:
  - `1`: Normal discharging
  - `2`: Fully charged
  - `3`: Currently charging
- `Byte 3`: Battery level index (1 to 10):
  $$\text{Battery Percentage} = \text{level} \times 10\%$$

---

## 7. Write Pacing & Cable Mode Notes

- **2.4G Wireless Dongle (`1d57:fa60`):** Fast non-blocking transfers with immediate readback verification.
- **Wired Cable (`1d57:22ae`):** The onboard microcontroller temporarily halts HID endpoint processing during EEPROM writes, resulting in transient USB stalls (`EPIPE` / Broken pipe). The driver handles this gracefully with exponential backoff (up to 6 retries × 300 ms).
