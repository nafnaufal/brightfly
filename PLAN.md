# Plan: Brightness Slider untuk Monitor Eksternal (Windows 11)

Dokumen ini adalah rencana implementasi lengkap. Target: aplikasi tray yang menampilkan flyout
slider brightness untuk monitor eksternal, tampilannya identik dengan flyout volume Windows 11,
dengan footprint memori dan CPU sekecil mungkin.

---

## 1. Ringkasan

| Item | Nilai |
|---|---|
| Platform | Windows 11 (build 22621+) |
| Bahasa | Rust (edition 2021) |
| UI | Win32 native + Direct2D / DirectComposition (digambar manual) |
| Kontrol brightness | DDC/CI VCP `0x10`, fallback gamma ramp |
| Backend | 1 worker thread (blocking DDC), tanpa async runtime |
| Dependency | praktis hanya crate `windows` |
| Target exe | ≤ 3 MB |
| Target RAM idle | ≤ 15 MB |
| Target CPU idle | 0% |

---

## 2. Batasan yang menentukan desain

### 2.1 Kenapa tidak bisa pakai slider bawaan Windows

Panel laptop dikontrol lewat ACPI (`WmiMonitorBrightness` / `WmiSetBrightness`).
Monitor eksternal tidak terhubung ke jalur itu. Monitor eksternal hanya bisa diredupkan lewat
**DDC/CI**: perintah yang dikirim ke chip monitor melalui jalur video (DP/HDMI), memakai
VCP code `0x10` (Brightness).

Windows 11 sampai 24H2/25H2 tidak menyediakan kontrol native yang bisa diandalkan untuk monitor
eksternal. Karena itu aplikasi pihak ketiga diperlukan.

**Konsekuensi:** keberhasilan bergantung pada monitor. Kalau DDC/CI dimatikan di OSD monitor,
atau kabel melewati adapter VGA/DVI/KVM tertentu, tidak ada perintah yang bisa dikirim.

### 2.2 Syarat "sangat ringan" menggugurkan Tauri

Pilihan stack sebelumnya (Tauri) bertabrakan dengan syarat ini. Perkiraan footprint:

| Stack | RAM idle | Ukuran distribusi | Waktu buka flyout |
|---|---|---|---|
| Tauri + WebView2 | ~80–150 MB (proses `msedgewebview2.exe` tetap hidup saat window disembunyikan) | ~8 MB + WebView2 Runtime | ~80–150 ms |
| Electron | ~150–250 MB | ~150 MB | ~200 ms |
| C# WPF | ~50–80 MB | butuh .NET runtime | ~150 ms |
| WinUI 3 (C++/WinRT) | ~35–60 MB | ~20 MB | ~300–500 ms |
| egui | ~25–40 MB | ~5 MB | ~50 ms |
| **Win32 native + Direct2D** | **~8–15 MB** | **~2–3 MB** | **~30–60 ms** |

Number di atas adalah perkiraan; yang menentukan adalah apakah ada browser engine / XAML runtime
yang di-load. Untuk aplikasi yang menetap di tray sepanjang hari, browser engine tidak sepadan.

**Keputusan:** Win32 native dengan Direct2D. Konsekuensinya, seluruh tampilan digambar manual
(tidak ada CSS/XAML), jadi estimasi kerja naik ~1,5–2× dibanding versi Tauri.
Kalau syarat "sangat ringan" dilonggarkan, kembali ke Tauri dan hemat waktu.

---

## 3. Spesifikasi UX

### 3.1 Bentuk flyout

Meniru flyout volume Windows 11 (bukan slider di Settings):

```
┌──────────────────────────────────────────────┐   radius 8, acrylic, drop shadow
│  ┌────┐                                      │   padding 16
│  │ ☀  │  ●━━━━━━━━━━━━━━━━━━━━━━○      64   │   tinggi baris 44
│  └────┘                                      │
└──────────────────────────────────────────────┘
     20px    track 4px tinggi, radius 2
```

Ukuran: lebar 360 DIP, tinggi = 16 + (44 × jumlah monitor) + 16. Satu monitor → 76 DIP.

| Elemen | Spesifikasi |
|---|---|
| Icon kiri | glyph `Segoe Fluent Icons` U+E706 (brightness), 20 px, warna teks sekunder |
| Track belum terisi | 4 px, radius 2, `rgba(255,255,255,0.25)` dark / `rgba(0,0,0,0.22)` light |
| Track terisi | 4 px, radius 2, accent color sistem |
| Thumb | lingkaran 20 px, putih (dark) / `#1A1A1A` (light), border tipis |
| Thumb saat hover/drag | membesar ke 24 px dengan animasi 100 ms |
| Angka persen | `Segoe UI Variable` 14 px SemiBold, lebar tetap, rata kanan |
| Sudut card | 8 px, pakai `DWMWA_WINDOW_CORNER_PREFERENCE` |
| Margin luar | 8 px agar shadow tidak terpotong |

Lebih dari satu monitor → baris ditumpuk di dalam card yang sama, dengan nama monitor
12 px warna sekunder di atas tiap slider **hanya jika jumlah monitor > 1**.

### 3.2 Interaksi

| Aksi | Perilaku |
|---|---|
| Klik kiri icon tray | Toggle flyout |
| Klik kanan icon tray | Menu: Sync semua, Mulai saat login, Keluar |
| Scroll di atas icon tray | naik/turun 2% tanpa membuka flyout |
| Drag thumb | ubah brightness, throttle 80 ms |
| Klik pada track | lompat ke posisi itu (animasi 120 ms) |
| Wheel di dalam flyout | ±2% |
| `Esc` atau klik di luar | tutup flyout |
| `↑` / `↓` saat flyout fokus | ±2% |
| Monitor tidak mendukung DDC | baris tetap tampil, slider disabled + tooltip alasan |

Flyout muncul di atas icon tray (`Shell_NotifyIconGetRect`), di-clamp ke work area monitor tempat
tray berada. Animasi masuk: fade 120 ms + geser 8 px dari bawah.

### 3.3 Perilaku sinkron

Kalau "Sync semua" aktif, menggerakkan satu slider menggerakkan semua. Nilai disimpan sebagai
persentase, bukan nilai mentah, karena `max` tiap monitor bisa berbeda (sering 100, kadang 255).

---

## 4. Struktur repo

```
win-11-monitor-brightness/
├── Cargo.toml
├── build.rs                 # embed icon + manifest (DPI PerMonitorV2)
├── PLAN.md
├── assets/
│   ├── app.ico
│   └── app.manifest
└── src/
    ├── main.rs              # entry, mutex single-instance, message loop
    ├── app.rs               # state global, koordinasi
    ├── monitor/
    │   ├── mod.rs           # MonitorId, MonitorInfo, Backend, Error
    │   ├── ddc.rs           # enumerasi + get/set VCP 0x10
    │   ├── gamma.rs         # fallback gamma ramp
    │   └── worker.rs        # thread per-monitor + timeout + coalescing
    ├── ui/
    │   ├── flyout.rs        # window, wndproc, show/hide, positioning
    │   ├── render.rs        # Direct2D + DirectWrite scene
    │   ├── anim.rs          # timer 60fps hanya saat beranimasi
    │   ├── theme.rs         # dark/light, accent, token warna
    │   └── tray.rs          # Shell_NotifyIcon
    ├── config.rs            # baca/tulis config.ini
    └── autostart.rs         # registry Run key
```

---

## 5. Detail teknis

### 5.1 Enumerasi & identitas monitor

Urutan panggilan (semua di `dxva2.dll`, tersedia di `windows` crate feature `Win32_Devices_Display`):

1. `EnumDisplayMonitors(NULL, NULL, callback, 0)` → kumpulkan `HMONITOR`.
2. Per HMONITOR: `GetMonitorInfoW` → `szDevice` (`\\.\DISPLAY1`), `rcMonitor`, `rcWork`.
3. `GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &n)`.
4. `GetPhysicalMonitorsFromHMONITOR(hmon, n, buf)` → `PHYSICAL_MONITOR { hPhysicalMonitor, szPhysicalMonitorDescription }`.
5. `GetVCPFeatureAndVCPFeatureReply(h, 0x10, &type, &cur, &max)` → deteksi dukungan + range.
6. Setelah selesai: `DestroyPhysicalMonitors(n, buf)`.

**Identitas persisten** (untuk menyimpan nilai per monitor) — `szDevice` tidak stabil, dan
`szPhysicalMonitorDescription` hampir selalu `"Generic PnP Monitor"`. Pakai device path:

```
EnumDisplayDevicesW(adapterName, 0, &dd, EDD_GET_DEVICE_INTERFACE_NAME)
→ dd.DeviceID = "\\?\DISPLAY#GSM5B7C#5&2b0f7f1&0&UID4352#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}"
```

Itu dipakai sebagai key config. Catatan: pindah port kabel = key berubah, nilai lama dianggap
tidak ada (fallback ke 50%).

`max` di-cache per monitor setelah enumerasi pertama; nilainya tidak berubah selama sesi.

### 5.2 Tulis brightness

- `SetVCPFeature(h, 0x10, value)` dengan `value = round(pct/100 * max)`.
- `GetVCPFeatureAndVCPFeatureReply` untuk refresh saat flyout dibuka.
- Kalau monitor tidak merespons 2× berturut-turut → tandai `Unavailable`, jangan spam call.
- Refresh ulang saat `WM_DISPLAYCHANGE` atau resume dari sleep.

### 5.3 Threading & timeout

`SetVCPFeature` **blocking**. Kalau monitor mati/asleep, call bisa menggantung beberapa detik.
Tidak ada parameter timeout di API — call tidak bisa dibatalkan.

Desain:

- Main thread: hanya UI, tidak pernah memanggil DDC.
- Satu worker thread per monitor. Tiap worker punya channel (std `mpsc`).
- UI mengirim nilai terbaru; worker **coalesce**: hanya memproses nilai terakhir, bukan tiap event.
- Kalau satu call > 2 detik, worker menandai monitor `Unresponsive` dan mengabaikan antrian
  sampai ada `WM_DISPLAYCHANGE` / resume.
- Monitor yang hang tidak memblokir monitor lain, karena thread terpisah.
- Saat aplikasi keluar, worker di-detach (tidak join) supaya tidak menggantung proses.

### 5.4 Gamma fallback

Untuk monitor yang tidak mendukung DDC/CI, dan sebagai penguat di bawah batas minimum backlight:

- `CreateDCW("DISPLAY", szDevice, NULL, NULL)` → `GetDeviceGammaRamp` / `SetDeviceGammaRamp`.
- Ramp baru = ramp asli × faktor. Simpan ramp asli di memori supaya bisa dipulihkan.
- Faktor dipetakan dari persentase: 0% → 0.25, 100% → 1.0 (gamma 0 terlalu gelap dan merusak warna).

Batasan yang harus ditulis di tooltip:
- Ini **bukan** backlight; yang diredupkan adalah output GPU.
- Diabaikan saat HDR aktif.
- Reset ketika mode display berubah (game fullscreen, ganti resolusi, driver reset) → refresh ulang
  pada `WM_DISPLAYCHANGE`.

Mode hybrid (disarankan): DDC 1–100% menggerakkan backlight asli; kalau user butuh lebih gelap dari
minimum hardware, lanjutkan dengan gamma 1.0 → 0.6.

### 5.5 Window, backdrop, dan DPI

- Window: `WS_POPUP`, `WS_EX_TOOLWINDOW` (tidak muncul di Alt+Tab), `WS_EX_NOREDIRECTIONBITMAP`.
- Backdrop, dicoba berurutan:
  1. `DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE(38), DWMSBT_TRANSIENTWINDOW(3))` — Win11 22H2+, ini yang paling mirip flyout volume.
  2. `SetWindowCompositionAttribute(ACCENT_ENABLE_ACRYLICBLURBEHIND)` — fallback Win10/11 lama.
     Efek samping: window jadi lag saat digeser; tidak relevan karena flyout diam.
  3. Solid `rgba(32,32,32,0.92)`.
- Sudut: `DWMWA_WINDOW_CORNER_PREFERENCE(33) = DWMWCP_ROUND(2)`.
- Dark mode: `DWMWA_USE_IMMERSIVE_DARK_MODE(20)`, ikuti `HKCU\...\Themes\Personalize\AppsUseLightTheme`.
- **Rendering:** DirectComposition + DXGI swapchain alpha (premultiplied) + Direct2D/DirectWrite.
  Alasan: window harus per-pixel alpha agar acrylic terlihat, dan D2D memberi anti-aliasing serta
  teks yang tajam tanpa flicker. Semua digambar di GPU → CPU ~0% saat idle.
- DPI: manifest `PerMonitorV2`. Detail skala di bagian 5.6.
- Animasidi jalan hanya selama transisi, pakai `SetTimer` 16 ms yang langsung dimatikan setelah
  selesai. Tidak ada timer yang berjalan terus.

### 5.6 Skala & ukuran (DIP, bukan resolusi)

Yang menentukan ukuran flyout **bukan resolusi, tapi skala DPI**. 1920×1080 di 100% dan 3840×2160
di 200% menghasilkan flyout dengan ukuran fisik yang sama di layar; jumlah pikselnya saja yang
berbeda. Kalau ukuran diikat ke resolusi, di layar 4K flyout akan terlihat mungil.

Aturan implementasi:

1. Semua angka di bagian 3.1 adalah **DIP** (device-independent pixel), basis 96 DPI.
2. Piksel fisik = DIP × (`dpi` / 96), dengan `dpi = GetDpiForWindow(hwnd)`.
3. Render target D2D di-set ke DPI window (`ID2D1DeviceContext::SetDpi(dpi, dpi)`). Setelah itu
   seluruh scene digambar dalam DIP dan konversi terjadi otomatis — **jangan** mengalikan manual
   di kode gambar, cukup di satu tempat (ukuran buffer swapchain).
4. Ukuran back buffer swapchain dalam piksel fisik; ukuran window (`SetWindowPos`) juga piksel fisik.
5. `WM_DPICHANGED` → pakai rect dari `lParam`, resize window, rebuild swapchain + resource D2D,
   gambar ulang. Ini terjadi kalau flyout muncul di monitor dengan skala berbeda.
6. Flyout memakai DPI **monitor tempat tray berada**, bukan monitor primer secara buta
   (`MonitorFromWindow` pada taskbar). Posisinya juga di-clamp ke `rcWork` monitor itu.
7. Jangan pakai `GetSystemMetrics` untuk ukuran yang bergantung monitor; itu nilai global/primary.

Ukuran hasil konversi:

| Konfigurasi | Skala | Flyout (px) | Tinggi baris (px) |
|---|---|---|---|
| 1920×1080 | 100% | 360 × 76 | 44 |
| 2560×1440 | 125% | 450 × 95 | 55 |
| 3840×2160 | 150% | 540 × 114 | 66 |
| 3840×2160 | 200% | 720 × 152 | 88 |

Batas aman: kalau lebar card melebihi `rcWork.width − 32 px` (hanya terjadi di layar sangat kecil,
misal 1024×600 dengan skala 100%), kurangi padding 16 → 12 DIP dan gap antar baris; lebar minimum
280 DIP. Jangan mengecilkan font di bawah 12 px hanya demi muat.

Windows sendiri tidak mengecilkan flyout volume di layar kecil, jadi meniru perilaku itu adalah
default yang benar; penyesuaian hanya sebagai pengaman agar tidak terpotong.

**Icon tray** juga ikut DPI: sediakan `.ico` multi-resolusi 16/20/24/32/40/48/64/256, supaya shell
memilih ukuran terdekat dengan `SM_CXSMICON` yang sudah diskalakan. Satu ukuran saja akan blur di
150% dan 200%.

**Saat resolusi berubah** (`WM_DISPLAYCHANGE`) skala DPI bisa ikut berubah — hitung ulang DPI dan
posisi, jangan hanya resize.

### 5.7 Tray

- `Shell_NotifyIconW(NIM_ADD)` + `NOTIFYICON_VERSION_4` (perlu untuk `Shell_NotifyIconGetRect`).
- Handle message: `NIN_SELECT` (klik kiri), `WM_CONTEXTMENU` (klik kanan),
  `WM_MOUSEWHEEL` (scroll langsung ubah brightness).
- Icon: `.ico` multi-resolusi (16/20/24/32/48/256), di-embed lewat `build.rs`.

### 5.8 Persist & autostart

- Config: `%APPDATA%\Brightfly\config.ini`
- Format sederhana, parser manual (tanpa serde supaya dependency tetap 1):

```ini
sync=1
start_with_windows=1
step=2
[\\?\DISPLAY#GSM5B7C#5&2b0f7f1&0&UID4352]
brightness=64
method=ddc
```

- Tulis config saat aplikasi keluar, bukan tiap perubahan slider (menghindari I/O terus-menerus).
- Autostart: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `Brightfly` = path exe.
  Tidak perlu Task Scheduler.
- Single instance: `CreateMutexW` bernama; kalau sudah ada, kirim pesan ke instance lama lalu keluar.

---

## 6. Anggaran performa (harus diukur, bukan diasumsikan)

| Metrik | Target | Cara ukur |
|---|---|---|
| Ukuran exe | ≤ 3 MB | `dir target\release\*.exe` |
| Working set idle (flyout tertutup) | ≤ 15 MB | Task Manager / `Get-Process` |
| Working set saat flyout terbuka | ≤ 25 MB | idem |
| CPU idle | 0% | Resource Monitor, 60 detik |
| CPU saat drag | ≤ 2% | idem |
| Waktu buka flyout | ≤ 60 ms | `QueryPerformanceCounter` di log |
| Waktu start sampai tray muncul | ≤ 300 ms | idem |
| Jumlah thread | ≤ 6 | Process Explorer |

Build profile:

```toml
[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

---

## 7. Model data

```rust
struct MonitorId(String);            // device path dari EnumDisplayDevicesW

struct MonitorInfo {
    id: MonitorId,
    name: String,                    // EDID friendly name, fallback "DISPLAY1"
    device: String,                  // \\.\DISPLAY1
    hmonitor: isize,
    physical_index: u32,
    max_brightness: u32,             // dari VCP reply
    method: Method,                  // Ddc | Gamma | Unsupported
    brightness_pct: u8,              // 0..=100
    state: MonitorState,             // Ready | Unresponsive | Asleep
}

enum Error {
    Unsupported,                     // VCP 0x10 tidak ada
    NoResponse,                      // call gagal / timeout
    MonitorOff,
    Enumeration,                     // tidak ada HMONITOR valid
}
```

---

## 8. Fase implementasi

Tiap fase punya kriteria selesai yang bisa diverifikasi. Jangan lanjut sebelum kriteria terpenuhi.

### Fase 0 — Probe (gate) · ~2–3 jam

CLI kecil, tanpa UI:

```
brightness-cli probe        # daftar monitor: nama, device, max, nilai sekarang, method
brightness-cli set 50       # set monitor pertama ke 50%
brightness-cli set --all 50
```

**Kriteria selesai:** mengeluarkan daftar monitor yang benar, `max` terbaca, dan `set` benar-benar
mengubah kecerahan fisik monitor.

**Kalau gagal:** cek DDC/CI di OSD monitor, cek kabel/adapter, coba port lain. Kalau tetap gagal,
seluruh proyek bergeser ke gamma-only (dan harus diputuskan sadar — hasilnya bukan backlight asli).

### Fase 1 — Core library · ~1–2 hari

`monitor/ddc.rs`, `monitor/gamma.rs`, `monitor/worker.rs`, `config.rs`.

**Kriteria selesai:** unit test untuk parser config; pengujian manual: drag 100× berturut-turut
tidak membuat antrian menumpuk dan tidak membuat UI freezee; monitor yang di-unplug tidak
membuat proses hang.

### Fase 2 — Shell aplikasi · ~1–2 hari

Message loop, tray icon, window flyout kosong yang bisa show/hide, sinyal single-instance.
Renderer Direct2D dasar (kotak + teks untuk verifikasi channel alpha).

**Kriteria selesai:** klik tray membuka/menutup flyout di posisi yang benar pada dua monitor dengan
skala DPI berbeda; `Esc` dan klik di luar menutup; app tidak muncul di Alt+Tab.

### Fase 3 — Visual match · ~2–3 hari

Acrylic/backdrop, corner radius, slider, thumb hover, animasi, tema light/dark, font.

**Kriteria selesai:** screenshot flyout aplikasi vs flyout volume asli pada ukuran sama,
light dan dark, diletakkan berdampingan dan tidak bisa dibedakan sekilas.

> Ambil dulu screenshot flyout volume asli (`Win+Shift+S`) sebagai referensi, dan cocokkan
> token warna/ukuran terhadap gambar itu — jangan mengandalkan ingatan.

### Fase 4 — Fitur · ~1–2 hari

Multi-monitor rows, sync all, scroll di tray, persist, autostart, menu klik kanan,
refresh saat `WM_DISPLAYCHANGE` dan resume.

**Kriteria selesai:** dua monitor (satu DDC, satu gamma-only) tampil benar; nilai bertahan setelah
restart; unplug/replug monitor mengembalikan baris ke kondisi yang benar.

### Fase 5 — Robustness & packaging · ~1–2 hari

Uji sleep/resume, monitor dimatikan saat aplikasi jalan, HDR on/off, drag cepat, resolusi berubah.
Buat build portable (zip) dan installer (NSIS via `cargo-wix`/`makensis`, atau cukup zip dulu).
Ikon aplikasi, versi, `--help`.

**Kriteria selesai:** semua item checklist bagian 10 lulus; ukuran exe dan RAM sesuai anggaran.

---

## 9. Risiko

| Risiko | Dampak | Mitigasi |
|---|---|---|
| Monitor menolak DDC/CI | Fitur inti mati | Probe di Fase 0; gamma fallback sudah masuk scope |
| `SetVCPFeature` menggantung | UI beku | Worker per monitor + batas 2 detik + tandai `Unresponsive` |
| Handle monitor basi setelah sleep | Set gagal senyap | Enumerasi ulang pada resume & `WM_DISPLAYCHANGE` |
| Backdrop acrylic tidak muncul di window `WS_POPUP` | Tampilan tidak mirip | Rantai fallback 3 tingkat; diverifikasi di Fase 3 |
| Device path berubah saat pindah port | Nilai tersimpan hilang | Terima, fallback ke 50%; dokumentasikan |
| DPI campuran antar monitor | Flyout blur / salah posisi | Manifest PerMonitorV2 + rebuild resource saat `WM_DPICHANGED` |
| SmartScreen memblokir exe unsigned | User ragu saat install | Signing opsional; sediakan source + build instruction |
| Waktu implementasi membengkak | Proyek mandek | Fase 0 gate; Fase 2 sudah bisa dipakai walau visual belum sempurna |

---

## 10. Checklist pengujian

- [ ] Satu monitor eksternal DDC/CI: set 0%, 50%, 100% → benar-benar berubah
- [ ] Dua monitor, DPI 100% dan 150% → posisi flyout benar di keduanya
- [ ] Ukur lebar flyout dengan penggaris di 1080p@100% dan 4K@200% → ukuran fisiknya sama
- [ ] Pindahkan flyout ke monitor dengan skala berbeda → teks tetap tajam, tidak blur
- [ ] Monitor dimatikan lalu dinyalakan → aplikasi tidak hang, slider kembali berfungsi
- [ ] Unplug lalu replug kabel → baris monitor kembali muncul tanpa restart
- [ ] Sleep → resume → slider masih mengubah brightness
- [ ] Ganti resolusi / refresh rate → flyout tetap tajam dan posisi benar
- [ ] HDR on → gamma fallback memberi peringatan, DDC tetap jalan
- [ ] Drag cepat 50× → tidak ada lag, tidak ada nilai "nyangkut" di tengah
- [ ] Light theme & dark theme → warna teks dan track kontrasnya cukup (WCAG AA untuk teks persen)
- [ ] Tutup aplikasi saat worker sedang menggantung → proses benar-benar keluar
- [ ] RAM dan CPU sesuai anggaran bagian 6
- [ ] Jalankan dua instance → instance kedua langsung keluar

---

## 11. Di luar scope (versi ini)

- Hotkey global + OSD overlay ala laptop
- Profil otomatis per aplikasi / per waktu (siang–malam)
- Kontrol kontras, suhu warna (VCP lain)
- Auto-brightness dari sensor cahaya
- Dukungan Windows 10 < 22H2
- Auto-update