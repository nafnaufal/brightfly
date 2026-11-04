# Progress

Diperbarui: fase 1 selesai.

## Status

| Fase | Isi | Status |
|---|---|---|
| 0 | Probe DDC/CI (gate) | ✅ lulus |
| 1 | Core library | ✅ selesai |
| 2 | Shell: tray + flyout | ⬜ belum |
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

Kesimpulan: **tidak perlu mundur ke gamma-only.** Fallback gamma tetap ada untuk monitor
yang nanti tidak mendukung.

## Yang sudah jalan (fase 1)

Perintah CLI:

```
brightfly probe          # daftar monitor + metode + nilai sekarang
brightfly set <0..100>   # set semua monitor, lalu baca ulang sebagai verifikasi
brightfly stress         # uji coalescing: 200 perubahan cepat
brightfly --version
```

Hasil uji coalescing: 200 perubahan terkirim dalam 381 ms, nilai akhir tepat 62%.
Artinya drag slider tidak akan menumpuk antrian dan tidak memblokir pemanggil.

10 unit test lulus (parser config, render round-trip, perhitungan gamma).

## Keputusan teknis yang diambil

- **Tanpa channel, pakai slot `desired` + Condvar.** Lebih sederhana daripada `mpsc`
  untuk coalescing, dan tidak mungkin menumpuk.
- **Acquire/destroy physical monitor handle per operasi.** Handle lama tidak disimpan,
  jadi unplug/replug tidak meninggalkan handle basi.
- **Join handle tidak di-join saat shutdown.** Proses harus tetap bisa keluar walau ada
  panggilan DDC yang menggantung.
- **`GetDeviceGammaRamp` ada di `Win32::UI::ColorSystem`, bukan `Win32::Graphics::Gdi`**
  (catatan untuk fase berikutnya, sudah ditangani).

## Utang teknis yang diketahui

- Nama monitor masih `Generic PnP Monitor` karena `EnumDisplayDevicesW` tidak memberi nama
  EDID untuk perangkat ini. Perlu baca EDID dari registry (`HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\*\Device Parameters\EDID`)
  memakai `DeviceID` sebagai kunci. Dikerjakan di fase 4 (UI multi-monitor).
- Belum ada penanganan `WM_DISPLAYCHANGE`; `Supervisor::rescan()` sudah tersedia tapi
  belum ada pemanggil karena belum ada message loop.

## Langkah berikutnya

Fase 2: message loop, tray icon, window flyout kosong, single-instance mutex.
