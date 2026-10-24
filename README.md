# Ignix V2 / F5 untuk Linux

Tool konfigurasi mouse VortexSeries Ignix V2 / F5 (Xenta F5V2) di Linux, dari
hasil reverse engineering aplikasi resmi vendor Windows (`IGNIX V2-Setup-1.0.0.exe`,
BK-MouseHub). Repo ini berisi dua hal yang berbeda, jangan dicampur:

1. **Aplikasi vendor (jalur utama installer).** Aplikasi Next.js + WebHID dari
   vendor, dijalankan di Linux lewat Electron. File aplikasinya (`app.asar`,
   sekitar 67 MB) tidak ada di repo ini; installer mengunduhnya dari asset
   release `v2.0.0` di repo ini.
2. **Tool Rust (`src/`, egui + hidapi).** Tool independen yang bicara langsung
   ke mouse lewat HID feature report, tanpa Electron dan tanpa Wine. Ini yang
   dibangun dari `cargo build`. Installer di bawah tidak memasang tool ini.

## Instalasi aplikasi vendor

```bash
curl -fsSL https://raw.githubusercontent.com/DarulQutni-Q/ignix-v2-linux/main/install.sh | bash
```

Yang dilakukan installer:

- Memeriksa runtime Electron di PATH (`electron39`, `electron42`, dan
  seterusnya; kalau tidak ada, instalasi lanjut tapi aplikasi belum bisa jalan
  sampai Electron dipasang).
- Mengunduh `app.asar` dari release terbaru dan menyimpannya di
  `~/.local/share/ignix-f5/`.
- Memasang launcher `~/.local/bin/ignix-f5`, ikon, dan desktop entry
  `~/.local/share/applications/ignix-f5.desktop`.
- Memasang aturan udev ke `/etc/udev/rules.d/99-ignix.rules` (pakai sudo kalau
  perlu) supaya mouse bisa diakses tanpa root, lalu reload aturan udev.

Jalankan dari launcher (cari "IGNIX F5") atau dari terminal:

```bash
ignix-f5
```

## Tool Rust dari source

```bash
# 1. Aturan udev (akses tanpa root)
sudo cp 99-ignix.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger

# 2. Build dan jalan
cargo build --release
./target/release/ignix-gui
```

Tool Rust ini mengatur DPI (50-30000, step 50, maks 6 stage), lighting,
polling rate (125/250/500/1000 Hz), remap tombol, dan makro lewat report
HID 4, 5, 6, 8, dan 9. Profil yang diedit adalah profil aktif di mouse (1-5),
dibaca dari report 12.

Batasan yang diketahui dari tool Rust:

- Mode kabel (wired) di firmware ini diam saat konfigurasi: pembacaan
  firmware kembali `0.00` dan sebagian write stall. Tool menolak lanjut
  dengan pesan itu. Pakai dongle 2.4G untuk konfigurasi.
- Baterai tidak ditampilkan tool Rust. Laporan baterai memang ada di
  protokol vendor lewat input report (lihat PROTOCOL.md), tapi tool Rust
  belum mengimplementasikannya, jadi tidak ada angka baterai palsu di UI.

## Hardware

| Koneksi | Vendor ID | Product ID | Keterangan |
|---|---|---|---|
| Kabel USB-C | `0x1D57` | `0x22AE` | F5V2, deviceId 174 |
| Dongle 2.4G | `0x1D57` | `0xFA60` | Receiver LXDDZ |

Keluarga OEM yang sama di aplikasi vendor: F1V2 (173), F3V2 (175),
F5V2 (174), F7V2 (211), F7PRO V2 (168), R1V2 (208). Tool Rust hanya
diuji untuk F5V2.

## Alternatif: web driver vendor (tanpa instalasi)

Aplikasi vendor berbasis WebHID, jadi bisa juga dibuka di Chromium/Chrome
setelah aturan udev terpasang:

- https://vortexseries.net/support/webdriver
- https://inputdrv.com/IGNIX

## Dokumentasi teknis

- `PROTOCOL.md`: format paket HID, mailbox handshake `0xA0`, dan layout
  tiap report.
- `docs/RE_VENDOR_APP_REPORT.md`: cara aplikasi vendor diekstrak dari
  installer NSIS (ASAR) dan routing WebHID di Linux.
- `docs/IGNIX_V2_RE_REPORT.md`: laporan RE awal, termasuk koreksi
  terhadap asumsi protokol yang lama.

## Troubleshooting

| Gejala | Solusi |
|---|---|
| Perangkat tidak terdeteksi | Pastikan aturan udev terpasang dan reload, lalu cabut-colok mouse atau dongle. |
| `No compatible Electron binary found` | Pasang Electron dari package manager distro (`pacman -S electron` di Arch/CachyOS, `apt install electron` di Debian/Ubuntu). |
| Gagal menulis konfigurasi saat pakai kabel | Lihat batasan mode kabel di atas; pakai dongle 2.4G. |

## Lisensi dan kredit

Lisensi MIT. Proyek reverse engineering independen, tidak berafiliasi
dengan Vortex Gaming / Xenta. Merek dagang milik pemiliknya masing-masing.
