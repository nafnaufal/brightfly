# Progress

Updated: post-phase 5 (background-thread rescan, PNG icon, tray click fix,
monitor identity from EDID, auto-close on outside click of the flyout, fix for
the "100" number, the Inno Setup installer, the console-window fix, the English
UI, the always-visible slider thumb, the rename to Brighty, and the animated
hover thumb).

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

## Installer and uninstaller (Inno Setup)

Version 0.1.0 shipped a hand-written Rust installer (`src/bin/setup.rs`, target
`brightfly-setup`). It was dropped in 0.1.1: a user reported that after
allowing the SmartScreen prompt, only an empty console window appeared and
nothing was installed, and the binary left no log or error to diagnose. The
installer is now a standard Inno Setup wizard (`installer/brightfly.iss`);
the application itself is still a single small Rust exe.

Installer configuration:

- `PrivilegesRequired=lowest` with `DefaultDirName={autopf}\Brightfly`, which
  resolves to `%LOCALAPPDATA%\Programs\Brightfly`; no admin prompt.
- `WizardStyle=modern`, `SetupLogging=yes`, `LicenseFile=..\LICENSE`,
  `SetupIconFile=..\assets\brightfly.ico`, `MinVersion=10.0`, x64 only.
- Tasks: `autostart` (checked on first install through `checkedonce`, writes the
  Run value `Brightfly` with `uninsdeletevalue`) and `desktopicon` (unchecked).
- Start Menu shortcut plus an entry `HKCU\...\Uninstall\{6A9791EB-...}_is1`, so
  it appears in Settings → Apps with the app icon.
- `[InstallDelete]` removes the legacy `uninstall.exe`; `[Registry]` deletes the
  legacy `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Brightfly`
  key left by 0.1.0.
- `[Code] PrepareToInstall` and `[UninstallRun]` run
  `taskkill /IM brightfly.exe /F`, so upgrading or uninstalling works while the
  app is running.
- The uninstaller is `unins000.exe`. Config in `%APPDATA%\Brightfly` is kept.

`scripts/package.ps1` builds the release exe, locates `ISCC.exe` via `%ISCC%`,
`PATH`, or the default Inno Setup folder, compiles the `.iss` with
`/DAppVersion=<version>`, and writes three files to `dist\`:
`brightfly-<version>-setup.exe`, `brightfly-<version>.exe`, and
`brightfly-<version>-win64.zip` (portable exe + README + LICENSE).

The old PowerShell scripts (`install.cmd/ps1`, `uninstall.cmd/ps1`) still exist for
installation from a source checkout, but are not included in the zip.

| Test | Method | Result |
|---|---|---|
| Silent install | `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-` | exit 0; `brightfly.exe`, `README.md`, `LICENSE`, `unins000.*` ✅ |
| Default tasks | silent install | Run value and Start Menu shortcut created ✅ |
| Uninstall entry | read `HKCU\...\Uninstall\{6A9791EB-...}_is1` | `Brightfly 0.1.1`, DisplayIcon points at the exe ✅ |
| Legacy cleanup | install over a 0.1.0 install | `uninstall.exe` and the old `...\Uninstall\Brightfly` key removed ✅ |
| Run the installed exe | `--version` and tray process | `brightfly 0.1.1`, app runs ✅ |
| Reinstall while the app runs | run setup again with the app open | process killed, files replaced, exit 0 ✅ |
| Uninstall while the app runs | `unins000.exe /VERYSILENT` | exit 0, process stops, folder removed ✅ |
| Uninstall cleanup | after uninstall | folder, shortcut, Run value, Uninstall key gone ✅ |
| Config preserved | normal uninstall | `%APPDATA%\Brightfly\config.ini` still present ✅ |
| Setup log | `%TEMP%\Setup Log*.txt` | full per-file log written ✅ |

Re-measurement after all changes (release):

| Test | Result |
|---|---|
| Exe / setup / zip size | 311.296 / 2.215.292 / 148.651 byte |
| Start until tray registered | median 131.0 ms (119.0–214.3) |
| Flyout open | median 14.89 ms (11.05–33.85) |
| Idle CPU 60 s | 0 ms (0%) |
| Idle working set | 10.6 MB WS, 1.6 MB private, 2 threads |
| Flyout-open working set | 22.8 MB WS, 5.0 MB private, 8 threads |
| Drag CPU 150 movements | 0 ms (below granularity) |
| `brightfly stress` | 200 changes in 309 ms, final delta 0 points |
| Clippy | 7 bin warnings / 8 test warnings, all pre-existing |

The zip is smaller than the 0.1.0 package because it no longer carries the
installer; `setup.exe` is now a separate release asset.

## Tray app no longer owns a console (0.1.2)

Up to 0.1.1 the binary was built as a console-subsystem executable. Windows
therefore allocated a console window on every launch. Two consequences:

- A terminal window appeared whenever the app started, including from the Start
  Menu shortcut.
- The process was attached to that console. Closing the window sent
  `CTRL_CLOSE_EVENT` and terminated the app.

Fix in `src/main.rs`:

- `#![cfg_attr(not(test), windows_subsystem = "windows")]` for the release
  binary. The `cfg_attr` keeps `cargo test` on the console subsystem so the test
  harness can still print results.
- CLI commands (`probe`, `set`, `stress`, `help`, `--version`) call
  `AttachConsole(ATTACH_PARENT_PROCESS)` and install `CONOUT$` only when the
  inherited stdout/stderr handles are null. Piped or redirected handles are left
  alone, so `brightfly probe > out.txt` and `cargo run -- probe` still work.
- The tray path (no arguments) never attaches to a console.

| Test | Method | Result |
|---|---|---|
| PE subsystem | parse the `OptionalHeader.Subsystem` field | `2` (GUI), not `3` ✅ |
| `--version` | run with stdout as a pipe | `brightfly 0.1.2` captured ✅ |
| `help` / `probe` | same | text and monitor list captured ✅ |
| Survives parent shell | start the tray app, exit the shell, check again | process still running ✅ |
| Single instance | start the exe a second time | still 1 process; existing tray opens ✅ |
| Unit tests | `cargo test` | 29 pass, harness output visible ✅ |

## English UI (0.1.3)

All user-facing text is now English:

- Tray menu: `Sync all monitors`, `Start with Windows`, `Quit`.
- CLI: usage, unknown-command and argument errors, `probe`/`set`/`stress`
  output, `PASS`/`FAIL`.
- Monitor error messages, renderer diagnostics, and build-script warnings.
- PowerShell install/uninstall helper output and the shortcut description.
- The Inno Setup wizard and installer tasks were already English.

Config keys (`sync`, `start_with_windows`, `step`, `brightness`, `method`),
registry names, and the EDID-based monitor IDs are unchanged, so existing
`%APPDATA%\Brightfly\config.ini` files keep working.

| Test | Method | Result |
|---|---|---|
| Help and version | capture stdout | English text ✅ |
| `probe` summary | run against the MSI monitor | `Summary: 1 monitor(s) — 1 DDC/CI, 0 gamma-only, 0 unsupported` ✅ |
| Unknown command | `brightfly oops` | `unknown command: oops`, exit 2 ✅ |
| Tray menu | open the tray menu | English labels ✅ |
| Unit tests | `cargo test` | 29 pass ✅ |

## Always-visible slider thumb (0.1.5)

The thumb was only drawn while a row was hovered or dragged, and even then it
could disappear. Two problems combined:

1. The draw was wrapped in `if active`, so the resting state had no handle at
   all. It is now always drawn and only grows to 1.2x while hovered or dragged.
2. The brush color was passed straight from an `if` expression:
   `SetColor(if row.enabled { &theme.text() } else { &theme.disabled() })`.
   Those branch temporaries can be dropped before D2D copies the color, so the
   brush sometimes received a stale/transparent value. The color is now stored
   in a local (`thumb_color`) before `SetColor`, which fixed the missing circle.

The flyout also did not know where the cursor was until the first
`WM_MOUSEMOVE`, so the thumb stayed in its resting size when the flyout opened
under the pointer. `sync_hover_to_cursor()` now runs on show and before wheel
handling.

| Test | Method | Result |
|---|---|---|
| Thumb at rest | show the flyout with the cursor away, capture the window | white circle visible at 56% ✅ |
| Thumb on hover | move the cursor over the row, capture again | diameter grows from ~15 px to ~17 px (1.2x) ✅ |
| Wheel right after opening | show, move onto the row, one notch without prior move | 56% → 58% ✅ |
| Drag | press and drag left across the track | reached 4%, restored to 56% ✅ |
| Unit tests | `cargo test` | 29 pass ✅ |
| Clippy | `cargo clippy --all-targets` | same 7 bin / 8 test warnings as before ✅ |

## Rename to Brighty (0.2.0)

The product is now **Brighty**. The GitHub repository is still
`nafnaufal/brightfly`, so existing links and old releases keep working, but
everything inside the app uses the new name:

- Crate and binary: `brighty`, `brighty.exe`.
- Installer: `installer/brighty.iss`, new `AppId`, output
  `brighty-<version>-setup.exe`, installs to `%LOCALAPPDATA%\Programs\Brighty`.
- Config: `%APPDATA%\Brighty\config.ini`.
- Autostart value: `Brighty` under `HKCU\...\CurrentVersion\Run`.
- Window classes `BrightyController` / `BrightyFlyout`, mutex
  `Global\Brighty.Singleton`, and icon file `assets/brighty.ico`.
- Tray tooltip, window title, CLI usage, scripts, README, and release assets.

This is a deliberate clean break, not a migration:

- The new installer has a different `AppId`, so it does **not** replace an
  installed Brightfly. Uninstall Brightfly first if it is still there.
- Existing `%APPDATA%\Brightfly\config.ini` is not imported; Brighty starts
  with defaults and creates its own config on first use.
- Older sections of this log keep the old name because that is what shipped
  at the time.

Artifacts: `brighty.exe` 311.296 byte, `brighty-0.2.0-setup.exe` 2.272.765
byte, `brighty-0.2.0-win64.zip` 205.942 byte.

| Test | Method | Result |
|---|---|---|
| CLI name | `brighty --version`, `brighty help` | `brighty 0.2.0` ✅ |
| Window title and tray | run the app, inspect the window | `Brighty` ✅ |
| Config path | run the app, check `%APPDATA%` | `%APPDATA%\Brighty\config.ini` created ✅ |
| Autostart value | toggle "Start with Windows" | `HKCU\...\Run` value `Brighty` ✅ |
| Installer | silent install from `dist\` | installs `brighty.exe` in `%LOCALAPPDATA%\Programs\Brighty` ✅ |
| Clean break | inspect after install | no Brightfly key or folder touched ✅ |
| Unit tests | `cargo test` | 29 pass ✅ |

## Animated slider thumb

The thumb used to jump from 1.0x to 1.2x the moment a row was hovered. It now
grows smoothly from a resting dot into a larger knob inside an opaque halo,
matching the Windows 11 volume flyout:

- Rest: 7 DIP dot inside a 10 DIP ring.
- Hover or drag: 9.5 DIP dot inside a 13 DIP ring.
- A per-row value is eased with `smoothstep` over 140 ms and driven by a 15 ms
  timer that stops as soon as every row settles, so idle cost is unchanged.
- The halo is opaque (card color mixed 7.5% toward the text color), so it masks
  the filled track behind the thumb instead of tinting with the accent.
- The icon is drawn after the thumb and the number after both, so a hovered
  halo at 0% or 100% cannot cover them.
- Leaving a row or releasing a drag animates back; moving straight between two
  rows animates both at once.

| Test | Method | Result |
|---|---|---|
| Rest state | hover away, capture the flyout | 165 halo px, 14 px dot ✅ |
| Hover state | park the cursor on the thumb, capture | 248 halo px, 18 px dot ✅ |
| Animation | sample the window every ~20 ms while hovering | 165 → 248 px over ~100–140 ms, monotonic ✅ |
| Hover out | move away, capture | back to 165 / 14 ✅ |
| Edges | 0% and 100%, capture while hovered | icon and "100" still clear, no clipping ✅ |
| Unit tests | `cargo test` | 30 pass (new thumb-scale test) ✅ |
| Clippy | `cargo clippy --all-targets` | same 7 bin / 8 test warnings ✅ |

Released as v0.2.1 with a refreshed `assets/screenshot.png` that shows the
hovered thumb. Local artifacts: `brighty.exe` 312.320 B,
`brighty-0.2.1-setup.exe` 2.269.802 B, `brighty-0.2.1-win64.zip` 203.014 B.

## GitHub release (CI)

The workflow `.github/workflows/release.yml` builds the package on `windows-latest`
and publishes a GitHub Release. There is no code signing.

- Trigger: push tag `v*` (e.g. `v0.1.1`) or `workflow_dispatch`.
- Steps: install Inno Setup (Chocolatey) → `cargo test` →
  `scripts/package.ps1` → prepare assets → upload artifact → create release.
- Release assets: setup exe, portable exe, full zip, and `SHA256SUMS.txt`.
- The tag must match the `Cargo.toml` version; the workflow fails before the release
  if they do not match.
- `workflow_dispatch` only produces artifacts, no release.
- First run: `v0.1.0` (run `41979730507`) succeeded in 1m55s and published
  <https://github.com/nafnaufal/brightfly/releases/tag/v0.1.0> with the old Rust
  installer. Assets from CI: `setup.exe` 607.232 B, zip 432.443 B, exe 309.760 B,
  `SHA256SUMS.txt` 273 B. Sizes differ slightly from the local build because the
  runner toolchain differs.
- The workflow uses `actions/checkout@v7`, `actions/upload-artifact@v7`,
  `softprops/action-gh-release@v3`, and `Swatinem/rust-cache@v2.9.3` so it
  stays clear of Node 20 deprecation warnings.

How to release:

```powershell
git tag v0.1.1
git push origin v0.1.1
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
- **The installer is an Inno Setup wizard, not a custom Rust binary.** 0.1.0 shipped a
  hand-written Rust setup that produced no window when it failed, which made install
  failures undiagnosable. Inno Setup gives a standard wizard, a persistent setup log,
  a real uninstaller, and correct Settings → Apps integration for a per-user app with
  no service and no admin rights.
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
- **The installer does not sign the exe.** SmartScreen shows "Windows protected
  your PC" when the file is downloaded from the internet; there is no code
  signing yet. Workarounds documented in the README: *More info → Run anyway*,
  or `Unblock-File` before running. Reputation only improves with downloads or
  a paid OV/EV certificate.
- **The installer requires Inno Setup 6 on the build machine.** CI installs it
  through Chocolatey; a local `scripts/package.ps1` run fails with a clear
  message if `ISCC.exe` cannot be found.
- **The uninstaller leaves `%APPDATA%\Brighty\config.ini` in place**, so a
  reinstall keeps the last brightness values. This is intentional; delete the
  folder manually to reset.
- **CLI output is asynchronous when the exe is called directly from a shell**
  because it is a GUI-subsystem process: the shell may return to the prompt
  before the text appears. The output itself is correct, and piping
  (`brighty probe | Out-Host`) or `cargo run -- probe` makes the shell wait.

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