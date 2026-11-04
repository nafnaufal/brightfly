# Progress

Diperbarui: fase 3 selesai.

## Status

| Fase | Isi | Status |
|---|---|---|
| 0 | Probe DDC/CI (gate) | ✅ lulus |
| 1 | Core library | ✅ selesai |
| 2 | Shell: tray + flyout | ✅ selesai |
| 3 | Visual match flyout volume | ✅ selesai |
| 4 | Fitur (multi-monitor, sync, persist, autostart) | ⬜ belum |
| 5 | Robustness + packaging | ⬜ belum |

## Hasil gate (fase 0)

Hardware uji: 1 monitor eksternal MSI.

```
name     : Generic PnP Monitor
device   : \\.\DISPLAY1
id       : \\?\DISPLAY#MSI30B9#7&21ebfe3c&0&UID24646#{e6f07b5f-...}
method   : ddc
range    : 0..100
current  : 50%
```

- DDC/CI berfungsi penuh: baca dan tulis.
- Range 0..100 (bukan 0..255).
- Tulis 70% → dibaca balik 70%. Tulis 50% → dibaca balik 50%.
- Config tersimpan di `%APPDATA%\Brightfly\config.ini` dan terbaca ulang dengan benar.

## Fase 3 — hasil uji

Target visual: flyout volume Windows 11, diukur pada DPI 100% (kartu 192×48 px,
radius 8, ikon 16 px, track 4 px dari x=42 ke x=150, angka 15 px rata kanan).

| Uji | Metode | Hasil |
|---|---|---|
| Geometri & warna | bandingkan piksel dengan screenshot volume asli | ✅ selisih ≤1 px / 1 level warna |
| Track kosong | token `ControlStrongFillColorDefault` (putih 54%) | ✅ `#9F9F9F` sama dengan asli |
| Track terisi | `AccentPalette` index 1 (gelap) / 4 (terang) | ✅ `#BDBAE7` |
| Thumb | hanya saat hover/drag | ✅ |
| Buka–tutup 5× | screenshot tiap siklus | ✅ isi selalu tergambar |
| Klik track | kirim `WM_LBUTTONDOWN` di 25% dan 80% | ✅ monitor ikut 25% / 80% |
| Drag | `WM_MOUSEMOVE` saat tombol ditahan | ✅ |
| Wheel | `WM_MOUSEWHEEL` | ✅ 82% → 84% → 82% |
| Panah kiri/kanan | `WM_KEYDOWN` | ✅ ±2% |
| `Esc` menutup | `WM_KEYDOWN` VK_ESCAPE | ✅ |
| Tema terang | `AppsUseLightTheme=1` | ✅ kartu `#F4F4F4`, aksen biru |
| Working set (release) | idle / flyout terbuka | 10.4 MB / 22.6 MB (private 5.1 MB) |
| Thread | idle / flyout terbuka | 5 / 7 |

## Fase 2 — hasil uji

| Uji | Metode | Hasil |
|---|---|---|
| Icon tray muncul | proses berjalan | ✅ |
| Klik tray membuka flyout | instance kedua memicu `WM_TOGGLE` | ✅ visible=true |
| Klik tray menutup flyout | idem | ✅ visible=false |
| `Esc` menutup | `SendKeys {ESC}` | ✅ |
| Klik di luar menutup | flyout kehilangan fokus | ✅ |
| Tidak muncul di Alt+Tab | `WS_EX_TOOLWINDOW` (exstyle 0x88) | ✅ |
| Single instance | instance kedua langsung keluar | ✅ |
| Posisi | di atas tray, di-clamp ke work area | ✅ |
| Working set | Task Manager | 15.4 MB (debug) |
| Thread | — | 4 |

## Keputusan teknis yang diambil

- **Tanpa channel, pakai slot `desired` + Condvar.** Lebih sederhana daripada `mpsc`
  untuk coalescing, dan tidak mungkin menumpuk.
- **Acquire/destroy physical monitor handle per operasi.** Handle lama tidak disimpan,
  jadi unplug/replug tidak meninggalkan handle basi.
- **Join handle tidak di-join saat shutdown.** Proses harus tetap bisa keluar walau ada
  panggilan DDC yang menggantung.
- **State controller disimpan di `GWLP_USERDATA`, bukan `thread_local`.** Wndproc bisa
  dipanggil reentran selama `SetWindowPos`; `thread_local` + `RefCell` akan panic.
- **`force_foreground()` pakai `AttachThreadInput`.** `SetForegroundWindow` polos ditolak
  Windows kalau proses bukan penerima input terakhir, akibatnya `Esc` tidak sampai ke flyout.
- **`GetDeviceGammaRamp` ada di `Win32::UI::ColorSystem`**, bukan `Win32::Graphics::Gdi`.

## Penyimpangan dari plan

- **Renderer Fase 3 tidak memakai DirectComposition/D3D/acrylic.** Rencana awal
  memakai swapchain alpha + backdrop acrylic. Kenyataannya visual DirectComposition
  lepas setiap `ShowWindow(SW_HIDE)` → `SW_SHOW`, sehingga flyout yang dibuka lagi
  tampil kosong; membuat ulang device/target tiap show hanya menutupi sebagian dan
  interaksi tetap tidak andal. Backend diganti ke **`ID2D1DCRenderTarget`**: gambar
  langsung ke HDC di `WM_PAINT`, kartu opaque, sudut membulat lewat `SetWindowRgn`.
  Konsekuensi yang diterima: tidak ada blur backdrop dan tepi sudut tidak
  anti-alias (radius 8 px). Keuntungan: tidak ada state komposisi yang bisa lepas,
  dan memori jauh lebih kecil.
- **`D2D1_RENDER_TARGET_TYPE_SOFTWARE` dipaksa.** Dengan tipe `DEFAULT`, Direct2D
  memuat driver D3D11 GPU (private memory 35 MB) hanya untuk kartu 192×48.
  Software membuat private memory turun ke 5 MB tanpa perubahan visual.
- **Fase 2 dulu memakai GDI placeholder**; renderer baru masuk di Fase 3.

## Utang teknis yang diketahui

- Nama monitor masih `Generic PnP Monitor` (perlu baca EDID dari registry). Fase 4.
- Posisi flyout bergantung `Shell_NotifyIconGetRect`; kalau icon masuk overflow area,
  hasilnya kurang presisi. Perlu diuji di Fase 5.
- Belum ada penanganan resume dari sleep (butuh `WM_POWERBROADCAST`). Fase 4.
- Config belum disimpan saat keluar. Fase 4.
- Wheel hanya aktif saat kursor berada di atas flyout (perilaku sama dengan flyout
  volume Windows), jadi tidak bisa diuji jarak jauh tanpa menjaga posisi kursor.

## Cara menjalankan

```
cargo run                # aplikasi tray
cargo run -- probe       # daftar monitor
cargo run -- set 70      # set brightness
cargo run -- stress      # uji coalescing
```

## Langkah berikutnya

Fase 4: multi-monitor (satu baris per monitor), sinkronisasi semua monitor,
persist nilai, autostart, dan nama monitor dari EDID.