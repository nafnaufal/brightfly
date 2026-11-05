# Brightfly

Brightness slider for external monitors on Windows 11, with a flyout that
mirrors the Windows volume flyout. Written with native Rust + Win32 and
Direct2D: no WebView, no XAML runtime, no large dependencies.

- exe size ± 0.31 MB (including the multi-resolution icon)
- Native `setup.exe` installer ± 0.61 MB, per-user, no admin
- Idle ± 10.6 MB working set, 2 threads, 0% CPU
- DDC/CI VCP `0x10`; gamma ramp as fallback
- One slider row per monitor; optional sync
- Monitor rescan on a background thread after display changes or resume
- Per-monitor values are keyed from EDID, so they survive moving the cable to another port
- Brightness values and settings are stored in `%APPDATA%\Brightfly\config.ini`
- Per-user autostart through `HKCU\...\CurrentVersion\Run`

Ready-to-use downloads are on the [Releases](https://github.com/nafnaufal/brightfly/releases) page.

## Installation

Extract `brightfly-<version>-win64.zip`, then run `setup.exe`. This native
per-user installer does not need admin: it copies the app to
`%LOCALAPPDATA%\Programs\Brightfly`, creates a Start Menu shortcut, registers
an entry in Settings → Apps, and enables autostart.

```powershell
.\setup.exe                  # install + autostart + launch
.\setup.exe --no-autostart   # without autostart
.\setup.exe --no-start       # without launching immediately
.\setup.exe --quiet          # no dialogs
```

To remove it, run `uninstall.exe` (in the installation folder) or use
Settings → Apps. Config in `%APPDATA%\Brightfly` is preserved; add
`--purge-config` to delete it as well.

`brightfly.exe` can still be run directly as a portable build with no
installation.

## Usage

Run `brightfly.exe`. A sun icon appears in the tray area.

| Action | Result |
|---|---|
| Left-click the tray icon | Open/close the flyout |
| Drag the thumb or click the track | Change monitor brightness |
| Left/right or up/down arrows while the flyout has focus | Adjust by `step` |
| Mouse wheel over the flyout | Adjust by `step` |
| `Esc` | Close the flyout and save the value |
| Click outside the flyout / another window | Close the flyout and save the value |
| Right-click the tray icon | Sync, autostart, and quit menu |

Launching the exe a second time does not create another instance; the existing
instance opens its flyout.

## Command line

```
brightfly probe          List monitors, method, range, and current value
brightfly set <0..100>   Set every monitor, then read back to verify
brightfly stress         Send 200 rapid changes to exercise coalescing
brightfly --version
```

## Build

Requirements:

- Rust stable with the MSVC target (`rustup default stable-msvc`)
- Visual Studio Build Tools with the C++ workload (for `link.exe`)
- Windows SDK (optional, for `rc.exe`: icon + version info). Without it the
  build still succeeds with a warning and the tray falls back to the stock
  Windows icon.

```powershell
cargo build --release
.\target\release\brightfly.exe
```

Running straight from source:

```powershell
cargo run                 # tray app
cargo run -- probe        # list monitors
cargo run -- set 70       # set brightness
```

Stop any existing instance first (`Get-Process brightfly | Stop-Process -Force`)
before `cargo run`; a second instance only opens the flyout of the first one.

`cargo test` runs the unit tests for the config parser, gamma, and flyout
layout.

## Packaging

```powershell
.\scripts\package.ps1
```

Produces `dist\brightfly-<version>-win64.zip` containing `brightfly.exe`
(portable), `setup.exe`, and the README. `setup.exe` embeds the exe + README
payload, so run this script for every new release. Installation and
uninstallation do not use admin; both only write to HKCU and user folders.

## GitHub releases

Push a tag that matches the version in `Cargo.toml`:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

The `.github/workflows/release.yml` workflow runs the tests, builds the
package, and creates a GitHub Release containing:

- `brightfly-<version>-win64.zip` (full zip)
- `brightfly-<version>-setup.exe`
- `brightfly-<version>.exe` (portable)
- `SHA256SUMS.txt`

To build without creating a release (for example, to check CI), run the
workflow through **Actions → Release → Run workflow**. The result is available
as an artifact.

The tag must be exactly `v<version>`; otherwise the workflow fails before the
release is created.

Regenerate the icon with:

```powershell
.\scripts\make-icon.ps1
```

## Known limitations

- Scrolling over the tray icon is not supported. Windows 11 does not forward
  `WM_MOUSEWHEEL` to third-party tray icon callbacks; implementing it would
  require raw input or a global hook that adds idle overhead.
- Monitors without DDC/CI support still appear, but their slider is disabled
  when no gamma ramp is available either.
- Brightness over DDC/CI depends on monitor firmware. Some monitors respond
  slowly or reject VCP `0x10`.
- HDR, physical sleep/resume, unplug/replug, and real multi-monitor setups have
  not been tested thoroughly; the development machine has only one external
  monitor.
- Per-monitor keys come from the EDID serial number. Monitors that do not
  report a serial are only identified at the model level, so two identical
  units share the same value.
- Release builds are not code-signed. SmartScreen may warn; verify file hashes
  against `SHA256SUMS.txt`.

## License

MIT. See [LICENSE](LICENSE).