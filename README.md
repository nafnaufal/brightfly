# Brightfly

Slider brightness untuk monitor eksternal di Windows 11, dengan tampilan
flyout yang meniru flyout volume Windows. Ditulis dengan Rust + Win32 native
dan Direct2D, tanpa WebView, tanpa runtime XAML, dan tanpa dependency besar.

- Ukuran exe ± 0,30 MB (termasuk icon multi-resolusi)
- Idle ± 10,6 MB working set, 2 thread, CPU 0%
- DDC/CI VCP `0x10`; gamma ramp sebagai fallback
- Satu baris slider per monitor; sinkronisasi opsional
- Rescan monitor di thread latar saat display berubah atau resume
- Nilai per monitor dikunci dari EDID, jadi tetap dikenali walau kabel pindah port
- Nilai brightness dan pengaturan disimpan di `%APPDATA%\Brightfly\config.ini`
- Autostart per-user lewat `HKCU\...\CurrentVersion\Run`

## Pemakaian

Jalankan `brightfly.exe`. Icon matahari muncul di area tray.

| Aksi | Hasil |
|---|---|
| Klik kiri icon tray | Buka/tutup flyout |
| Drag thumb atau klik track | Ubah brightness monitor |
| Panah kiri/kanan, atas/bawah saat flyout fokus | Ubah sesuai `step` |
| Wheel di atas flyout | Ubah sesuai `step` |
| `Esc` | Tutup flyout dan simpan nilai |
| Klik kanan icon tray | Menu sync, autostart, keluar |

Menjalankan exe untuk kedua kalinya tidak membuat instance baru; instance yang
ada akan membuka flyout.

## Command line

```
brightfly probe          Tampilkan monitor, metode, range, dan nilai sekarang
brightfly set <0..100>   Set semua monitor lalu baca ulang untuk verifikasi
brightfly stress         Kirim 200 perubahan cepat untuk menguji coalescing
brightfly --version
```

## Build

Butuh Rust toolchain (edition 2024) dan Windows SDK untuk `rc.exe`
(icon + version info). Kalau `rc.exe` tidak ditemukan, build tetap berjalan
tanpa resource dan tray memakai icon bawaan Windows.

```powershell
cargo build --release
.\target\release\brightfly.exe
```

`cargo test` menjalankan unit test parser config, gamma, dan layout flyout.

## Packaging

```powershell
.\scripts\package.ps1
```

Menghasilkan `dist\brightfly-<versi>-win64.zip` berisi exe dan README. Tidak
ada installer (NSIS/WiX tidak diperlukan); cukup ekstrak dan jalankan.

Icon dibuat ulang dengan:

```powershell
.\scripts\make-icon.ps1
```

## Batasan yang diketahui

- Scroll di atas icon tray tidak didukung. Windows 11 tidak meneruskan
  `WM_MOUSEWHEEL` ke callback icon tray pihak ketiga; mewujudkannya butuh
  raw input/hook global yang menambah beban idle.
- Monitor yang tidak mendukung DDC/CI tetap tampil, tetapi slider dinonaktifkan
  bila gamma ramp juga tidak tersedia.
- Brightness melalui DDC/CI bergantung pada firmware monitor. Sebagian monitor
  lambat merespons atau menolak VCP `0x10`.
- HDR, sleep/resume fisik, unplug/replug, dan multi-monitor nyata belum diuji
  menyeluruh; mesin pengembangan hanya memiliki satu monitor eksternal.
- Kunci per monitor diambil dari nomor seri EDID. Monitor yang tidak
  mencantumkan serial hanya dikenali sampai tingkat model, sehingga dua unit
  identik akan berbagi nilai.
