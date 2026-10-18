# Vortex Ignix V2 (Xenta IGNIX F5V2) — Full HID Protocol Reference

**Device:** USB `1d57:22ae` (Xenta IGNIX F5V2, rebranded as Vortex Ignix V2)  
**Config interface:** USB interface 2 (use the `usage_page=0x0B` hidraw node)  
**Device ID in app:** 174 (`F5V2`)  
**Source:** Reverse-engineered from BK-MouseHub Electron app (`page-1ed75bcfb886c7bb.js`)  
**Date:** 2026-10-18

---

## Transport

All config uses **HID Feature Reports** on interface 2.

### Reading (mailbox pattern)

1. Send Feature Report ID **160** (0xA0): `[160, targetId, lenLo, lenHi, profile, 0, 0, 0]`
2. Wait ~500ms (the official app polls report 160 for a "ready" flag, but on this firmware the poll never signals — a fixed delay + direct read works reliably)
3. GET Feature Report `targetId` with the **correct length** (see per-report notes)

**Critical:** GET_REPORT length must be exactly right or the device STALLs:
- Reports 4, 5, 6: use **64 bytes** (NOT the HID descriptor sizes!)
- Report 11: use **exactly 6 bytes**
- Report 8: likely 64 bytes (unverified — JS requests 59)

### Writing

Send Feature Report directly with the full packet. No mailbox needed for writes.
- DPI writes: retry up to 3× with 180ms delay (device ACKs via a separate mechanism)
- Macro writes: 666ms delay between chunks (device is slow)

### Checksum

16-bit sum of specified byte range: `hi = (sum >> 8) & 0xFF`, `lo = sum & 0xFF`

---

## Report ID Summary

| ID (dec) | ID (hex) | Direction | Size (send) | Size (read) | Purpose |
|----------|----------|-----------|-------------|-------------|---------|
| 4 | 0x04 | R/W | 56 | 64 | DPI levels, sensor flags |
| 5 | 0x05 | R/W | 15 | 64 | Lighting, debounce, sleep |
| 6 | 0x06 | R/W | 9 | 64 | Polling rate |
| 8 | 0x08 | R/W | 59 | 64* | Button remapping (18 buttons) |
| 9 | 0x09 | W | 64 | — | Macro data (3 chunks per macro) |
| 11 | 0x0B | R | — | 6 | Firmware version (read-only) |
| 12 | 0x0C | R | — | 6 | Current profile ID (read-only) |
| 160 | 0xA0 | R/W | 8 | 8 | Mailbox for reads |

*Unverified — needs testing on hardware.

---

## Report 4 (0x04): DPI Configuration

**Send:** 56 bytes | **Read:** 64 bytes (device returns 64, parse first 52)

### Send Layout (56 bytes)

| Byte(s) | Field | Description |
|---------|-------|-------------|
| 0 | `0x04` | Report ID |
| 1 | `56` | Length |
| 2 | profile | Profile ID (1–5) |
| 3 | angleSnapping | Angle snapping: 0=off, 1=on |
| 4 | ripple | Ripple control: 0=off, 1=on |
| 5 | levelMask | Active level bitmask (see below) |
| 6–7 | 0 | Reserved |
| 8–15 | dpiLo[8] | DPI low bytes (8 levels) |
| 16–23 | dpiHi[8] | DPI high bytes (8 levels) |
| 24 | activeIdx+1 | Active DPI level (1-based) |
| 25–48 | rgb[8][3] | Per-level RGB colors (8 levels × 3 bytes) |
| 49 | indicationType | DPI indication: 1, 2, or 3 (default 2) |
| 50–51 | checksum | `sum(bytes[3..49])`, big-endian |
| 52–55 | 0 | Padding |

### Level Mask

From `[1, 3, 7, 15, 31, 63, 127, 255][count-1]` where count = number of active levels (1–8, max 6 for F5V2).

### DPI Encoding (F5V2, strategy class `W`)

**Raw 16-bit little-endian.** `dpi = (hi << 8) | lo`. No lookup table, no scaling.

- Valid range: 50–30000
- Step: 50
- Max levels: 6 (F5V2)

### Read Layout

Same as send layout. Parse:
- `dpis[i] = (d[16+i] << 8) | d[8+i]` for i in 0..7
- `count = popcount(d[5])`
- `active = d[24] - 1`
- `colors[i] = #%02x%02x%02x % (d[25+3*i], d[26+3*i], d[27+3*i])`

### F5V2 Factory Defaults

DPI: 1200, 2400, 3600, 4800, 8000, 30000  
Colors: #FF0000, #00FF00, #0000FF, #FFFF00, #00FFFF, #FF00FF  
Angle snapping: 0 (off), Ripple: 1 (on), Indication: 2

### Notes

- Device 168 (F7PRO V2) uses a different layout with LOD and motion sync packed into bytes 3–4. **F5V2 (174) does NOT support LOD or motion sync.**
- Y-axis DPI (`enableYAxis`) is false for F5V2 — X and Y are the same.

---

## Report 5 (0x05): Lighting / Debounce / Sleep

**Send:** 15 bytes | **Read:** 64 bytes (parse first 13)

### Send Layout (15 bytes)

| Byte(s) | Field | Description |
|---------|-------|-------------|
| 0 | `0x05` | Report ID |
| 1 | `15` | Length |
| 2 | profile | Profile ID (1–5) |
| 3 | mode | `(lightMode << 4) & 0xF0` |
| 4 | speedSleep | `(speed & 0x0F) \| (0xF0 & level2Sleep)` |
| 5 | brightSleep | `(brightness & 0x0F) \| ((level2Sleep & 0x0F) << 4)` |
| 6–8 | RGB | Red, Green, Blue (0–255 each) |
| 9 | sleep1 | Level 1 sleep timeout in minutes (default 1) |
| 10 | keyRespond/2 | Debounce time / 2 in ms (default 4 = 8ms) |
| 11–12 | checksum | `sum(bytes[3..10])`, big-endian |
| 13–14 | 0 | Padding |

### Light Modes

| Value | Name | Brightness | Speed | Color | Direction |
|-------|------|------------|-------|-------|-----------|
| 7 | Off | — | — | — | — |
| 0 | Static | ✓ | — | ✓ | — |
| 1 | Breathing | ✓ | ✓ | ✓ | — |
| 2 | Single Color Cycle | ✓ | ✓ | — | ✓ |
| 3 | Rainbow Cycle | ✓ | ✓ | — | ✓ |
| 4 | Marquee | ✓ | ✓ | — | ✓ |
| 5 | Flow | ✓ | ✓ | — | — |
| 6 | APM | ✓ | — | — | — |

- Brightness: 0–15
- Speed: 0–15

### Debounce (Key Respond)

Byte 10 = `round(keyRespond_ms / 2)`. Default 8ms → byte value 4.  
**This is the debounce/click latency setting.** Lower = faster clicks but more risk of double-click issues.

### Sleep

- `level1SleepMinutes` (byte 9): minutes before dimming (default 1)
- `level2SleepMinutes` (packed in bytes 4–5): minutes before deeper sleep (default 10)

---

## Report 6 (0x06): Polling Rate

**Send:** 9 bytes | **Read:** 64 bytes (parse first 9)

### Layout (9 bytes)

| Byte(s) | Field | Description |
|---------|-------|-------------|
| 0 | `0x06` | Report ID |
| 1 | `9` | Length |
| 2 | profile | Profile ID (1–5) |
| 3 | value | Rate code (see below) |
| 4 | ~value | Bitwise NOT of value (`(~value) & 0xFF`) |
| 5–8 | 0 | Padding |

### Rate Codes

| Code | Rate |
|------|------|
| 8 | 125 Hz |
| 4 | 250 Hz |
| 2 | 500 Hz |
| 1 | 1000 Hz |

---

## Report 8 (0x08): Button Remapping

**Send:** 59 bytes | **Read:** 64 bytes* (*unverified*)

### Send Layout (59 bytes)

| Byte(s) | Field | Description |
|---------|-------|-------------|
| 0 | `0x08` | Report ID |
| 1 | `59` | Length |
| 2 | profile | Profile ID (1–5) |
| 3–56 | buttons[18][3] | 18 buttons × (type, code1, code2) |
| 57–58 | checksum | `sum(bytes[3..56])`, big-endian |

For button `i` (0–17):
- `type` = `data[3 + 3*i]`
- `code1` = `data[4 + 3*i]`
- `code2` = `data[5 + 3*i]`

Unmapped buttons default to `type=1` (disabled), `code1=0`, `code2=0`.

### F5V2 (174) Buttons

| Button Index | Default Type | Function |
|--------------|--------------|----------|
| 0 | 2 | Left click |
| 1 | 3 | Right click |
| 2 | 4 | Middle click |
| 3 | 5 | Back |
| 4 | 6 | Forward |
| 8 | 13 | DPI cycle |
| 16 | 9 | Wheel up (DPI+?) |
| 17 | 10 | Wheel down (DPI-?) |

(Note: buttons 16/17 map to wheel up/down in the M table, which seems odd — may be DPI+/- on actual hardware. The `keysLayout` in the device JSON only shows 5 buttons for UI positioning.)

### Button Function Types

| Type | Code | Name (Chinese) | Description |
|------|------|----------------|-------------|
| 1 | KC_NO | 禁用 | Disabled |
| 2 | KC_BTN1 | 鼠标左键 | Left click |
| 3 | KC_BTN2 | 鼠标右键 | Right click |
| 4 | KC_BTN3 | 鼠标中键 | Middle click |
| 5 | KC_BTN5 | 鼠标后退 | Back |
| 6 | KC_BTN4 | 鼠标前进 | Forward |
| 7 | KC_DBL_CLICK | 双击 | Double click |
| 9 | KC_WH_U | 向上滚轮 | Wheel up |
| 10 | KC_WH_D | 向下滚轮 | Wheel down |
| 11 | KC_WH_L | 向左滚动 | Wheel left |
| 12 | KC_WH_R | 向右滚动 | Wheel right |
| 13 | KC_DPI_CYCLE | DPI循环 | DPI cycle |
| 14 | KC_DPI_UP | DPI+ | DPI increase |
| 15 | KC_DPI_DOWN | DPI- | DPI decrease |
| 16 | KC_EASY_AIM | 狙击键 | Sniper key (DPI shift) |
| 17 | (keyboard) | — | Keyboard key (code2 = HID usage) |
| 18 | (macro) | — | Macro (code1 = macro type, code2 = macro ID 1–32) |
| 21 | KC_MEDIA_SELECT | 打开播放器 | Open media player |
| 22 | KC_MPRV | 上一首 | Previous track |
| 23 | KC_MNXT | 下一首 | Next track |
| 24 | KC_MPLY | 播放/暂停 | Play/pause |
| 25 | KC_MSTP | 停止 | Stop |
| 26 | KC_MUTE | 静音 | Mute |
| 27 | KC_VOLU | 音量+ | Volume up |
| 28 | KC_VOLD | 音量- | Volume down |
| 29 | KC_CALC | 计算器 | Calculator |
| 30 | KC_MAIL | 邮件 | Email |
| 31 | KC_WFAV | 收藏夹 | Favorites |
| 32 | KC_WFWD | 浏览器前进 | Browser forward |
| 33 | KC_WBAK | 浏览器后退 | Browser back |
| 34 | KC_WSTP | 停止加载 | Stop loading |
| 35 | KC_MYCM | 我的电脑 | My computer |
| 36 | KC_WREF | 刷新页面 | Refresh |
| 37 | KC_WHOM | 浏览器主页 | Browser home |
| 38 | KC_WSCH | 搜索 | Search |
| 48 | — | — | (alt media codes) |
| 60 | — | — | (used in some device defaults) |

### Keyboard Keys (Type 17)

`code2` = USB HID keyboard usage code. Examples:
- `KC_A` (A): HID 4
- `KC_1` (1): HID 30
- Standard HID codes: 4–29 = A–Z, 30–39 = 1–0, 40 = Enter, 41 = Escape, 42 = Backspace, 43 = Tab, 44 = Space, etc.

`code1` is typically 0 (may be modifier flags).

### Macros (Type 18)

- `code1` = macro loop type (0 = ?, from `macroProfile.type`)
- `code2` = macro ID (1–32)
- When a button is set to type 18, the macro data must also be uploaded via report 9 (see below).

---

## Report 9 (0x09): Macro Data

**Write-only.** 64-byte packets, 3 chunks per macro.

### Macro Buffer (512 bytes, assembled from chunks)

| Byte(s) | Field | Description |
|---------|-------|-------------|
| 0 | type | Macro loop type |
| 1–3 | RGB | Macro indicator color |
| 4 | count | Repeat count |
| 5–23 | — | Reserved/padding |
| 24–25 | eventCount | Number of encoded events, big-endian |
| 26+ | events | Encoded macro events (2 bytes each) |
| 126–127 | checksum | `sum(bytes[0..125])`, big-endian |

### Event Encoding (2 bytes per event)

| Byte 0 | Byte 1 | Meaning |
|--------|--------|---------|
| `delay & 0x7F` | keycode | Key down (delay in 10ms units, 0–127) |
| `delay \| 0x80` | keycode | Key up (delay in 10ms units, 0–127) |
| `s` | `0x03` | Long-delay extension (see below) |

- **Delay:** in 10ms units, minimum 10ms (`Math.max(10, delay)`). Max 1270ms in a single byte.
- **Long delays (>1270ms):** encoded as base delay + extra `[s, 3]` pair where `s = floor(delay_ms / 200)` (max 255), and base is `delay_ms - 200*s`.
- **Keycode:** USB HID usage code. For mouse buttons (in macro context): `{0:241, 1:243, 2:242, 3:244, 4:245}`.
- **Max 50 events** per macro.

### Chunk Protocol

Three 64-byte Feature Reports (ID 9):

| Chunk | Bytes from buffer | Packet layout |
|-------|-------------------|---------------|
| 0 | `a[0..59]` | `[9, 64, macroIdx+1, 0, a[0..59]]` |
| 1 | `a[60..119]` | `[9, 64, macroIdx+1, 1, a[60..119]]` |
| 2 | `a[120..127]` | `[9, 12, macroIdx+1, 2, a[120..127], 0×52]` |

- `macroIdx` = 0-based macro index (from button's `keyValue`)
- 666ms delay between chunks (device is slow!)
- `n[1]` = 64 for chunks 0–1, 12 for chunk 2 (4 header + 8 data)

---

## Report 11 (0x0B): Firmware Version

**Read-only.** 6 bytes.

| Byte | Field |
|------|-------|
| 0 | `0x0B` (11) |
| 1 | 8 |
| 2 | Major version |
| 3 | Minor version |
| 4–5 | Device info (0x41, 0xAE = 174 for F5V2) |

Version string: `"{d[2]}.{d[3]:02d}"` (e.g., "1.04").

**Note:** Does NOT use the mailbox! Send `[160, 11, 8, 0, 0, 0, 0, 0]`, wait 100ms, read directly.

---

## Report 12 (0x0C): Profile ID

**Read-only.** 6 bytes (via mailbox: `getConfig(12, 9, 1)`).

| Byte | Field |
|------|-------|
| 0 | `0x0C` (12) |
| 3 | Current profile ID (1–5) |

---

## Report 160 (0xA0): Mailbox

Used to initiate reads. See "Reading" section above.

**Send:** `[160, targetId, lenLo, lenHi, profile, 0, 0, 0]` (8 bytes)  
**Read:** 8 bytes. `d[1]==1` means "ready" (but unreliable on this firmware — use fixed delay instead).

---

## Profiles

- **5 hardware profiles** (IDs 1–5)
- Each profile stores: DPI, lighting, polling rate, button mappings, macros
- Profile ID is byte 2 in all write reports
- Current profile read via report 12
- `restoreFactorySettings` resets profiles 1–5 (clamped)

---

## F5V2 (Device 174) Capability Summary

| Feature | Supported | Notes |
|---------|-----------|-------|
| DPI levels | ✓ | 50–30000, 50-step, max 6 levels, 16-bit LE |
| Per-level RGB | ✓ | 8 slots (though UI shows 6) |
| Polling rate | ✓ | 125/250/500/1000 Hz |
| Lighting modes | ✓ | 8 modes (off, static, breathing, single, rainbow, marquee, flow, apm) |
| Button remapping | ✓ | 8 buttons, 18 slots in protocol |
| Macros | ✓ | Up to 32 macros, 50 events each |
| Debounce (keyRespond) | ✓ | Via light report byte 10, default 8ms |
| Sleep settings | ✓ | Level 1 & 2 timeouts |
| Angle snapping | ✓ | On/off |
| Ripple control | ✓ | On/off |
| LOD (lift-off distance) | ✗ | Only on device 168 |
| Motion sync | ✗ | Only on device 168 |
| Y-axis independent DPI | ✗ | `enableYAxis: false` |
| Profiles | ✓ | 5 hardware profiles |

---

## Implementation Notes for Rust

1. **Use `hidapi` crate.** Open interface 2, `usage_page=0x0B` node.
2. **Read pattern:** `send_feature_report([160, id, lo, hi, profile, 0, 0, 0])` → sleep 500ms → `get_feature_report(id, 64)` (or 6 for report 11).
3. **Write pattern:** `send_feature_report(packet)` directly. For DPI, retry 3× on failure.
4. **Macro writes:** 666ms between the 3 chunks. Do NOT rush the device.
5. **Checksums:** Always compute and verify. The device may silently ignore bad checksums.
6. **Profile:** Default to profile 1. Read current via report 12 if needed.
7. **Button mapping:** Read current via report 8 before modifying (to preserve unmapped buttons).

---

## Unverified / Needs Hardware Testing

- [ ] Report 8 read length (59 vs 64)
- [ ] Report 8 write (button remapping) — protocol documented but not tested
- [ ] Report 9 macro write — protocol documented but not tested
- [ ] Profile switching (is there a "set active profile" command, or is it read-only?)
- [ ] Button indices 5, 6, 7, 9–15 (undefined in M table for 174)
- [ ] `type=60` and `type=0` meanings
- [ ] Macro loop types (what does `a[0]` type field accept?)
