# Progress

Updated: post-phase 5 (background-thread rescan, PNG icon, tray click fix,
monitor identity from EDID, auto-close on outside click of the flyout, fix for
the "100" number, and per-user installer/uninstaller).

## Status

| Phase | Scope | Status |
|---|---|---|
| 0 | DDC/CI probe (gate) | ✅ passed |
| 1 | Core library | ✅ done |
| 2 | Shell: tray + flyout | ✅ done |
| 3 | Visual match to volume flyout | ✅ done |
| 4 | Features (multi-monitor, sync, persist, autostart) | ✅ done* |
| 5 | Robustness + packaging | ✅ done* |

\* Items that could not be tested on this machine are documented in
"Deviations from the plan" and "Known technical debt".

## Phase 5 — test results

Icon and version info:

- `scripts/make-icon.ps1` produces `assets/brightfly.ico` (48.496 byte) with
  8 sizes (16/20/24/32/40/48/64/256). Sizes 16–64 are stored as 32-bit DIB;
  the 256 frame is stored as PNG (5.474 byte). The PNG frame is verified by
  extracting it directly from the ICO, because `System.Drawing.Icon.ToBitmap()`
  does not support PNG frames.
- `build.rs` looks for `rc.exe` (env `RC`, Windows SDK, then PATH) and
  compiles the resources with icon ID `1` + `VERSIONINFO`. If `rc.exe` is
  missing, the build still runs with a warning and the tray uses the built-in icon.
- `Tray` loads the icon via `LoadImageW` at `SM_CXSMICON` size; fallback is
  `IDI_APPLICATION`. The resource icon renders correctly in the taskbar.
- Exe file properties: `FileDescription=Brightfly`, `FileVersion=0.1.0`,
  `ProductVersion=0.1.0`.

Packaging:

- `scripts/package.ps1` → `dist/brightfly-0.1.0-win64.zip` (422.648 byte),
  containing `brightfly.exe` (310.272 byte), `setup.exe` (608.256 byte), and
  `README.md` (5.428 byte). `setup.exe` carries the exe + README inside itself.
- Zip extracted to another folder and the exe run: ✅ tray appears,
  `--version` prints `brightfly 0.1.0`.

| Test | Method | Result |
|---|---|---|
| Exe size | `target\release\brightfly.exe` | 310.272 byte (0.31 MB; target ≤3 MB) ✅ |
| Start until tray registered | 5× start, poll `Shell_NotifyIconGetRect` | median 121–130 ms (120–177; target ≤300 ms) ✅ |
| Flyout open time | `WM_TOGGLE` + `UpdateWindow`, 10× | median 17.7 ms (12.3–37.4; target ≤60 ms) ✅ |
| `WM_DISPLAYCHANGE` handler | `SendMessageW` with the flyout open | 0.90 ms (enumeration moved to the background thread) ✅ |
| Resume handler | `WM_POWERBROADCAST` + `PBT_APMRESUMEAUTOMATIC` | 1.36 ms ✅ |
| Idle CPU | 60 seconds, `TotalProcessorTime`, flyout confirmed closed | 0 ms (target 0%) ✅ |
| Idle working set | 60 seconds after start | 10.6 MB WS, 1.6 MB private, 2 threads ✅ |
| Flyout-open working set | 700 ms after show | 22.8 MB WS, 5.1 MB private, 9 threads (WS target ≤25 MB) ✅ |
| Drag CPU | 150 movements, 2.5 seconds | <15.6 ms CPU (below granularity) ✅ |
| Fast drag / coalescing | `brightfly stress` | 200 changes in 305 ms, final delta 0 points ✅ |
| Build without `rc.exe` | fallback path in `build.rs` | not tested directly; the fallback uses the built-in Windows icon |
| Slow / stalled monitor | — | cannot be tested (only one monitor, no faulty monitor) |
| Real sleep/resume | — | cannot be tested in this session; the resume message is simulated in Phase 4 |
| Monitor powered off while the app runs | — | cannot be tested (no spare monitor) |
| HDR on/off | — | cannot be tested (test monitor has no HDR) |
| Physical unplug/replug | — | cannot be tested (one monitor, in use for this session) |
| Real resolution change | — | cannot be tested without changing the active display; `WM_DISPLAYCHANGE` is simulated in Phase 4 |

Measurement note: start is measured from `Start-Process` until
`Shell_NotifyIconGetRect` succeeds, so it includes process creation. The thread
count fluctuates with sample timing (DDC/WARP helper threads may
already have exited); the idle figure of 2 and the flyout figure of 8 are 60-second/700 ms samples.

## Post-phase 5 — rescan on the background thread

Previously `WM_DISPLAYCHANGE` and resume called `ddc::enumerate()` directly
on the UI thread: a slow monitor could freeze the UI for seconds. Now:

- `Supervisor::apply_scan()` only installs the enumeration result; there is no I/O.
- The controller uses `mpsc` + a `brightfly-scan` thread; the thread sends
  `Vec<RawMonitor>` and then `WM_RESCAN_DONE` to the controller window.
- A request that arrives while a scan runs only sets `rescan_pending`;
  the first result is applied, then the scan repeats once with the latest state.
- Resume still calls `refresh_all()` after the last scan.
- Row values are preserved by priority: currently displayed value →
  config → hardware reading. Three new unit tests lock this order.
- The initial enumeration at start remains synchronous on purpose, so the first tray
  click does not show an empty flyout. The tray is registered before enumeration
  starts, so the icon still appears first.

| Test | Method | Result |
|---|---|---|
| Display change handler | `SendMessageW(WM_DISPLAYCHANGE)` with the flyout open | 0.90 ms; flyout closes immediately ✅ |
| Open after scan | wait 700 ms, open the flyout, click 50% | visible, monitor 50% ✅ |
| Two display changes in a row | send two messages 80 ms apart, wait 900 ms, click 60% | scan repeats, monitor 60% ✅ |
| Resume | send `PBT_APMRESUMEAUTOMATIC`, open, click 30% | visible, monitor 30%, config follows ✅ |
| Unit test | `cargo test` | 16 pass (3 new value-priority tests) ✅ |
| Truly slow monitor | — | cannot be tested; no monitor hangs |

## Fix — tray click does not open the flyout

Symptom: a single left click on the tray icon does not show the flyout at all.

Cause found via temporary logging in the tray wndproc: with
`NOTIFYICON_VERSION_4`, one left click produces **three** messages in sequence:

```
event=0x0201 (WM_LBUTTONDOWN)
event=0x0202 (WM_LBUTTONUP)
event=0x0400 (NIN_SELECT)
```

The old handler processed all three as `toggle()`, so the flyout went
open → close → open/close again and was finally held shut by the
`hidden_recently` guard in the flyout. Right click had a similar problem:
`WM_RBUTTONUP` then `WM_CONTEXTMENU`.

Fix:

- `WM_TRAY` handles only `NIN_SELECT` and `NIN_KEYSELECT` → `toggle()`.
- Only `WM_CONTEXTMENU` opens the menu.
- The `NIN_KEYSELECT` constant is defined locally (`NIN_SELECT + 1`) because the
  `windows` 0.62 crate does not export it. Without that constant, the match pattern becomes
  a new variable binding and triggers `E0408`.
- Temporary debug logging removed.

| Test | Method | Result |
|---|---|---|
| Left click 1 | physical click via `mouse_event` on the tray icon | flyout opens, 192×48 ✅ |
| Left click 2 | physical click again | flyout closes ✅ |
| Left click 3 | physical click again | flyout opens ✅ |
| Right click | physical right click | menu appears (`#32768`), flyout stays closed ✅ |
| Escape | `keybd_event(VK_ESCAPE)` | menu closes ✅ |
| Release build | repeat the entire sequence above on the release exe | same result ✅ |
| Unit test | `cargo test` | 16 pass ✅ |

## Monitor identity from EDID

Previously the config key was the device interface path
(`\\?\DISPLAY#MSI30B9#7&21ebfe3c&0&UID24646#{...}`). The instance part of the
path is created by Windows per connection, so a cable moved to another port makes the
stored value unfindable and the monitor falls back to the hardware reading.

Evidence on the test machine: the registry has **two** entries for the same monitor —

```
MSI30B9\7&21ebfe3c&0&UID24646   EDID serial 'PB9H304C03601'
MSI30B9\7&b7543e2&1&UID4165    EDID serial 'PB9H304C03601'
```

Changes:

- New module `src/monitor/edid.rs` reads the EDID blob from
  `HKLM\SYSTEM\CurrentControlSet\Enum\<class>\<hardware id>\<instance>\Device Parameters`
  (path derived from the device interface path), then extracts the manufacturer code,
  product code, and serial number.
- The serial number is taken preferentially from descriptor 0x0F (text), then the number in the
  EDID header. Characters that break the INI section name (`[`, `]`, `#`) are replaced with `_`.
- The config key becomes `MSI30B9:PB9H304C03601`. If the EDID cannot be read,
  the device path is still used as fallback (old behavior).
- `MonitorInfo` stores `path` separately from `id`, specifically for migration.
- Migration of the old config runs at start: a value from an old section is moved
  to the EDID key only when it is unambiguous — one old section and one active
  monitor with the same model code. Otherwise it is left alone, so two
  identical units do not overwrite each other's values.
- Two monitors with identical EDID (same model, no serial number) get the
  suffix `#2`, `#3` so they still have their own row.

| Test | Method | Result |
|---|---|---|
| Read real EDID | `read()` + `parse()` on the test machine registry | `MSI30B9`, serial `PB9H304C03601` ✅ |
| `probe` | `brightfly probe` | `id: MSI30B9:PB9H304C03601` ✅ |
| Migration from the current port | old config `UID24646`, brightness 55 | `[MSI30B9:PB9H304C03601] brightness=55` ✅ |
| Migration from another port | config `UID4165` brightness 42, monitor on `UID24646` | monitor becomes 42%, config rewritten to the EDID key ✅ |
| Slider writes the new key | open the flyout, click 66% | `brightness=66` in the EDID section ✅ |
| Tray click (regression) | left click 1/2/3 + right click | open/close/open, menu appears ✅ |
| Unit test | `cargo test` | 29 pass (8 EDID + 5 new migration) ✅ |
| Start after adding registry reads | 5× start | median 121 ms (+4 ms) ✅ |

## Closing the flyout on outside click

Request: the flyout must close automatically when the user clicks another window or
an area outside the box.

Diagnosis: the `WM_ACTIVATE(WA_INACTIVE)` and `WM_KILLFOCUS` handlers already exist,
but neither **ever fires** when another window is clicked. Temporary logging
in the flyout wndproc recorded this sequence while the flyout is shown —

```
WM_ACTIVATEAPP(1)  WM_NCACTIVATE  WM_ACTIVATE(1)  WM_SETFOCUS
```

— and then no message at all when the desktop is clicked. `WM_KILLFOCUS` only
arrives when the flyout itself is hidden. The cause: `force_foreground()`
forces the window to the foreground via `AttachThreadInput`, so its activation
state is not normal.

Fix:

- `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` is installed **only while the flyout
  is shown** and removed in `hide_now()` and `WM_DESTROY`. There is no polling
  and no mouse/keyboard hook, so idle CPU stays at 0%.
- The hook callback does not close the window directly; it only `PostMessage`s
  `WM_OUTSIDE_FOREGROUND` (`WM_APP + 6`) to the flyout window.
- The handler checks `IsWindowVisible`, `ACTIVATION_GRACE` (200 ms), and
  confirms the foreground window is not the flyout itself.
- `WINEVENT_SKIPOWNPROCESS` prevents events from the own process (e.g. when
  the flyout forces itself to the foreground) from also closing the flyout.
- `WM_KILLFOCUS` is kept as a safety net.
- Additional finding from testing: once the flyout has been closed by an outside
  click, `force_foreground()` does not give it keyboard focus, so Escape stops working.
  `SetFocus(hwnd)` is now called after `SetForegroundWindow`.

| Test | Method | Result |
|---|---|---|
| Desktop click | open the flyout, click (300.300) | closes in < 250 ms ✅ |
| Start button click | open the flyout, click Start | closes ✅ |
| Alt+Tab | open the flyout, Alt+Tab | closes ✅ |
| Slider drag | drag 20 steps on the track | stays open, brightness changes ✅ |
| Escape | open → close via outside click → open → Escape | closes ✅ |
| Tray click 1/2/3 | consecutive left clicks | open/close/open ✅ |
| Right click | right click on the tray icon | menu appears, flyout stays closed ✅ |
| Release build | entire sequence above on the release exe | same ✅ |
| Idle after the hook | 60 seconds | 0 ms CPU, 10.6 MB WS, 2 threads ✅ |
| Unit test | `cargo test` | 29 pass ✅ |
| Clippy | `cargo clippy --all-targets` | 7 warnings, all pre-existing ✅ |

## Fix — "100" number truncated

Symptom: at brightness 100, the last digit drops to a second line, so the
card shows "10" on top and "0" below.

Cause: only 20 DIP was reserved for the number column, enough for two digits at
15 px font. The text format also let DirectWrite break the line.

Fix in `src/ui/render.rs`:

- `NUMBER_WIDTH_DIP` 20 → 28 DIP (enough for three digits, including padding).
- `SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)` on the number format, so
  numbers never wrap again.

| Test | Method | Result |
|---|---|---|
| Value 100 | flyout screenshot 192×48 at DPI 100% | "100" on one line, right-aligned ✅ |
| Value 5 | flyout screenshot | "5" still on one line, correct position ✅ |
| Card width | screenshot | still 192×48; track does not collide with the number ✅ |
| Unit test | `cargo test` | 29 pass ✅ |

## Native installer and uninstaller (`setup.exe`)

The installer is now a separate Rust binary (`src/bin/setup.rs`, target
`brightfly-setup`). During packaging, `build.rs` embeds `brightfly.exe` and
`README.md` into the binary via `BRIGHTFLY_PAYLOAD_DIR`, then
`scripts/package.ps1` copies it as `setup.exe`. There is no PowerShell,
MSI, or extraction to a temp folder during installation; it stays per-user without admin.

`setup.exe`:

- Copies the payload to `%LOCALAPPDATA%\Programs\Brightfly`, then copies
  itself as `uninstall.exe`.
- Creates the Start Menu shortcut via COM `IShellLinkW` + `IPersistFile`.
- Registers the entry `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Brightfly`
  (`DisplayName`, `DisplayVersion`, `DisplayIcon`, `InstallLocation`,
  `UninstallString`, `QuietUninstallString`, `EstimatedSize`, `NoModify=1`,
  `NoRepair=1`) so it appears in Settings → Apps.
- Autostart via the Run value `Brightfly`; `--no-autostart` removes it.
- Stops the old instance with `WM_CLOSE` (fallback `TerminateProcess`)
  before overwriting files. File writes are retried for up to 3 seconds if the exe is still
  locked.
- Flags: `--no-start`, `--quiet`, `--help`.

Uninstall mode is selected from the file name (`uninstall.exe`) or the flag
`--uninstall`. The uninstaller stops the process, removes the Run value, the Apps &
features entry, and the shortcut. The config in `%APPDATA%\Brightfly` is kept;
`--purge-config` removes it. The install folder is removed via a scheduled
`cmd /c rmdir` after the dialog closes, because the exe cannot delete
itself.

The old PowerShell scripts (`install.cmd/ps1`, `uninstall.cmd/ps1`) still exist for
installation from a source checkout, but are not included in the zip.

| Test | Method | Result |
|---|---|---|
| Install from zip | `setup.exe --no-start --quiet` | exit 0; `brightfly.exe`, `README.md`, `uninstall.exe` ✅ |
| Default dialog | `setup.exe`, close the dialog | app runs, exit 0 ✅ |
| `--no-autostart` | install with the flag | Run value absent, exit 0 ✅ |
| `--quiet` | install/uninstall with the flag | no dialog, exit 0 ✅ |
| Reinstall while the app runs | run setup again without closing the app | old process stops, files overwritten, exit 0 ✅ |
| Registry | read the Run + Uninstall keys | metadata complete, paths quoted ✅ |
| Shortcut | read the `.lnk` via WScript.Shell | target and working dir correct ✅ |
| Run the installed exe | `--version`, tray process | 0.1.0, app runs ✅ |
| Uninstall from the install folder | `uninstall.exe --quiet` | exit 0, folder gone in ±2 s ✅ |
| Config preserved | normal uninstall | `config.ini` still present ✅ |
| `--purge-config` | uninstall with the flag | config folder removed ✅ |
| Uninstall when not installed | `setup.exe --uninstall --quiet` | exit 0, no error ✅ |
| Setup resources | `VersionInfo` | `Brightfly Setup`, `setup.exe` ✅ |
| Leftover state | after uninstall | Run, Uninstall key, shortcut, process, folder clean ✅ |

Re-measurement after all changes (release):

| Test | Result |
|---|---|
| Exe / setup / zip size | 310.272 / 608.256 / 422.648 byte |
| Start until tray registered | median 131.0 ms (119.0–214.3) |
| Flyout open | median 14.89 ms (11.05–33.85) |
| Idle CPU 60 s | 0 ms (0%) |
| Idle working set | 10.6 MB WS, 1.6 MB private, 2 threads |
| Flyout-open working set | 22.8 MB WS, 5.0 MB private, 8 threads |
| Drag CPU 150 movements | 0 ms (below granularity) |
| `brightfly stress` | 200 changes in 309 ms, final delta 0 points |
| Clippy | 7 bin warnings / 8 test warnings, all pre-existing |

## GitHub release (CI)

The workflow `.github/workflows/release.yml` builds the package on `windows-latest`
and publishes a GitHub Release. There is no code signing.

- Trigger: push tag `v*` (e.g. `v0.1.0`) or `workflow_dispatch`.
- Steps: `cargo test` → `scripts/package.ps1` → prepare assets → upload
  artifact → create release.
- Release assets: full zip, `setup.exe`, portable exe, and `SHA256SUMS.txt`.
- The tag must match the `Cargo.toml` version; the workflow fails before the release
  if they do not match.
- `workflow_dispatch` only produces artifacts, no release.
- First run: `v0.1.0` (run `41979730507`) succeeded in 1m55s and published
  <https://github.com/nafnaufal/brightfly/releases/tag/v0.1.0>. Assets from CI:
  `setup.exe` 607.232 B, zip 432.443 B, exe 309.760 B, `SHA256SUMS.txt` 273 B.
  Sizes differ slightly from the local build because the runner toolchain differs.
- The workflow uses `actions/checkout@v7`, `actions/upload-artifact@v7`,
  `softprops/action-gh-release@v3`, and `Swatinem/rust-cache@v2.9.3` so it
  stays clear of Node 20 deprecation warnings.

How to release:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

## Gate results (phase 0)

Test hardware: 1 external MSI monitor.

```
name     : Generic PnP Monitor
device   : \\.\DISPLAY1
id       : \\?\DISPLAY#MSI30B9#7&21ebfe3c&0&UID24646#{e6f07b5f-...}
method   : ddc
range    : 0..100
current  : 50%
```

- DDC/CI works fully: read and write.
- Range 0..100 (not 0..255).
- Write 70% → read back 70%. Write 50% → read back 50%.
- Config is stored in `%APPDATA%\Brightfly\config.ini` and read back correctly.

## Phase 4 — test results

| Test | Method | Result |
|---|---|---|
| Name from EDID | `probe` | ✅ `MSI MP223 E2` (previously `Generic PnP Monitor`) |
| Startup applies config | config 50%, monitor at 80% | ✅ monitor becomes 50% after the app starts |
| Persist while the slider moves | click 30%, wait for the 1.2 s debounce | ✅ `brightness=30` in config |
| Persist when the flyout closes | set 80%, `Esc` 100 ms later, check after 250 ms | ✅ `brightness=80` (immediate flush, no debounce wait) |
| Persist after restart | restart the process | ✅ `probe` reads the last value from config |
| Autostart | menu → "Start with Windows" | ✅ Run value `Brightfly` contains the quoted exe path |
| Autostart off | toggle the same item | ✅ Run value removed, config follows at 0 |
| Sync menu | toggle the first item | ✅ `sync` config changes 1↔0 |
| Step from config | `step=5`, right arrow | ✅ 80% → 85% |
| `WM_DISPLAYCHANGE` | send the message with the flyout open | ✅ flyout closes, rescan, slider still works |
| Resume | `WM_POWERBROADCAST` + `PBT_APMRESUMEAUTOMATIC` | ✅ flyout closes, rescan + refresh, slider still works |
| Two rows + names | render two rows (real monitor + temporary test row) | ✅ card 192×128, 12 px name above each slider |
| Single instance | run a second exe | ✅ the old instance's flyout opens |
| Memory & threads (release) | idle / flyout open | 10.5 MB WS, 5 threads / 22.6 MB WS, 9 threads (private 5.0 MB) |

Note: the test machine has only one physical monitor, so two-monitor
synchronization and physical unplug/replug are untested; display-change and resume
messages were simulated directly against the controller. The double row was verified by
adding one temporary test row while taking screenshots, then that test code
was removed.

## Phase 3 — test results

Visual target: Windows 11 volume flyout, measured at DPI 100% (card 192×48 px,
radius 8, icon 16 px, track 4 px from x=42 to x=150, number 15 px right-aligned).

| Test | Method | Result |
|---|---|---|
| Geometry & color | compare pixels against the real volume screenshot | ✅ difference ≤1 px / 1 color level |
| Empty track | `ControlStrongFillColorDefault` token (54% white) | ✅ `#9F9F9F` matches the original |
| Filled track | `AccentPalette` index 1 (dark) / 4 (light) | ✅ `#BDBAE7` |
| Thumb | only on hover/drag | ✅ |
| Open–close 5× | screenshot each cycle | ✅ fill always drawn |
| Track click | send `WM_LBUTTONDOWN` at 25% and 80% | ✅ monitor follows 25% / 80% |
| Drag | `WM_MOUSEMOVE` with the button held | ✅ |
| Wheel | `WM_MOUSEWHEEL` | ✅ 82% → 84% → 82% |
| Left/right arrow | `WM_KEYDOWN` | ✅ ±2% |
| `Esc` closes | `WM_KEYDOWN` VK_ESCAPE | ✅ |
| Light theme | `AppsUseLightTheme=1` | ✅ card `#F4F4F4`, blue accent |
| Working set (release) | idle / flyout open | 10.4 MB / 22.6 MB (private 5.1 MB) |
| Threads | idle / flyout open | 5 / 7 |

## Phase 2 — test results

| Test | Method | Result |
|---|---|---|
| Tray icon appears | process running | ✅ |
| Tray click opens the flyout | second instance triggers `WM_TOGGLE` | ✅ visible=true |
| Tray click closes the flyout | same | ✅ visible=false |
| `Esc` closes | `SendKeys {ESC}` | ✅ |
| Outside click closes | flyout loses focus | ✅ |
| Not shown in Alt+Tab | `WS_EX_TOOLWINDOW` (exstyle 0x88) | ✅ |
| Single instance | second instance exits immediately | ✅ |
| Position | above the tray, clamped to the work area | ✅ |
| Working set | Task Manager | 15.4 MB (debug) |
| Threads | — | 4 |

## Technical decisions made

- **No channel, use a `desired` slot + Condvar.** Simpler than `mpsc`
  for coalescing, and cannot pile up.
- **Acquire/destroy the physical monitor handle per operation.** Old handles are not kept,
  so unplug/replug leaves no stale handle.
- **Join handles are not joined on shutdown.** The process must still be able to exit even with
  a hanging DDC call.
- **Controller state is stored in `GWLP_USERDATA`, not `thread_local`.** Wndproc can be
  called reentrantly during `SetWindowPos`; `thread_local` + `RefCell` would panic.
- **`force_foreground()` uses `AttachThreadInput`.** A plain `SetForegroundWindow` is rejected
  by Windows when the process is not the last input recipient, so `Esc` never reaches the flyout.
- **`GetDeviceGammaRamp` lives in `Win32::UI::ColorSystem`**, not `Win32::Graphics::Gdi`.
- **Sync updates the flyout rows locally.** `supervisor.monitors()` still holds the old
  values until the DDC worker finishes, so `Flyout::set_all_pct()` is called directly so all
  rows move together without waiting for hardware.
- **Config is saved with a 500 ms debounce** (`SetTimer`), then flushed when the flyout closes
  and on `WM_DESTROY`. Writing the file on every slider change would cause excessive I/O.
- **The registry is the source of truth for autostart at start.** The `start_with_windows` value
  in config is not used to write the registry automatically; the menu only reads/writes the registry
  when the user selects it.
- **The last value is also stored in `Supervisor`.** When a monitor disappears and reappears,
  the fallback is the last known value, not the 50% default.
- **The monitor name comes from `DISPLAYCONFIG_TARGET_DEVICE_NAME`.** `EnumDisplayDevices`
  only gives "Generic PnP Monitor"; the GDI source is linked to the target via
  `DISPLAYCONFIG_SOURCE_DEVICE_NAME`.
- **Name labels appear only when there is more than 1 row.** Each row grows by 16 DIP for the
  name strip, per PLAN section 3.1; the single-monitor flyout stays 192×48 like the volume flyout.
- **Icon and version info are embedded via `build.rs`, not an extra crate.** `rc.exe`
  is looked up in the Windows SDK; the compiled resource is passed with
  `cargo:rustc-link-arg`. There is no new build dependency and the build still works
  (with a warning) on machines without the Windows SDK.
- **The installer/uninstaller is a native Rust binary, not MSI/NSIS.** NSIS/WiX is not
  available on the build machine and adds no value for a per-user app without a service.
  The payload is embedded via `build.rs`, while the shortcut, registry, autostart, and
  cleanup are handled directly with Win32/COM. The result is a single `setup.exe` with no
  temp extraction and no PowerShell dependency.
- **Monitor enumeration on display change/resume runs on a background thread.** A DDC probe
  can block, so results are sent over `mpsc` and installed via
  `WM_RESCAN_DONE`; consecutive requests are coalesced into one rescan. The initial
  enumeration at start stays synchronous so the first flyout is not empty.
- **The 256 icon frame is stored as PNG inside the ICO.** Windows supports it and
  the size drops from 270.376 to 5.474 byte; small frames stay DIB for compatibility
  with older shells.

## Deviations from the plan

- **The Phase 3 renderer does not use DirectComposition/D3D/acrylic.** The initial plan
  used an alpha swapchain + acrylic backdrop. In practice the DirectComposition visual
  is lost on every `ShowWindow(SW_HIDE)` → `SW_SHOW`, so a reopened flyout
  renders empty; recreating the device/target on each show only masks part of it and
  interaction stays unreliable. The backend was switched to **`ID2D1DCRenderTarget`**: draw
  directly to the HDC in `WM_PAINT`, opaque card, rounded corners via `SetWindowRgn`.
  Accepted consequences: no backdrop blur and the corner edges are not
  anti-aliased (radius 8 px). Benefits: no composition state that can be lost,
  and much lower memory.
- **`D2D1_RENDER_TARGET_TYPE_SOFTWARE` is forced.** With the `DEFAULT` type, Direct2D
  loads the GPU D3D11 driver (35 MB private memory) just for a 192×48 card.
  Software brings private memory down to 5 MB with no visual change.
- **Phase 2 first used a GDI placeholder**; the new renderer arrived in Phase 3.
- **Scroll over the tray icon is not implemented (product decision).** Windows 11 does not
  forward `WM_MOUSEWHEEL` to third-party tray icon callbacks: tested with
  `NOTIFYICON_VERSION_4` and version 3, the taskbar still consumes the wheel. To
  make it work would need `RegisterRawInputDevices` + `RIDEV_INPUTSINK` or
  `WH_MOUSE_LL`, which keeps the UI thread waking on mouse movement and
  conflicts with the "very light" target. Decided to skip it; scroll still
  works while the cursor is over the flyout.
- **The ≤6 thread budget is exceeded when the flyout is open (8 threads).** Idle is only 2 threads,
  but creating `ID2D1DCRenderTarget` loads WARP/D2D helper threads. Removing them means
  going back to GDI and losing text/shape antialiasing. Working set stays at 22.7 MB
  (target ≤25 MB) and idle CPU is 0%, so this deviation is accepted and recorded.

## Known technical debt

- **Scroll over the tray icon is not implemented** (product decision); see
  the deviation above.
- **The initial enumeration at start still runs on the UI thread** before the message loop;
  this is intentional so the first flyout is not empty. Display change and resume already
  use a background thread. A monitor that truly hangs during a scan cannot yet be
  tested on this machine.
- **Two-monitor synchronization and physical unplug/replug are untested** because the test
  machine has only one monitor.
- **Long monitor names are truncated in the 192 px card** (one line, no tooltip).
- **A monitor without a serial number in EDID is only identified at model level.** Two
  identical units share values; with no serial at all, values can swap
  when enumeration order changes. The test monitor has a serial, so this case is untested.
- **Old config migration skips ambiguous monitors.** If there are two old sections
  with the same model, the value is not moved (better to lose a value than
  attach it to the wrong monitor).
- **`method` in config is not yet used to force a method.** Probe always determines
  DDC vs gamma; config only stores the result.
- **`step` can only be changed via the config file**; there is no UI control yet.
- Flyout position depends on `Shell_NotifyIconGetRect`; if the icon is in the overflow area,
  the result is less precise and untested.
- Wheel is only active while the cursor is over the flyout (same behavior as the Windows
  volume flyout), so it cannot be tested remotely without holding the cursor position.
- **The installer does not sign the exe.** SmartScreen may show a warning
  when the file is downloaded from the internet; there is no code signing yet.
- **The uninstaller delays folder removal by ±2 seconds** via `cmd /c rmdir`,
  because the exe cannot delete itself while running. This is
  intentional, not a failure.

## How to run

```
cargo run                # tray app
cargo run -- probe       # list monitors
cargo run -- set 70      # set brightness
cargo run -- stress      # exercise coalescing
```

Build and package the release:

```powershell
cargo build --release
.\scripts\package.ps1
```

## Next steps

Phases 0–5, post-phase 5 hardening, and the installer/uninstaller are done. Reasonable
follow-up work: testing on multi-monitor machines, HDR, slow/hanging
monitors, and code signing. The physical port-move test can already be
performed at any time: config must still use the same EDID section.