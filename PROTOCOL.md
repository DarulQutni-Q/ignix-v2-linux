# Ignix V2 HID protocol (F5V2) — verified summary

Verified against real hardware on Linux (both the 2.4G dongle and the USB cable)
with firmware 1.04, and against the official BK-MouseHub app's WebHID JS.
The full reverse-engineering log lives in [docs/IGNIX_V2_RE_REPORT.md](docs/IGNIX_V2_RE_REPORT.md).

## Transport

- USB: `1d57:22ae` (USB cable) / `1d57:fa60` (2.4G dongle)
- Config channel: **interface 2**, vendor HID collection (usage page `0x0B`, usage `0`)
- All traffic = **HID feature reports** on the hidraw node, report id at byte 0.
  On hidraw this is `HIDIOCSFEATURE` / `HIDIOCGFEATURE`; report ids are *kept*
  in the buffer (firmware payload already has `[id, len_lo, len_hi, ...]` at byte 0).

## Mailbox handshake (report 0xA0, 8 bytes)

Before every read, arm the mailbox:

```
A0 <target> <len_lo> <len_hi> <profile> 00 00 00
```

Then poll `GET(0xA0)` (8 bytes) until the device answers `[A0 01 …]` (data
ready). Official timing: up to 6 polls × 160 ms, then a 120 ms settle before
reading the target report. If the mailbox never flags ready, one last read is
still attempted after ~500 ms.

**Reads**: `GET(target)` with buffer length 64 (6/8/9 for firmware/profile when
the device answers short) → `[target, len_lo, len_hi, payload…]`.

## Report map

| id  | report            | payload                                       |
|-----|-------------------|-----------------------------------------------|
| 0x04| DPI               | stages (dpi u16 LE, RGB), active stage, angle snap, indicator |
| 0x05| Lighting          | mode, color, brightness/speed, sleep timers, debounce |
| 0x06| Polling rate      | one byte: 8=125Hz, 4=250, 2=500, 1=1000       |
| 0x08| Buttons           | 18 slots × `kind, code1, code2`                |
| 0x09| Macros            | slot, loop type, repeat, RGB, delay-encoded events |
| 0x0B| Firmware          | `[0B, len, major, minor, 0x41, …]`             |
| 0x0C| Profile id        | `[0C, len, requested, active, …]`              |
| 0x10| (present in descriptor; not used on fw 1.04) | —                   |

Feature-report payload sizes observed on the wire: DPI write 56, lighting 15,
rate 9, buttons 59, macro up to 128 (50 event pairs max), mailbox 8.

## Writes & verification

Every **SET** is followed by a read-back through the mailbox and is retried
until the readback matches what was sent:

- DPI: 3 attempts, 180 ms apart
- Lighting / polling rate / buttons: 2 attempts
- Macros: written in ~666 ms-apart chunks (official `setMacroData` pacing)

Only the 6 physical button slots are exposed in the UI (left, right, middle,
back, forward and the DPI-cycle button); the remaining slots (wheel, spare
slot 8, …) are echoed back unchanged on every write.

## Macro event encoding

Each event row = a delay that **rides on the next event** (a trailing delay
with no following event is invalid and rejected by the device):
`[kind, <params>]` with a per-event delay byte (10 ms min).

- `Click(n)` — mouse button n (0..=4), emitted as a down+up pair
- `Press(u)` / `Release(u)` — keyboard HID usage byte (modifiers `0xE0–0xE7`)
- `Wait` — pure delay, folded onto the next event's delay

Long delays (>1270 ms) are encoded as a `[d, code]` pair plus an appended
`[s, 3]` extension byte, `s = floor(min(delay, 52270) / 200)`; the key-up flag
is bit 0x80 in the delay byte. Maximum 50 pairs per macro.

## Cable vs dongle

The cable firmware (22AE) **stalls `SET_REPORT` for a while after every mailbox
exchange** (`ioctl (SFEATURE): Broken pipe`). This tool therefore retries every
SET on `EPIPE` (300 ms spacing, up to 6 tries). The dongle never stalls. Reads
work identically on both transports — only the SET pacing differs.
