# VortexSeries IGNIX F5 / IGNIX V2 Linux Driver

<div align="center">

![IGNIX F5 Banner](assets/vortex-ignix.png)

### Driver & Tool Konfigurasi Mouse VortexSeries IGNIX F5 / V2 untuk Linux
**Hasil Reverse Engineering dari Aplikasi Resmi Vendor (`IGNIX V2-Setup-1.0.0.exe`)**

[![Platform: Linux](https://img.shields.io/badge/platform-Linux%20(Wayland%20%2F%20X11)-blue.svg)](#)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](#)
[![Reverse Engineered](https://img.shields.io/badge/reverse--engineered-Official%20OEM%20App-orange.svg)](#)
[![Hardware: 1D57:22AE / 1D57:FA60](https://img.shields.io/badge/hardware-1d57%3A22ae%20%7C%201d57%3Afa60-purple.svg)](#)

</div>

---

## 📢 Pembaruan: Dari "Buatan Sendiri" Menjadi "Hasil RE App Aslinya"

Versi awal dari repository ini berawal dari prototipe GUI kustom yang dibangun dari nol menggunakan Rust + egui ("hasil buatan sendiri"). 

Kini repository ini telah **diperbarui secara total dengan hasil Reverse Engineering langsung dari aplikasi resmi vendor Windows (`IGNIX V2-Setup-1.0.0.exe` / OEM BK-MouseHub)**:
- **100% Fitur & Tampilan Asli:** Tampilan GUI, manajemen profil, tuning sensor PixArt PAW3315, efek RGB, dan editor makro identik 1:1 dengan aplikasi resmi Windows.
- **100% Native di Linux:** Aplikasi resmi berjalan langsung di atas Linux menggunakan runtime Electron dan WebHID standar W3C.
- **Zero Wine / Emulasi:** Tidak memerlukan Wine, Proton, atau layer emulasi Windows. Berkomunikasi langsung ke `/dev/hidraw*` pada kernel Linux.
- **Dukungan Penuh Wayland & X11:** Terintegrasi rapi dengan compositor Wayland modern (Niri, Hyprland, Sway, GNOME, KDE) dengan hardware acceleration penuh.

---

## 🚀 Quick Install (One-Line Installer)

Cukup jalankan satu baris perintah berikut di terminal:

```bash
curl -fsSL https://raw.githubusercontent.com/DarulQutni-Q/ignix-v2-linux/main/install.sh | bash
```

### Yang dilakukan oleh installer otomatis:
1. Memeriksa ketersediaan runtime Electron di sistem (`electron`, `electron39`, `electron42`, dsb).
2. Memasang paket aplikasi resmi hasil RE ke `~/.local/share/ignix-f5/`.
3. Memasang runner launcher script ke `~/.local/bin/ignix-f5`.
4. Memasang ikon resolusi tinggi ke direktori tema sistem dan `~/.local/share/icons/`.
5. Mendaftarkan desktop entry ke `~/.local/share/applications/ignix-f5.desktop` (langsung muncul di launcher aplikasi sistem).
6. Memasang aturan `udev` ke `/etc/udev/rules.d/99-ignix.rules` agar perangkat dapat diakses tanpa `sudo`.
7. Me-refresh cache desktop dan icon database.

Setelah instalasi selesai, Anda dapat langsung meluncurkan aplikasi dari menu launcher aplikasi (cari **"IGNIX F5"**) atau via terminal:
```bash
ignix-f5
```

---

## 📊 Perbandingan Fitur

| Fitur | Aplikasi Resmi Windows | Tool Custom Lama (v1 - Buatan Sendiri) | Versi Baru (v2 - Hasil RE App Aslinya) |
|---|---|---|---|
| **Antarmuka (GUI)** | Resmi OEM (Next.js/Chakra) | Custom egui sederhana | **100% Resmi OEM Asli** |
| **Kebutuhan Wine** | Wajib (jika di Linux) | Tidak butuh | **0% Wine (Murni Native Linux)** |
| **Profil Hardware** | 5 Profil tersimpan di mouse | 5 Profil | **5 Profil Lengkap** |
| **DPI Sensor Tuning** | 50 – 30.000 DPI (PAW3315) | 50 – 30.000 DPI | **50 – 30.000 DPI (Step 50)** |
| **Status Baterai** | Real-time Battery & Charging | Statis / Belum tersedia | **Live Battery & Charging Indicator** |
| **Kalibrasi Sensor** | Angle Snapping, Ripple, LOD | Sebagian | **Lengkap (Angle Snap, Ripple, LOD)** |
| **Macro Editor** | Visual Action Sequencer | 5 Preset terbatas | **Full Visual Multi-Action Macro Editor** |
| **Pilihan Mode RGB** | 8 Mode Efek Dinamis | 8 Mode | **8 Mode Efek + Custom Palette** |
| **Integrasi Wayland** | Tidak ada | Parsial | **Native Wayland + Auto Window Decoration** |

---

## 🎮 Kompatibilitas Hardware

Semua koneksi didukung penuh pada firmware 1.04 ke atas:

| Mode Koneksi | USB Vendor ID | USB Product ID | Status di Linux |
|---|---|---|---|
| **Kabel USB-C** | `0x1D57` | `0x22AE` | ✅ Terdeteksi Penuh (Auto-retry stall handling) |
| **2.4G Wireless Dongle** | `0x1D57` | `0xFA60` | ✅ Terdeteksi Penuh (LXDDZ Receiver) |

Kompatibel juga dengan keluarga mouse OEM sejenis:
- **IGNIX F5V2 / IGNIX V2** (`deviceId: 174`)
- **IGNIX F1V2** (`deviceId: 173`)
- **IGNIX F3V2** (`deviceId: 175`)
- **IGNIX F7V2** (`deviceId: 211`)
- **IGNIX F7PRO V2** (`deviceId: 168`)
- **IGNIX R1V2** (`deviceId: 208`)

---

## 🌐 Alternatif: Web Driver (Tanpa Instalasi)

Karena aplikasi resmi menggunakan arsitektur WebHID, Anda juga dapat mengakses driver resmi langsung melalui browser Chromium/Chrome (pastikan aturan udev sudah terpasang):

1. Buka browser Chromium / Google Chrome.
2. Kunjungi portal Web Driver resmi:
   - **VortexSeries Web Driver:** [https://vortexseries.net/support/webdriver](https://vortexseries.net/support/webdriver)
   - **Direct Web Driver URL:** [https://inputdrv.com/IGNIX](https://inputdrv.com/IGNIX)
3. Klik tombol hubungkan perangkat dan izinkan akses ke mouse VortexSeries Anda.

---

## 🛠️ Instalasi Manual dari Sumber (Git Clone)

Jika Anda ingin menginstall secara manual dari repository ini:

```bash
# 1. Clone repository
git clone https://github.com/DarulQutni-Q/ignix-v2-linux.git
cd ignix-v2-linux

# 2. Pasang aturan udev (akses non-root)
sudo cp 99-ignix.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger

# 3. Jalankan installer lokal
./install.sh
```

### Menjalankan Versi CLI Headless (Opsional)
Jika Anda menggunakan server Linux tanpa desktop GUI dan hanya membutuhkan binary Rust headless:
```bash
cargo build --release
./target/release/ignix-gui
```

---

## 🔬 Dokumentasi Teknis & Reverse Engineering

Untuk mempelajari bagaimana protokol HID, sistem mailbox, format paket, dan ekstraksi aplikasi dianalisis:
- [PROTOCOL.md](PROTOCOL.md): Spesifikasi lengkap format paket HID, mailbox handshake `0xA0`, register DPI, lighting, dan macro.
- [docs/RE_VENDOR_APP_REPORT.md](docs/RE_VENDOR_APP_REPORT.md): Laporan teknis mendalam tentang ekstraksi NSIS, ASAR unpacking, dan routing WebHID di Linux.
- [docs/IGNIX_V2_RE_REPORT.md](docs/IGNIX_V2_RE_REPORT.md): Laporan awal reverse engineering protokol perangkat keras.

---

## ⚠️ Troubleshooting

| Gejala | Penyebab & Solusi |
|---|---|
| **"Perangkat tidak terdeteksi"** | Pastikan aturan udev sudah terpasang: jalankan `sudo udevadm control --reload-rules && sudo udevadm trigger`, lalu cabut dan tancapkan kembali mouse atau dongle USB. |
| **"Error: No compatible Electron binary found"** | Pasang Electron dari package manager distro Anda: `sudo pacman -S electron` (Arch/CachyOS) atau `sudo apt install electron` (Debian/Ubuntu). |
| **Gagal menulis konfigurasi saat pakai kabel** | Mikrokontroler mouse melakukan pause sejenak saat commit data ke EEPROM. Driver memiliki sistem auto-retry built-in, tunggu 1-2 detik hingga sinkronisasi selesai. |

---

## 📄 Lisensi & Kredit

- **Lisensi:** MIT License.
- **Penafian:** Repository ini adalah proyek reverse-engineering independen untuk komunitas pengguna Linux dan tidak berafiliasi dengan Vortex Gaming / Xenta. Semua merk dagang dan hak cipta adalah milik pemilik resminya masing-masing.
