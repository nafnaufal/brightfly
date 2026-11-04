# Progress

Diperbarui: fase 2 selesai.

## Status

| Fase | Isi | Status |
|---|---|---|
| 0 | Probe DDC/CI (gate) | ✅ lulus |
| 1 | Core library | ✅ selesai |
| 2 | Shell: tray + flyout | ✅ selesai |
| 3 | Visual match flyout volume | ⬜ belum |
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

- Fase 2 memakai **GDI placeholder**, bukan Direct2D. Renderer Direct2D + DirectComposition
  dipindah ke Fase 3 karena butuh swapchain alpha yang memang untuk acrylic.
  Akibatnya window belum `WS_EX_NOREDIRECTIONBITMAP`; itu ikut berubah di Fase 3.

## Utang teknis yang diketahui

- Nama monitor masih `Generic PnP Monitor` (perlu baca EDID dari registry). Fase 4.
- Posisi flyout bergantung `Shell_NotifyIconGetRect`; kalau icon masuk overflow area,
  hasilnya kurang presisi. Perlu diuji di Fase 5.
- Belum ada penanganan resume dari sleep (butuh `WM_POWERBROADCAST`). Fase 4.
- Config belum disimpan saat keluar. Fase 4.

## Cara menjalankan

```
cargo run                # aplikasi tray
cargo run -- probe       # daftar monitor
cargo run -- set 70      # set brightness
cargo run -- stress      # uji coalescing
```

## Langkah berikutnya

Fase 3: DirectComposition + swapchain alpha, backdrop acrylic, slider, animasi, tema.