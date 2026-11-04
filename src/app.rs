//! Controller: window tersembunyi yang memegang tray, flyout, dan supervisor.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM, ERROR_ALREADY_EXISTS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Shell::NIN_SELECT;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW, GetWindowLongPtrW, KillTimer,
    PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetTimer,
    SetWindowLongPtrW, TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA,
    MF_CHECKED, MF_SEPARATOR, MF_STRING, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND,
    TPM_NONOTIFY, TPM_RETURNCMD, WM_APP, WM_CONTEXTMENU as WM_CONTEXTMENU_MSG, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_POWERBROADCAST, WM_TIMER,
    WNDCLASSW, WS_OVERLAPPED,
};

use crate::autostart;
use crate::config::{Config, MonitorConfig};
use crate::monitor::ddc::{self, RawMonitor};
use crate::monitor::{Method, State, Supervisor};
use crate::ui::flyout::{self, Flyout};
use crate::ui::render::Row;
use crate::ui::tray::{self, Tray};

const WM_TRAY: u32 = WM_APP + 1;
const WM_TOGGLE: u32 = WM_APP + 2;
/// Dikirim flyout saat slider bergerak: `wparam` = index baris, `lparam` = persen.
pub const WM_SET_BRIGHTNESS: u32 = WM_APP + 3;
/// Dikirim flyout saat tertutup supaya config segera disimpan.
pub const WM_FLYOUT_HIDDEN: u32 = WM_APP + 4;
/// Dikirim thread scan setelah enumerasi monitor selesai.
const WM_RESCAN_DONE: u32 = WM_APP + 5;

/// `NIN_KEYSELECT` tidak diekspor crate `windows`; di shellapi.h nilainya
/// `NIN_SELECT + 1`. Klik keyboard (Enter/Space pada icon) harus diperlakukan
/// sama dengan klik kiri.
const NIN_KEYSELECT: u32 = NIN_SELECT + 1;

const TIMER_SAVE: usize = 1;
const SAVE_DELAY_MS: u32 = 500;

const MENU_SYNC: usize = 1;
const MENU_AUTOSTART: usize = 2;
const MENU_QUIT: usize = 3;

const CONTROLLER_CLASS: PCWSTR = w!("BrightflyController");

struct Controller {
    hwnd: HWND,
    flyout: Flyout,
    tray: Tray,
    supervisor: Supervisor,
    /// Urutan id monitor yang sedang tampil di flyout, sejajar dengan index baris.
    shown_ids: Vec<String>,
    config: Config,
    config_dirty: bool,
    rescan_tx: Sender<Vec<RawMonitor>>,
    rescan_rx: Receiver<Vec<RawMonitor>>,
    rescanning: bool,
    rescan_pending: bool,
    refresh_after_scan: bool,
}

impl Controller {
    fn toggle(&mut self) {
        if self.flyout.is_visible() {
            self.flyout.hide();
            self.flush_config();
            return;
        }
        // Klik tray yang membuat flyout kehilangan fokus tidak boleh
        // langsung membukanya lagi.
        if flyout::hidden_recently() {
            return;
        }

        let monitors = self.supervisor.monitors();
        self.shown_ids = monitors.iter().map(|m| m.id.clone()).collect();
        let rows: Vec<Row> = monitors
            .iter()
            .map(|m| Row {
                pct: m.pct,
                enabled: m.state != State::Unsupported,
                name: m.name.clone(),
            })
            .collect();

        let anchor = self.tray.rect().unwrap_or_else(tray::cursor_anchor);
        self.flyout.show(&anchor, rows, self.config.step);
    }

    fn set_brightness(&mut self, index: usize, pct: u8) {
        let Some(id) = self.shown_ids.get(index).cloned() else {
            return;
        };
        let pct = pct.min(100);

        if self.config.sync {
            self.supervisor.set_all(pct);
            // `supervisor.monitors()` masih berisi nilai lama sampai worker
            // selesai, jadi baris lain diperbarui langsung di sini.
            self.flyout.set_all_pct(pct);
            self.store_all(pct);
        } else {
            self.supervisor.set_pct(&id, pct);
            self.store_one(&id, pct);
        }

        self.mark_dirty();
    }

    fn store_one(&mut self, id: &str, pct: u8) {
        let method = self.supervisor.method_of(id).unwrap_or(Method::Ddc);
        self.config.monitors.insert(
            id.to_string(),
            MonitorConfig {
                brightness: pct,
                method,
            },
        );
    }

    fn store_all(&mut self, pct: u8) {
        for monitor in self.supervisor.monitors() {
            self.config.monitors.insert(
                monitor.id,
                MonitorConfig {
                    brightness: pct,
                    method: monitor.method,
                },
            );
        }
    }

    fn mark_dirty(&mut self) {
        self.config_dirty = true;
        unsafe {
            SetTimer(Some(self.hwnd), TIMER_SAVE, SAVE_DELAY_MS, None);
        }
    }

    /// Tulis config ke disk sekarang dan batalkan timer debounce.
    fn flush_config(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_SAVE);
        }
        if !self.config_dirty {
            return;
        }
        match self.config.save() {
            Ok(()) => self.config_dirty = false,
            Err(error) => eprintln!("Gagal menyimpan config: {error}"),
        }
    }

    fn menu(&mut self) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return;
            };

            let _ = AppendMenuW(
                menu,
                menu_flags(self.config.sync),
                MENU_SYNC,
                w!("Sinkronkan semua monitor"),
            );
            let _ = AppendMenuW(
                menu,
                menu_flags(self.config.start_with_windows),
                MENU_AUTOSTART,
                w!("Mulai bersama Windows"),
            );
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Keluar"));

            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);

            let _ = SetForegroundWindow(self.hwnd);
            let chosen = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY,
                point.x,
                point.y,
                None,
                self.hwnd,
                None,
            );
            let _ = DestroyMenu(menu);

            match chosen.0 as usize {
                MENU_SYNC => self.toggle_sync(),
                MENU_AUTOSTART => self.toggle_autostart(),
                MENU_QUIT => {
                    let _ = DestroyWindow(self.hwnd);
                }
                _ => {}
            }
        }
    }

    fn toggle_sync(&mut self) {
        self.config.sync = !self.config.sync;
        self.mark_dirty();
        self.flush_config();
    }

    fn toggle_autostart(&mut self) {
        let target = !self.config.start_with_windows;
        if autostart::set_enabled(target) {
            self.config.start_with_windows = target;
            self.mark_dirty();
            self.flush_config();
        } else {
            eprintln!("Gagal mengubah entri autostart.");
        }
    }

    /// Monitor ditambah/dilepas atau resolusi berubah.
    fn on_display_change(&mut self) {
        self.flyout.hide();
        self.request_rescan(false);
        self.flush_config();
    }

    /// Bangun kembali handle monitor setelah sleep; DDC sering kehilangan
    /// handle fisik saat sistem tidur.
    fn on_resume(&mut self) {
        self.flyout.hide();
        self.request_rescan(true);
        self.flush_config();
    }

    /// Enumerasi monitor berjalan di thread sendiri: probe DDC bisa memblokir
    /// beberapa detik dan tidak boleh membekukan UI.
    fn request_rescan(&mut self, refresh_after: bool) {
        self.refresh_after_scan |= refresh_after;
        if self.rescanning {
            // Scan yang sedang berjalan mungkin sudah membaca keadaan lama;
            // ulangi sekali lagi setelah hasilnya masuk.
            self.rescan_pending = true;
            return;
        }
        self.start_scan();
    }

    fn start_scan(&mut self) {
        let sender = self.rescan_tx.clone();
        // HWND bukan `Send`; simpan sebagai isize dan bangun ulang di thread.
        let hwnd = self.hwnd.0 as isize;
        self.rescanning = true;

        let spawned = thread::Builder::new()
            .name("brightfly-scan".to_string())
            .spawn(move || {
                let monitors = ddc::enumerate();
                if sender.send(monitors).is_ok() {
                    unsafe {
                        let _ = PostMessageW(
                            Some(HWND(hwnd as *mut c_void)),
                            WM_RESCAN_DONE,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                }
            });

        if spawned.is_err() {
            self.rescanning = false;
            eprintln!("Gagal memulai scan monitor.");
        }
    }

    fn on_rescan_done(&mut self) {
        self.rescanning = false;

        if let Ok(monitors) = self.rescan_rx.try_recv() {
            self.supervisor.apply_scan(monitors);

            if self.refresh_after_scan && !self.rescan_pending {
                self.supervisor.refresh_all();
                self.refresh_after_scan = false;
            }
        }

        if self.rescan_pending {
            self.rescan_pending = false;
            self.start_scan();
        }
    }
}

fn menu_flags(checked: bool) -> windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS {
    let mut flags = MF_STRING;
    if checked {
        flags |= MF_CHECKED;
    }
    flags
}

pub fn run() -> ExitCode {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    // Mutex harus dibuat sebelum mencari instance lain supaya tidak ada balapan.
    let _singleton = unsafe { CreateMutexW(None, false, w!("Global\\Brightfly.Singleton")) };
    let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;

    if already_running {
        if let Ok(existing) = unsafe { FindWindowW(CONTROLLER_CLASS, PCWSTR::null()) } {
            unsafe {
                let _ = PostMessageW(Some(existing), WM_TOGGLE, WPARAM(0), LPARAM(0));
            }
        }
        return ExitCode::SUCCESS;
    }

    let Ok(module) = (unsafe { GetModuleHandleW(None) }) else {
        eprintln!("Gagal mengambil module handle.");
        return ExitCode::FAILURE;
    };
    let instance = HINSTANCE(module.0);

    if !flyout::register_class(instance) {
        eprintln!("Gagal mendaftarkan window class flyout.");
        return ExitCode::FAILURE;
    }
    if !register_controller_class(instance) {
        eprintln!("Gagal mendaftarkan window class controller.");
        return ExitCode::FAILURE;
    }

    let Ok(hwnd) = (unsafe {
        CreateWindowExW(
            Default::default(),
            CONTROLLER_CLASS,
            w!("Brightfly"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }) else {
        eprintln!("Gagal membuat window controller.");
        return ExitCode::FAILURE;
    };

    let Some(flyout_window) = Flyout::create(instance, hwnd) else {
        eprintln!("Gagal membuat window flyout.");
        return ExitCode::FAILURE;
    };

    let Some(tray_icon) = Tray::add(
        instance,
        hwnd,
        WM_TRAY,
        "Brightfly — brightness monitor",
    ) else {
        eprintln!("Gagal memasang icon tray.");
        return ExitCode::FAILURE;
    };

    let mut config = Config::load();
    // Registry adalah sumber kebenaran untuk autostart: config lama mungkin
    // masih menyimpan nilai default tanpa entri registry yang benar-benar ada.
    config.start_with_windows = autostart::is_enabled();

    let saved: BTreeMap<String, u8> = config
        .monitors
        .iter()
        .map(|(id, monitor)| (id.clone(), monitor.brightness))
        .collect();

    let supervisor = Supervisor::new();
    supervisor.apply_config(&saved);

    let (rescan_tx, rescan_rx) = mpsc::channel();

    let controller = Box::new(Controller {
        hwnd,
        flyout: flyout_window,
        tray: tray_icon,
        supervisor,
        shown_ids: Vec::new(),
        config,
        config_dirty: false,
        rescan_tx,
        rescan_rx,
        rescanning: false,
        rescan_pending: false,
        refresh_after_scan: false,
    });
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(controller) as isize);
    }

    message_loop();
    ExitCode::SUCCESS
}

fn message_loop() {
    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn register_controller_class(instance: HINSTANCE) -> bool {
    unsafe {
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(controller_wndproc),
            hInstance: instance,
            lpszClassName: CONTROLLER_CLASS,
            ..Default::default()
        };
        RegisterClassW(&class) != 0
    }
}

unsafe extern "system" fn controller_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Controller;

        match msg {
            WM_DESTROY => {
                if !pointer.is_null() {
                    let mut controller = Box::from_raw(pointer);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    controller.flush_config();
                    controller.supervisor.stop_all();
                    controller.flyout.hide();
                    controller.flyout.destroy();
                    drop(controller);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_TRAY if !pointer.is_null() => {
                let event = (lparam.0 as u32) & 0xFFFF;
                let controller = &mut *pointer;
                match event {
                    // NOTIFYICON_VERSION_4 mengirim NIN_SELECT setelah
                    // WM_LBUTTONUP. Menangani keduanya membuat satu klik
                    // membuka lalu langsung menutup flyout.
                    NIN_SELECT | NIN_KEYSELECT => controller.toggle(),
                    // WM_RBUTTONUP juga dikirim sebelum WM_CONTEXTMENU.
                    WM_CONTEXTMENU_MSG => controller.menu(),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_SET_BRIGHTNESS if !pointer.is_null() => {
                (*pointer).set_brightness(wparam.0, lparam.0 as u8);
                LRESULT(0)
            }
            WM_TOGGLE if !pointer.is_null() => {
                (*pointer).toggle();
                LRESULT(0)
            }
            WM_DISPLAYCHANGE if !pointer.is_null() => {
                (*pointer).on_display_change();
                LRESULT(0)
            }
            WM_POWERBROADCAST if !pointer.is_null() => {
                if matches!(
                    wparam.0 as u32,
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND
                ) {
                    (*pointer).on_resume();
                }
                LRESULT(0)
            }
            WM_TIMER if !pointer.is_null() && wparam.0 == TIMER_SAVE => {
                (*pointer).flush_config();
                LRESULT(0)
            }
            WM_FLYOUT_HIDDEN if !pointer.is_null() => {
                (*pointer).flush_config();
                LRESULT(0)
            }
            WM_RESCAN_DONE if !pointer.is_null() => {
                (*pointer).on_rescan_done();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}