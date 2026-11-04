# Progress

Diperbarui: fase 4 selesai.

## Status

| Fase | Isi | Status |
|---|---|---|
| 0 | Probe DDC/CI (gate) | ✅ lulus |
| 1 | Core library | ✅ selesai |
| 2 | Shell: tray + flyout | ✅ selesai |
| 3 | Visual match flyout volume | ✅ selesai |
| 4 | Fitur (multi-monitor, sync, persist, autostart) | ✅ selesai* |
| 5 | Robustness + packaging | ⬜ belum |

\* Semua butir Fase 4 selesai kecuali scroll di atas icon tray; lihat
"Penyimpangan dari plan" dan "Utang teknis".

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

## Fase 4 — hasil uji

| Uji | Metode | Hasil |
|---|---|---|
| Nama dari EDID | `probe` | ✅ `MSI MP223 E2` (sebelumnya `Generic PnP Monitor`) |
| Startup menerapkan config | config 50%, monitor sedang 80% | ✅ monitor jadi 50% setelah aplikasi start |
| Persist saat slider bergerak | klik 30%, tunggu debounce 1,2 dtk | ✅ `brightness=30` di config |
| Persist saat flyout ditutup | set 80%, `Esc` 100 ms kemudian, cek 250 ms | ✅ `brightness=80` (flush langsung, tanpa menunggu debounce) |
| Persist setelah restart | restart proses | ✅ `probe` membaca nilai terakhir dari config |
| Autostart | menu → "Mulai bersama Windows" | ✅ nilai Run `Brightfly` berisi path exe berkutip |
| Autostart nonaktif | toggle item yang sama | ✅ nilai Run dihapus, config ikut 0 |
| Menu sync | toggle item pertama | ✅ config `sync` berubah 1↔0 |
| Langkah dari config | `step=5`, panah kanan | ✅ 80% → 85% |
| `WM_DISPLAYCHANGE` | kirim pesan saat flyout terbuka | ✅ flyout tertutup, rescan, slider tetap bekerja |
| Resume | `WM_POWERBROADCAST` + `PBT_APMRESUMEAUTOMATIC` | ✅ flyout tertutup, rescan + refresh, slider tetap bekerja |
| Dua baris + nama | render dua baris (monitor nyata + baris uji sementara) | ✅ kartu 192×128, nama 12 px di atas tiap slider |
| Single instance | jalankan exe kedua | ✅ flyout instance lama terbuka |
| Memori & thread (release) | idle / flyout terbuka | 10,5 MB WS, 5 thread / 22,6 MB WS, 9 thread (private 5,0 MB) |

Catatan: mesin uji hanya punya satu monitor fisik, jadi sinkronisasi dua monitor
nyata dan unplug/replug fisik belum diuji; pesan display-change dan resume
disimulasikan langsung ke controller. Baris ganda diverifikasi dengan
menambahkan satu baris uji sementara saat pengambilan screenshot, lalu kode uji
itu dihapus.

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
- **Sync memperbarui baris flyout secara lokal.** `supervisor.monitors()` masih berisi nilai
  lama sampai worker DDC selesai, jadi `Flyout::set_all_pct()` dipanggil langsung agar semua
  baris bergerak bersamaan tanpa menunggu hardware.
- **Config disimpan dengan debounce 500 ms** (`SetTimer`), lalu di-flush saat flyout ditutup
  dan saat `WM_DESTROY`. Menulis file tiap perubahan slider akan membuat I/O berlebihan.
- **Registry adalah sumber kebenaran autostart saat start.** Nilai `start_with_windows` di
  config tidak dipakai untuk menulis registry otomatis; menu hanya membaca/menulis registry
  ketika pengguna memilihnya.
- **Nilai terakhir juga disimpan di `Supervisor`.** Saat monitor hilang lalu muncul lagi,
  fallback-nya adalah nilai terakhir yang diketahui, bukan default 50%.
- **Nama monitor diambil dari `DISPLAYCONFIG_TARGET_DEVICE_NAME`.** `EnumDisplayDevices`
  hanya memberi "Generic PnP Monitor"; sumber GDI dihubungkan ke target lewat
  `DISPLAYCONFIG_SOURCE_DEVICE_NAME`.
- **Label nama hanya muncul saat baris > 1.** Tinggi tiap baris bertambah 16 DIP untuk pita
  nama, sesuai PLAN bagian 3.1; flyout satu monitor tetap 192×48 seperti flyout volume.

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
- **Scroll di atas icon tray tidak diimplementasikan (keputusan produk).** Windows 11 tidak
  meneruskan `WM_MOUSEWHEEL` ke callback tray icon pihak ketiga: diuji dengan
  `NOTIFYICON_VERSION_4` maupun versi 3, taskbar tetap mengonsumsi wheel. Untuk
  mewujudkannya perlu `RegisterRawInputDevices` + `RIDEV_INPUTSINK` atau
  `WH_MOUSE_LL`, yang membuat UI thread terus dibangunkan oleh gerakan mouse dan
  bertentangan dengan target "sangat ringan". Diputuskan dilewati; scroll tetap
  berfungsi saat kursor berada di atas flyout.

## Utang teknis yang diketahui

- **Scroll di atas icon tray tidak diimplementasikan** (keputusan produk); lihat
  penyimpangan di atas.
- **Enumerasi + probe masih berjalan di UI thread** saat start, `WM_DISPLAYCHANGE`, dan
  resume. Dengan banyak monitor atau monitor yang lambat, flyout bisa tertunda sesaat.
  Kandidat perbaikan: pindahkan rescan ke thread terpisah di Fase 5.
- **Sinkronisasi dua monitor nyata dan unplug/replug fisik belum diuji** karena mesin uji
  hanya punya satu monitor.
- **Nama monitor panjang dipotong di kartu 192 px** (satu baris, tanpa tooltip).
- **Device path berubah kalau kabel pindah port**, sehingga nilai tersimpan tidak
  ditemukan dan monitor kembali ke nilai bacaannya saat itu.
- **`method` di config belum dipakai untuk memaksa metode.** Probe selalu menentukan
  DDC vs gamma; config hanya menyimpan hasilnya.
- **`step` hanya bisa diubah lewat file config**, belum ada kontrol di UI.
- Posisi flyout bergantung `Shell_NotifyIconGetRect`; kalau icon masuk overflow area,
  hasilnya kurang presisi. Perlu diuji di Fase 5.
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

Fase 5: robustness (sleep/resume nyata, monitor dimatikan, HDR, drag cepat, resolusi
berubah), packaging portable/installer, dan ikon aplikasi.