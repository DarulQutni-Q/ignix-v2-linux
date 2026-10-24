# Reverse-Engineering Report: Official OEM Vendor App to Native Linux Driver

**Target:** `IGNIX V2-Setup-1.0.0.exe` (Official VortexSeries IGNIX V2 / F5 Windows Installer, 136 MB)  
**Hardware Tested:** VortexSeries IGNIX F5 / IGNIX V2 (PixArt PAW3315 Optical Sensor, Firmware 1.04)  
**Author:** Community Reverse Engineering Project

---

## 1. Executive Summary

Earlier community efforts created a custom prototype GUI from scratch in Rust (`ignix-gui`) based on initial USB packet captures. While functional for basic DPI and RGB adjustments, advanced features like official animated themes, real-time battery indicators, and complex multi-action macro sequences required replicating the complete OEM logic.

By reverse-engineering the official Windows installer (`IGNIX V2-Setup-1.0.0.exe`), we discovered that the vendor software is not a Win32/WPF native program, but a modern **Next.js + WebHID application** bundled inside an Electron shell.

Because WebHID is a W3C cross-platform standard that maps to Linux `/dev/hidraw*` character devices, the vendor application can run on Linux via system Electron without Wine. No latency measurement was done, so no latency claim is made here.

---

## 2. Disassembly & Extraction Pipeline

### Step 1: NSIS Container Extraction
The installer `IGNIX V2-Setup-1.0.0.exe` was created using `electron-builder` and NSIS.
Extraction using `7z`:
```bash
7z x "IGNIX V2-Setup-1.0.0.exe" -o"extracted_nsis"
```
Extracted structure:
- `$PLUGINSDIR/app-64.7z` (64-bit Electron runtime + resources)
- Unpacking `app-64.7z` yielded `resources/app.asar` (67.1 MB).

Note: `app.asar` itself is not committed to this repo. It is distributed as an asset of release `v2.0.0`. The steps below describe what was extracted from the vendor installer, not a file you will find in this tree.

### Step 2: ASAR Archive Inspection
Using `npx asar extract app.asar asar_extracted`, we analyzed the bundled architecture:
- `main.js`: Electron main process setup. Automatically intercepts WebHID device selection queries (`select-hid-device`) and auto-approves devices with VID `0x1D57`.
- `preload.js`: Context isolation bridge exposing WebHID and window control APIs.
- `client-config.json`: Client identity (`"appName": "IGNIX V2"`, `"clientCode": "attack-shark"`).
- `device-config.js`: Whitelist of vendor IDs (`0x1D57`).
- `_next/static/`: Production Next.js single-page application with Chakra UI styling.

---

## 3. WebHID Endpoint Routing on Linux

In Chromium and Electron on Linux, WebHID access requires read/write permissions on the underlying `/dev/hidraw*` nodes. Without appropriate `udev` rules, calls to `navigator.hid.requestDevice()` fail silently or return empty arrays.

By deploying the udev rules:
```udev
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1d57", ATTRS{idProduct}=="22ae", MODE="0666", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1d57", ATTRS{idProduct}=="fa60", MODE="0666", TAG+="uaccess"
```
the Linux kernel grants unprivileged user access to interface 2 (usage page `0x000B`) and interface 3 (usage page `0x000A`).

When running via system Electron (`electron /path/to/app.asar`), the official Next.js frontend discovers the mouse immediately, completes the `0xA0` mailbox handshake, and reads the hardware state.

---

## 4. Key Findings & Hardware Validation

1. **Transport:** WebHID issues `ioctl(HIDIOCSFEATURE)` and `ioctl(HIDIOCGFEATURE)` system calls, the same calls a native program would use. That says nothing about UI latency, which was not measured.
2. **Battery reporting:** Discovered the listener endpoint on collection `0x000A` receiving asynchronous `inputreport` packets with battery charge levels (1-10 scale = 10%-100%).
3. **Macro Engine Compatibility:** Verified that the official macro compiler formats delay units in 10ms intervals with long-delay extension headers, eliminating macro drift.
4. **Wayland Support:** The launcher passes `--ozone-platform-hint=auto` and `--enable-features=WaylandWindowDecorations` when `WAYLAND_DISPLAY` is set. Wayland behaviour was not verified on every compositor listed in older versions of this doc.
