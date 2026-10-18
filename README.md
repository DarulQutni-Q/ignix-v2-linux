# Ignix V2 Configurator (Linux)

Native Linux config tool for the **Vortex Ignix V2** gaming mouse
(Xenta IGNIX F5V2, USB `1d57:22ae`). No Windows needed.

Reverse-engineered from the official BK-MouseHub Electron app's WebHID protocol.

## Install (one line)

```bash
curl -fsSL https://raw.githubusercontent.com/DarulQutni-Q/ignix-v2-linux/main/install.sh | bash
```

Downloads the latest prebuilt binary to `~/.local/bin`, installs the udev rule, and adds a desktop entry.

## Features

- **DPI**: 1–6 levels (50–30000, step 50), per-level RGB, active level, angle snapping, ripple control
- **Buttons**: remap 8 physical buttons (mouse, DPI, media, keyboard key, macro)
- **Lighting**: 8 modes (static, breathing, rainbow, ...), color, brightness, speed, sleep timers
- **Performance**: polling rate (125/250/500/1000 Hz), click debounce
- **Macros**: 32 slots (assignment works; recorder coming soon)
- **Profiles**: 5 hardware profiles

## Build

Requires Rust (https://rustup.rs):

```bash
cargo build --release
```

Binary: `target/release/ignix-gui`

Dependencies (Debian/Ubuntu): `libudev-dev`
Arch/CachyOS: `systemd-libs` (already present)

## Run without sudo (recommended)

```bash
sudo cp 99-ignix-v2.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
# unplug/replug the mouse
./target/release/ignix-gui
```

Without the udev rule, run with `sudo` (hidraw needs root).

## Safety

- Settings are only written when you click **Apply to Mouse**.
- Every write is verified by reading back from the device.
- `status` equivalent: the app reads all current settings on startup (read-only).

## Protocol

See [PROTOCOL_FULL.md](../PROTOCOL_FULL.md) for the complete reverse-engineered HID protocol.
Python CLI version: [ignix.py](../ignix.py).
