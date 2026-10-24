# RE Report — IGNIX V2 (official app) vs GLM_PROMPT.md

Target: `downloads/01M0CJNWW26EP6ETFQN6F9MWMH.zip` → `IGNIX V2-Setup-1.0.0.exe` (NSIS, electron-builder)
→ `ins/$PLUGINSDIR/app-64.7z` → Electron app (`IGNIX V2.exe`, `resources/app.asar` 68.7 MB).
Method: REA `analyze_javascript_application` pada `app.asar` (ringkasan, sha256
`87bb1283b439a8653da043a4fac8709b7a2ae49d129163743419fe491b91f520`), lalu ASAR diekstrak &
dideep-dive manual (beautified: `/tmp/opencode/ignix/asar-extracted/_next/static/chunks/app/page-pretty.js`,
semua referensi `L####` mengarah ke file ini).

## Identitas aplikasi
- BK-MouseHub **rebrand**: `client-config.json` = `{clientCode:"attack-shark", appName:"IGNIX V2", resetBaseUrl:"https://inputdrv.com/Attack-Shark"}`. App ini template OEM Tiongkok yang di-brand ulang per klien — *file yang sama* (`page-1ed75bcfb886c7bb.js`) mengapa `GLM_PROMPT.md` menyebut BK-MouseHub: benar, kode identik.
- Renderer = Next.js + **WebHID** (`sendReport`/`sendFeatureReport`/`receiveFeatureReport`); main.js pasang permission handlers + `select-hid-device` (main.js ~L184–270). Tidak ada native hid module.
- `device-config.js` hanya whitelist VID `0x1D57` (semua PID).
- Produk table (JS-embedded): F5V2 = **deviceId 174**, wired PID **0x22AE**, dongle PID **0xFA60** (konstanta `RECEIVER_24G_VID=7511`/`PID=64096`) — cocok dengan `1d57:22ae` & `1d57:fa60` OK. Interface dikode hard-coded `interface: 2` OK.
- Channel selection: config collection `usagePage 0x0B` (`11 === usagePage && usage === 0`), status collection `usagePage 0x0A`; device harus punya **kedua** collection OK (prompt: interface 2, usage_page 0x0B).

## Semua report (chip ke modul konfigurasi, Feature Reports)
| Klaim di GLM_PROMPT.md | Status | Bukti |
|---|---|---|
| Transport = Feature Reports, mailbox 160 = `[160, targetId, lenLo, lenHi, profile, 0,0,0]` (fungsi `getConfig`) | OK | L1272 |
| Report 4 DPI: 56 B, levelMask `[1,3,7,15,31,63,127,255]`, split lo@[8+i] hi@[16+i] (`dpi = hi<<8 \| lo`), aktif = idx+1 @byte24, RGB@25+, indicationType@49 (default 2), checksum sum(3..50) BE | OK | L941–1015 (strategi W utk deviceId 166/169/173/174/175/211/208: b6/b7 diabaikan), L1448–1487 |
| Info F5V2: DPI 50–30000 step 50, max 6 level, default 1200/2400/3600/4800/8000/30000 | OK | JSON device `ed` (L1681): `dpiConfig{min:50,max:30000,step:50,maxLevels:6}`, `supportedDpi` 1200…30000 |
| Report 5 Light: 15 B, mode `<<4`, speed&15@b4, brightness&15@b5, sleep2 = `((d[4]>>4)&0xF)<<4 \| ((d[5]>>4)&0xF)`, RGB@6–8, sleep1@9, debounce=b10×2, checksum sum(3..11) | OK | L1418–1435 (encode), L1333–1334 (decode — persis formula "official JS" di prompt) |
| Report 6 Rate: 9 B, kode 8=125/4=250/2=500/1=1000, `[4] = 255 & ~code` | OK | L1438–1442 |
| Report 8 Buttons: 59 B, 18 slot × 3 B, checksum sum(3..57) BE | OK | L1386–1404; default M[174] = slot0 L2,1 R3,2 M4,3 Back5,4 Fwd6,**8 DPI13**,16 WheelUp9,17 WheelDown10 (slot lain = disabled type 1) |
| Report 9 Macro: 512-B buffer, event count BE @24–25, checksum sum(0..126) BE @126–127, event maks **50**, delay unit 10 ms, keyup |= 0x80 di *delay byte* (safe utk 0xE0), long-delay s = ms/200 ≤255, mouse codes 241/243/242/244/245, slot = code2 1–32, 3 chunk 60/60/8 (+header `[9,64,s,0]`,`[9,64,s,1]`,`[9,12,s,2]`), jeda 666 ms | OK | L1513–1535 (sendMacroData), L1019–1052 (strategi B; device 166 pakai U yang berbeda — abaikan utk F5V2) |
| NOTE **Order long-delay**: ekstensi `[s, 3]` ditulis **SETELAH** pasangan event (bukan "prepend" seperti prompt) — event pair membawa *remainder*, lalu `o[r]=s, o[r+1]=3`. Cap delay total 200×255+remainder ≤ ~52270 ms. | FIX | L1051 |
| NOTE **Report 11 Firmware**: official = `[160,11,8,…]`, tunggu `versionReportWaitMs = **100 ms**` (bukan 500), GET 11, versi = `b2.b3(padded2)` OK, tapi JS validasi buffer **≥8 byte** — prompt bilang read "exactly 6" | CHECK ON HARDWARE | L1305–1308, L1540 |
| NOTE **Report 12 Profile**: read via mailbox, `profile = d[3]` OK, tapi JS cek panjang **≥9** (prompt: read 6). Write report 12 tidak pernah dipakai — report 12 = read-only. | CHECK ON HARDWARE | L1297–1299, L1270–1292 |
| NOTE **Ready-flag**: official **polling** GET feature 160 sampai 6×160 ms, sukses jika `[0]==160 && [1]==1`, lalu `delay(120)` baru GET target — prompt bilang "fixed 500ms, jangan polling" | FIX: pakai strategi hibrida | L1274–1288 |
| DPI retry 3×180 ms + ack | OK plus detail: ack via **input report** `b1==0x50`, `b3==4`, status `b2==0`, timeout 1500 ms | L1486–1499, L1176, L1240–1245 |
| Button/light/rate write verify | OK ditambah: **write pertama sesi** untuk report [5,6,8] di-retry 2×180 ms dengan ack (flag `retryFirstConfigWrite`) | L1236–1258, L1223–1224 |
| Func types 1–13, 17=Keyboard, 18=Macro | OK type 17: `code1 = mask modifier`, `code2 = HID usage`; type 18: `code1 = loop type`, `code2 = slot 1–32` (L3667, L4399). 14–16, 21–30 tidak kelihatan di file ini (picker UI pakai tabel i18n) — periksa di hw | L3648–3668 |
| Battery "NOT available, tampilkan --" | NOTE sebenarnya ada via **input report** `[0]=174, b1=0x40/0x41, b2=charge(1 bisa/2 penuh/3 charging), b3=level 1–10 → % = level×10` (F5V2 10 band). Wired mungkin tidak kirim report ini — tampilkan "--" aman, bisa di-upgrade nanti | L1887–1891, L1695–1708 |

## Prompt yang salah / perlu adjust (ringkas)
1. `prepend [s,3]` → **append after event pair**.
2. Ready-flag: implementasi **poll GET 160 (6×160 ms) → fallback fixed 500 ms** — dua-duanya tetap didukung; firmware F5V2 yang benar ditentukan saat on-hardware test.
3. Read length utk report 11/12: percepat `len=6` sesuai prompt, tapi **fallback ke 8/64** (minimal) kalau EIO/STALL — JS validasi ≥8/≥9.
4. Repeat count makro: official selalu menulis `count ≥ 1` (default 1); default prompt 0 — validasi di hw.
5. Tombol type-18 binding: `setMouseKeys` menulis `code2 = slot+1` untuk macro (bukan langsung id macro yang dipilih), sedangkan upload isi makro ke slot `code2`. Ada indikasi slot makro per-tombol = `slot tombol+1`. Verifikasi dengan read-back report 8 setelah assign macro #7 (mis. di tombol 1) sebelum finalisasi.
6. Profil: byte 2 semua report = profile id (1–5); **selalu isi byte 2** = profil aktif (dibaca via report 12). Tidak ada report switch profile terpisah — prompt rule 1 sudah benar secara semantik.

## Ukuran penting (siap dipakai di `protocol.rs`)
Write: DPI 56, Light 15, Rate 9, Buttons 59, Macro chunk 64/64/12, Mailbox 8.
Read: WebHID mengembalikan panjang sesuai descriptor; kode client cek konten ≥ len yang dikirim — hidapi READ: pakai 64 (cukup utk semua report), kecuali batasan prompt (grep 11/12: uji hw).

## Kesimpulan
Spec di `GLM_PROMPT.md` **~97% sudah benar** (sandaran RE kemarin valid — file codebase memang sama). Koreksi yang bersifat *bisa terbang* yang ingin diterapkan ke implementasi Rust:
- order ekstensi long-delay makro,
- hibrida ready-poll/fixed-wait,
- fallback read-length report 11/12,
- detail ack input-report (0x50/0xAA) dan battery — bonus, bukan blocker.

Ekstraksi tetap tersimpan di `/tmp/opencode/ignix/` (asar-extracted, page-pretty.js utk referensi future).
