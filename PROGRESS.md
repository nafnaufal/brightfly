# Progress

Diperbarui: pasca-fase 5 (rescan thread latar, ikon PNG, perbaikan klik tray,
identitas monitor dari EDID, tutup otomatis saat klik di luar flyout).

## Status

| Fase | Isi | Status |
|---|---|---|
| 0 | Probe DDC/CI (gate) | ✅ lulus |
| 1 | Core library | ✅ selesai |
| 2 | Shell: tray + flyout | ✅ selesai |
| 3 | Visual match flyout volume | ✅ selesai |
| 4 | Fitur (multi-monitor, sync, persist, autostart) | ✅ selesai* |
| 5 | Robustness + packaging | ✅ selesai* |

\* Butir yang tidak bisa diuji di mesin ini didokumentasikan di bagian
"Penyimpangan dari plan" dan "Utang teknis".

## Fase 5 — hasil uji

Ikon dan version info:

- `scripts/make-icon.ps1` menghasilkan `assets/brightfly.ico` (48.496 byte) dengan
  8 ukuran (16/20/24/32/40/48/64/256). Ukuran 16–64 disimpan sebagai DIB 32-bit;
  frame 256 disimpan sebagai PNG (5.474 byte). Frame PNG diverifikasi dengan
  mengekstraknya langsung dari ICO, karena `System.Drawing.Icon.ToBitmap()`
  tidak mendukung frame PNG.
- `build.rs` mencari `rc.exe` (env `RC`, Windows SDK, lalu PATH) dan
  mengompilasi resource dengan icon ID `1` + `VERSIONINFO`. Kalau `rc.exe`
  tidak ada, build tetap jalan dengan warning dan tray memakai icon bawaan.
- `Tray` memuat icon lewat `LoadImageW` pada ukuran `SM_CXSMICON`; fallback
  `IDI_APPLICATION`. Icon hasil resource terlihat benar di taskbar.
- File properties exe: `FileDescription=Brightfly`, `FileVersion=0.1.0`,
  `ProductVersion=0.1.0`.

Packaging:

- `scripts/package.ps1` → `dist/brightfly-0.1.0-win64.zip` (145.567 byte),
  berisi `brightfly.exe` (310.272 byte) dan `README.md`. Zip sedikit lebih besar
  dari versi sebelumnya meski exe jauh lebih kecil, karena resource PNG dan
  kode sudah sulit dipadatkan lagi.
- Zip diekstrak ke folder lain dan exe dijalankan: ✅ tray muncul,
  `--version` mencetak `brightfly 0.1.0`.

| Uji | Metode | Hasil |
|---|---|---|
| Ukuran exe | `target\release\brightfly.exe` | 310.272 byte (0,31 MB; target ≤3 MB) ✅ |
| Start sampai tray terdaftar | 5× start, poll `Shell_NotifyIconGetRect` | median 121–130 ms (120–177; target ≤300 ms) ✅ |
| Waktu buka flyout | `WM_TOGGLE` + `UpdateWindow`, 10× | median 17,7 ms (12,3–37,4; target ≤60 ms) ✅ |
| Handler `WM_DISPLAYCHANGE` | `SendMessageW` saat flyout terbuka | 0,90 ms (enumerasi pindah ke thread latar) ✅ |
| Handler resume | `WM_POWERBROADCAST` + `PBT_APMRESUMEAUTOMATIC` | 1,36 ms ✅ |
| CPU idle | 60 detik, `TotalProcessorTime`, flyout dipastikan tertutup | 0 ms (target 0%) ✅ |
| Working set idle | 60 detik setelah start | 10,6 MB WS, 1,6 MB private, 2 thread ✅ |
| Working set flyout terbuka | 700 ms setelah show | 22,8 MB WS, 5,1 MB private, 9 thread (WS target ≤25 MB) ✅ |
| CPU drag | 150 gerakan, 2,5 detik | <15,6 ms CPU (di bawah granularitas) ✅ |
| Drag cepat / coalescing | `brightfly stress` | 200 perubahan dalam 305 ms, selisih akhir 0 poin ✅ |
| Build tanpa `rc.exe` | jalur fallback di `build.rs` | belum diuji langsung; fallback memakai icon bawaan Windows |
| Monitor lambat / macet | — | tidak bisa diuji (hanya satu monitor, tidak ada monitor bermasalah) |
| Sleep/resume nyata | — | tidak bisa diuji di sesi ini; pesan resume disimulasikan di Fase 4 |
| Monitor dimatikan saat aplikasi jalan | — | tidak bisa diuji (tidak ada monitor cadangan) |
| HDR on/off | — | tidak bisa diuji (monitor uji tanpa HDR) |
| Unplug/replug fisik | — | tidak bisa diuji (satu monitor, dipakai untuk sesi ini) |
| Resolusi berubah nyata | — | tidak bisa diuji tanpa mengubah display aktif; `WM_DISPLAYCHANGE` disimulasikan di Fase 4 |

Catatan pengukuran: start dihitung dari `Start-Process` sampai
`Shell_NotifyIconGetRect` berhasil, jadi termasuk pembuatan proses. Jumlah
thread berfluktuasi tergantung waktu sampel (thread helper DDC/WARP bisa
sudah berhenti); angka idle 2 dan flyout 8 adalah sampel 60 detik/700 ms.

## Pasca-fase 5 — rescan di thread latar

Sebelumnya `WM_DISPLAYCHANGE` dan resume memanggil `ddc::enumerate()` langsung
di UI thread: monitor yang lambat bisa membekukan UI beberapa detik. Sekarang:

- `Supervisor::apply_scan()` hanya memasang hasil enumerasi; tidak ada I/O.
- Controller memakai `mpsc` + thread `brightfly-scan`; thread mengirim
  `Vec<RawMonitor>` lalu `WM_RESCAN_DONE` ke window controller.
- Permintaan yang datang saat scan berjalan hanya menandai `rescan_pending`;
  hasil pertama diterapkan, lalu scan diulang sekali dengan keadaan terbaru.
- Resume tetap memanggil `refresh_all()` setelah scan terakhir.
- Nilai baris dipertahankan dengan prioritas: nilai yang sedang tampil →
  config → bacaan hardware. Tiga unit test baru mengunci urutan ini.
- Enumerasi awal saat start sengaja tetap sinkron, supaya klik tray pertama
  tidak menampilkan flyout kosong. Tray sudah terdaftar sebelum enumerasi
  dimulai, jadi icon tetap muncul lebih dulu.

| Uji | Metode | Hasil |
|---|---|---|
| Handler display change | `SendMessageW(WM_DISPLAYCHANGE)` saat flyout terbuka | 0,90 ms; flyout langsung tertutup ✅ |
| Buka setelah scan | tunggu 700 ms, buka flyout, klik 50% | visible, monitor 50% ✅ |
| Dua display change beruntun | kirim dua pesan berjarak 80 ms, tunggu 900 ms, klik 60% | scan diulang, monitor 60% ✅ |
| Resume | kirim `PBT_APMRESUMEAUTOMATIC`, buka, klik 30% | visible, monitor 30%, config ikut ✅ |
| Unit test | `cargo test` | 16 lulus (3 test prioritas nilai baru) ✅ |
| Monitor benar-benar lambat | — | tidak bisa diuji; tidak ada monitor yang menggantung |

## Perbaikan — klik tray tidak membuka flyout

Gejala: satu klik kiri pada icon tray tidak menampilkan flyout sama sekali.

Penyebab ditemukan lewat log sementara di wndproc tray: dengan
`NOTIFYICON_VERSION_4`, satu klik kiri menghasilkan **tiga** pesan berurutan:

```
event=0x0201 (WM_LBUTTONDOWN)
event=0x0202 (WM_LBUTTONUP)
event=0x0400 (NIN_SELECT)
```

Handler lama memproses ketiganya sebagai `toggle()`, sehingga flyout
buka → tutup → buka/tutup lagi dan akhirnya tertahan oleh guard
`hidden_recently` di flyout. Klik kanan punya masalah serupa:
`WM_RBUTTONUP` lalu `WM_CONTEXTMENU`.

Perbaikan:

- `WM_TRAY` hanya menangani `NIN_SELECT` dan `NIN_KEYSELECT` → `toggle()`.
- `WM_CONTEXTMENU` saja yang membuka menu.
- Konstanta `NIN_KEYSELECT` didefinisikan lokal (`NIN_SELECT + 1`) karena crate
  `windows` 0.62 tidak mengekspornya. Tanpa konstanta itu, pola match menjadi
  binding variabel baru dan memicu `E0408`.
- Log debug sementara dihapus.

| Uji | Metode | Hasil |
|---|---|---|
| Klik kiri 1 | klik fisik via `mouse_event` pada icon tray | flyout terbuka, 192×48 ✅ |
| Klik kiri 2 | klik fisik lagi | flyout tertutup ✅ |
| Klik kiri 3 | klik fisik lagi | flyout terbuka ✅ |
| Klik kanan | klik fisik kanan | menu muncul (`#32768`), flyout tetap tertutup ✅ |
| Escape | `keybd_event(VK_ESCAPE)` | menu tertutup ✅ |
| Build release | ulangi seluruh urutan di atas pada exe release | hasil sama ✅ |
| Unit test | `cargo test` | 16 lulus ✅ |

## Identitas monitor dari EDID

Sebelumnya kunci config adalah device interface path
(`\\?\DISPLAY#MSI30B9#7&21ebfe3c&0&UID24646#{...}`). Bagian instance di dalam
path dibuat Windows per koneksi, jadi kabel yang pindah port membuat nilai
tersimpan tidak ditemukan dan monitor kembali ke bacaan hardware.

Bukti di mesin uji: registry punya **dua** entri untuk monitor yang sama —

```
MSI30B9\7&21ebfe3c&0&UID24646   EDID serial 'PB9H304C03601'
MSI30B9\7&b7543e2&1&UID4165    EDID serial 'PB9H304C03601'
```

Perubahan:

- Modul baru `src/monitor/edid.rs` membaca blob EDID dari
  `HKLM\SYSTEM\CurrentControlSet\Enum\<class>\<hardware id>\<instance>\Device Parameters`
  (path diturunkan dari device interface path), lalu mengambil kode pabrikan,
  kode produk, dan nomor seri.
- Nomor seri diprioritaskan dari descriptor 0x0F (teks), lalu angka di header
  EDID. Karakter yang merusak nama section INI (`[`, `]`, `#`) diganti `_`.
- Kunci config menjadi `MSI30B9:PB9H304C03601`. Kalau EDID tidak terbaca,
  device path tetap dipakai sebagai fallback (perilaku lama).
- `MonitorInfo` menyimpan `path` terpisah dari `id`, khusus untuk migrasi.
- Migrasi config lama berjalan saat start: nilai dari section lama dipindahkan
  ke kunci EDID hanya kalau tidak ambigu — satu section lama dan satu monitor
  aktif dengan kode model yang sama. Selain itu dibiarkan, supaya dua unit
  identik tidak saling menimpa nilai.
- Dua monitor dengan EDID identik (model sama, tanpa nomor seri) mendapat
  akhiran `#2`, `#3` agar tetap punya baris sendiri.

| Uji | Metode | Hasil |
|---|---|---|
| Baca EDID nyata | `read()` + `parse()` pada registry mesin uji | `MSI30B9`, serial `PB9H304C03601` ✅ |
| `probe` | `brightfly probe` | `id: MSI30B9:PB9H304C03601` ✅ |
| Migrasi dari port yang sekarang | config lama `UID24646`, brightness 55 | `[MSI30B9:PB9H304C03601] brightness=55` ✅ |
| Migrasi dari port lain | config `UID4165` brightness 42, monitor di `UID24646` | monitor jadi 42%, config ditulis ulang ke kunci EDID ✅ |
| Slider menulis kunci baru | buka flyout, klik 66% | `brightness=66` di section EDID ✅ |
| Klik tray (regresi) | klik kiri 1/2/3 + klik kanan | buka/tutup/buka, menu muncul ✅ |
| Unit test | `cargo test` | 29 lulus (8 EDID + 5 migrasi baru) ✅ |
| Start setelah tambah pembacaan registry | 5× start | median 121 ms (+4 ms) ✅ |

## Menutup flyout saat klik di luar

Permintaan: flyout harus tertutup otomatis saat user mengklik window lain atau
area di luar kotak.

Diagnosis: handler `WM_ACTIVATE(WA_INACTIVE)` dan `WM_KILLFOCUS` sudah ada,
tetapi keduanya **tidak pernah terpicu** saat window lain diklik. Log sementara
di wndproc flyout mencatat urutan ini saat flyout tampil —

```
WM_ACTIVATEAPP(1)  WM_NCACTIVATE  WM_ACTIVATE(1)  WM_SETFOCUS
```

— lalu tidak ada pesan apa pun ketika desktop diklik. `WM_KILLFOCUS` hanya
datang saat flyout sendiri disembunyikan. Penyebabnya: `force_foreground()`
memaksa window menjadi foreground lewat `AttachThreadInput`, sehingga state
aktivasinya tidak normal.

Perbaikan:

- `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` dipasang **hanya selama flyout
  tampil** dan dilepas di `hide_now()` serta `WM_DESTROY`. Tidak ada polling
  dan tidak ada hook mouse/keyboard, jadi idle tetap 0% CPU.
- Callback hook tidak menutup window langsung; ia hanya `PostMessage`
  `WM_OUTSIDE_FOREGROUND` (`WM_APP + 6`) ke window flyout.
- Handler memeriksa `IsWindowVisible`, `ACTIVATION_GRACE` (200 ms), dan
  memastikan foreground bukan flyout itu sendiri.
- `WINEVENT_SKIPOWNPROCESS` mencegah event dari proses sendiri (mis. saat
  flyout memaksa dirinya foreground) ikut menutup flyout.
- `WM_KILLFOCUS` dipertahankan sebagai jaring pengaman.
- Tambahan yang ditemukan saat uji: setelah flyout pernah ditutup oleh klik
  luar, `force_foreground()` tidak memberi fokus keyboard, sehingga Escape mati.
  `SetFocus(hwnd)` sekarang dipanggil setelah `SetForegroundWindow`.

| Uji | Metode | Hasil |
|---|---|---|
| Klik desktop | buka flyout, klik (300,300) | tertutup < 250 ms ✅ |
| Klik tombol Start | buka flyout, klik Start | tertutup ✅ |
| Alt+Tab | buka flyout, Alt+Tab | tertutup ✅ |
| Drag slider | drag 20 langkah di track | tetap terbuka, brightness berubah ✅ |
| Escape | buka → tutup lewat klik luar → buka → Escape | tertutup ✅ |
| Klik tray 1/2/3 | klik kiri berurutan | buka/tutup/buka ✅ |
| Klik kanan | klik kanan icon tray | menu muncul, flyout tetap tertutup ✅ |
| Build release | seluruh urutan di atas pada exe release | sama ✅ |
| Idle setelah hook | 60 detik | 0 ms CPU, 10,6 MB WS, 2 thread ✅ |
| Unit test | `cargo test` | 29 lulus ✅ |
| Clippy | `cargo clippy --all-targets` | 7 warning, semuanya pre-existing ✅ |

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
- **Icon dan version info ditanam lewat `build.rs`, bukan crate tambahan.** `rc.exe`
  dicari di Windows SDK; resource hasil kompilasi diteruskan dengan
  `cargo:rustc-link-arg`. Tidak ada dependency build baru dan build tetap jalan
  (dengan warning) di mesin tanpa Windows SDK.
- **Distribusi berupa zip portable.** PLAN mengizinkan zip saja; NSIS/WiX tidak tersedia
  di mesin build dan installer tidak menambah nilai untuk aplikasi per-user tanpa service.
- **Enumerasi monitor saat display change/resume berjalan di thread latar.** Probe DDC
  bisa memblokir, jadi hasilnya dikirim lewat `mpsc` dan dipasang lewat
  `WM_RESCAN_DONE`; permintaan beruntun di-coalesce menjadi satu scan ulang. Enumerasi
  awal saat start tetap sinkron supaya flyout pertama tidak kosong.
- **Frame 256 ikon disimpan sebagai PNG di dalam ICO.** Windows mendukungnya dan
  ukurannya turun dari 270.376 ke 5.474 byte; frame kecil tetap DIB agar kompatibel
  dengan shell lama.

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
- **Budget thread ≤6 terlampaui saat flyout terbuka (8 thread).** Idle hanya 2 thread,
  tetapi membuat `ID2D1DCRenderTarget` memuat WARP/D2D helper thread. Menghapusnya berarti
  kembali ke GDI dan kehilangan antialiasing teks/ bentuk. Working set tetap 22,7 MB
  (target ≤25 MB) dan CPU idle 0%, jadi penyimpangan ini diterima dan dicatat.

## Utang teknis yang diketahui

- **Scroll di atas icon tray tidak diimplementasikan** (keputusan produk); lihat
  penyimpangan di atas.
- **Enumerasi awal saat start masih berjalan di UI thread** sebelum message loop;
  ini disengaja agar flyout pertama tidak kosong. Display change dan resume sudah
  memakai thread latar. Monitor yang benar-benar menggantung saat scan belum bisa
  diuji di mesin ini.
- **Sinkronisasi dua monitor nyata dan unplug/replug fisik belum diuji** karena mesin uji
  hanya punya satu monitor.
- **Nama monitor panjang dipotong di kartu 192 px** (satu baris, tanpa tooltip).
- **Monitor tanpa nomor seri di EDID hanya dikenali sampai tingkat model.** Dua unit
  identik akan berbagi nilai; kalau tidak ada serial sama sekali, nilai bisa bertukar
  saat urutan enumerasi berubah. Monitor uji punya serial, jadi kasus ini belum diuji.
- **Migrasi config lama melewati monitor yang ambigu.** Kalau ada dua section lama
  dengan model sama, nilai tidak dipindahkan (lebih baik kehilangan nilai daripada
  salah tempel).
- **`method` di config belum dipakai untuk memaksa metode.** Probe selalu menentukan
  DDC vs gamma; config hanya menyimpan hasilnya.
- **`step` hanya bisa diubah lewat file config**, belum ada kontrol di UI.
- Posisi flyout bergantung `Shell_NotifyIconGetRect`; kalau icon masuk overflow area,
  hasilnya kurang presisi dan belum diuji.
- Wheel hanya aktif saat kursor berada di atas flyout (perilaku sama dengan flyout
  volume Windows), jadi tidak bisa diuji jarak jauh tanpa menjaga posisi kursor.
- **Hanya tersedia paket zip, belum ada installer/uninstaller.** Autostart harus
  dimatikan lewat menu sebelum exe dihapus.

## Cara menjalankan

```
cargo run                # aplikasi tray
cargo run -- probe       # daftar monitor
cargo run -- set 70      # set brightness
cargo run -- stress      # uji coalescing
```

Build dan paket rilis:

```powershell
cargo build --release
.\scripts\package.ps1
```

## Langkah berikutnya

Fase 0–5 dan hardening pasca-fase 5 selesai. Pekerjaan lanjutan yang masuk akal:
installer/uninstaller, serta pengujian di mesin multi-monitor, HDR, dan monitor
yang lambat/menggantung. Uji pindah port fisik sudah bisa dilakukan kapan saja:
config harus tetap memakai section EDID yang sama.