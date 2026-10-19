# Ignix V2 Configurator (Linux)

Native Linux configuration tool for the **Vortex Ignix V2** gaming mouse
(Xenta IGNIX F5V2). Rust + egui + hidapi — no Windows, no browser, no WebHID.

The HID protocol was reverse-engineered from the official BK-MouseHub
Electron app and then **verified against real hardware** (firmware 1.04) on
both the 2.4G dongle and the USB cable. See
[PROTOCOL.md](PROTOCOL.md) and the full log in
[docs/IGNIX_V2_RE_REPORT.md](docs/IGNIX_V2_RE_REPORT.md).

> Unofficial community tool. Not affiliated with Vortex/Xenta.

## Compatibility

| Connection | USB id      | Status |
|------------|-------------|--------|
| 2.4G dongle| `1d57:fa60` | fully supported |
| USB cable  | `1d57:22ae` | fully supported (SET-stall auto-retry) |

## Features

- **Profiles** — 5 hardware profiles, read + switch
- **DPI** — up to 6 stages, 50–30000 (step 50), per-stage RGB, active stage,
  angle snapping, ripple
- **Buttons** — remap the 6 physical buttons (left, right, middle, back,
  forward, DPI-cycle): mouse buttons, DPI cycle, media keys, keyboard keys,
  or macro slots
- **Macros** — 32 slots, full event editor (click / key press / key release /
  wait, 50 events max), loop modes (once / while held / toggle), per-slot RGB,
  plus **5 one-click presets**:
  - Double click (hold button)
  - Click spam 10/s (hold)
  - Hold W (AFK walk)
  - Reload + quick switch
  - Jump loop (bunny hop, hold)
- **Lighting** — 8 modes (Off, Static, Breathing, Single Color Cycle,
  Rainbow Cycle, Marquee, Flow, APM), color, brightness, speed
- **Performance** — polling rate 125/250/500/1000 Hz, sleep timers,
  button debounce
- **Safety** — every write is **read back and verified** before it is
  reported as OK; per-part OK/FAILED status line; single *Apply to Mouse*
  button; unused report slots are echoed back unchanged, never zeroed

## Build

Requires a Rust toolchain (https://rustup.rs) and the udev development
headers:

```bash
# Debian/Ubuntu
sudo apt install libudev-dev
# Arch/CachyOS: systemd-libs is already installed

cargo build --release
# binary: target/release/ignix-gui
```

## Install

### From source

```bash
cargo build --release
sudo cp 99-ignix-v2.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
# unplug/replug the mouse once, then:
./target/release/ignix-gui
```

The udev rule grants non-root access to both the dongle and the cable
hidraw nodes (`uaccess` + `0666`).

### Release installer

`install.sh` installs the binary from the **latest published GitHub
release** plus the udev rule and a menu entry:

```bash
curl -fsSL https://raw.githubusercontent.com/DarulQutni-Q/ignix-v2-linux/main/install.sh | bash
```

Note: the published release can lag behind `main`; building from source
always gives you the newest code.

## Usage

- The header shows the detected device (transport + firmware version).
  Press **Refresh** after plugging/unplugging or switching cable ⇄ dongle —
  the app re-opens the device automatically.
- Edit anything, then hit **Apply to Mouse**. Each part (buttons, macros,
  DPI, rate, lighting) is written, read back and compared; the status line
  reports OK or FAILED per part.
- Macros → select a slot → *Load preset* fills name, loop mode, repeat and
  events; tweak rows or add your own; **Upload this slot** writes just that
  slot.
- Cable mode note: the wired firmware briefly refuses config writes right
  after each exchange (USB stall / `Broken pipe`). The app retries
  automatically — you only ever see a delay, not an error.

## How it works (short version)

All configuration lives on HID **feature reports** of interface 2 (vendor
usage page `0x0B`). Reads go through an 8-byte mailbox handshake
(`A0 <target> <len> <profile>` → poll `A0` ready → read target report);
writes are feature-report SETs followed by a read-back compare. Macro rows
encode their delay on the *following* event; delays > 1270 ms use a
`[d, code] + [s, 3]` extension pair (cap ≈ 52 s, 50 pairs max).

Full details: [PROTOCOL.md](PROTOCOL.md).

## Hardware test suite

Unit tests (protocol encoding) run offline; the live tests need the mouse
plugged in and are `#[ignore]`-gated:

```bash
cargo test                                   # 15 protocol unit tests
cargo test --release -- --ignored            # live: read all, write-verify same values, macro write
```

`live_write_same_values` writes back exactly what it read (no settings
change); `live_write_macro_autoclick` fills macro slot 1 with the 10 CPS
click-spam preset.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| "Not connected" | Install the udev rule, replug, press Refresh |
| Permission denied | `sudo cp 99-ignix-v2.rules /etc/udev/rules.d/ && sudo udevadm control --reload-rules && sudo udevadm trigger` |
| Stuck after switching cable ⇄ dongle | Press Refresh (the app drops the stale handle automatically) |
| Write FAILED repeatedly | Replug the mouse; if it persists, report the exact status text in an issue |
