# Brightfly

Slider brightness untuk monitor eksternal di Windows 11, dengan tampilan
flyout yang meniru flyout volume Windows. Ditulis dengan Rust + Win32 native
dan Direct2D, tanpa WebView, tanpa runtime XAML, dan tanpa dependency besar.

- Ukuran exe ± 0,31 MB (termasuk icon multi-resolusi)
- Installer native `setup.exe` ± 0,61 MB, per-user tanpa admin
- Idle ± 10,6 MB working set, 2 thread, CPU 0%
- DDC/CI VCP `0x10`; gamma ramp sebagai fallback
- Satu baris slider per monitor; sinkronisasi opsional
- Rescan monitor di thread latar saat display berubah atau resume
- Nilai per monitor dikunci dari EDID, jadi tetap dikenali walau kabel pindah port
- Nilai brightness dan pengaturan disimpan di `%APPDATA%\Brightfly\config.ini`
- Autostart per-user lewat `HKCU\...\CurrentVersion\Run`

## Instalasi

Ekstrak `brightfly-<versi>-win64.zip`, lalu jalankan `setup.exe`. Installer
native per-user ini tidak butuh admin dan menyalin aplikasi ke
`%LOCALAPPDATA%\Programs\Brightfly`, membuat shortcut Start Menu, mendaftarkan
entri di Settings → Apps, dan mengaktifkan autostart.

```powershell
.\setup.exe                  # install + autostart + jalankan
.\setup.exe --no-autostart   # tanpa autostart
.\setup.exe --no-start       # tanpa langsung menjalankan
.\setup.exe --quiet          # tanpa dialog
```

Untuk melepas, jalankan `uninstall.exe` (ada di folder instalasi), atau lewat
Settings → Apps. Config di `%APPDATA%\Brightfly` dipertahankan; tambahkan
`--purge-config` kalau ingin menghapusnya juga.

`brightfly.exe` tetap bisa dijalankan langsung sebagai versi portable tanpa
instalasi.

## Pemakaian

Jalankan `brightfly.exe`. Icon matahari muncul di area tray.

| Aksi | Hasil |
|---|---|
| Klik kiri icon tray | Buka/tutup flyout |
| Drag thumb atau klik track | Ubah brightness monitor |
| Panah kiri/kanan, atas/bawah saat flyout fokus | Ubah sesuai `step` |
| Wheel di atas flyout | Ubah sesuai `step` |
| `Esc` | Tutup flyout dan simpan nilai |
| Klik di luar flyout / window lain | Tutup flyout dan simpan nilai |
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

Butuh:

- Rust stable dengan target MSVC (`rustup default stable-msvc`)
- Visual Studio Build Tools dengan komponen C++ (untuk `link.exe`)
- Windows SDK (opsional untuk `rc.exe`: icon + version info). Tanpa itu build
  tetap jalan dengan warning dan tray memakai icon bawaan Windows.

```powershell
cargo build --release
.\target\release\brightfly.exe
```

Menjalankan langsung dari source:

```powershell
cargo run                 # aplikasi tray
cargo run -- probe        # daftar monitor
cargo run -- set 70       # set brightness
```

Hentikan instance lama dulu (`Get-Process brightfly | Stop-Process -Force`)
sebelum `cargo run`; instance kedua hanya akan membuka flyout instance lama.

`cargo test` menjalankan unit test parser config, gamma, dan layout flyout.

## Packaging

```powershell
.\scripts\package.ps1
```

Menghasilkan `dist\brightfly-<versi>-win64.zip` berisi `brightfly.exe`
(portable), `setup.exe`, dan README. `setup.exe` dibangun dengan payload exe +
README tertanam, jadi jalankan script ini setiap kali merilis versi baru.
Instalasi dan uninstall tidak memakai admin; keduanya hanya menulis ke HKCU
dan folder user.

## Rilis GitHub

Push tag yang sama dengan versi di `Cargo.toml`:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

Workflow `.github/workflows/release.yml` akan menjalankan test, build, lalu
membuat GitHub Release berisi:

- `brightfly-<versi>-win64.zip` (zip lengkap)
- `brightfly-<versi>-setup.exe`
- `brightfly-<versi>.exe` (portable)
- `SHA256SUMS.txt`

Untuk build tanpa membuat release (mis. mengecek CI), jalankan workflow lewat
**Actions → Release → Run workflow**. Hasilnya tersedia sebagai artifact.

Tag harus persis `v<versi>`; kalau tidak cocok, workflow gagal sebelum
release dibuat.

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
- Build rilis tidak ditandatangani secara digital. SmartScreen bisa
  menampilkan peringatan; cocokkan hash file dengan `SHA256SUMS.txt`.

## Lisensi

MIT. Lihat [LICENSE](LICENSE).
