//! Controller: window tersembunyi yang memegang tray, flyout, dan supervisor.

use std::process::ExitCode;

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
    DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW, GetWindowLongPtrW, PostMessageW,
    PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, TrackPopupMenu,
    TranslateMessage, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, MF_STRING, MSG, TPM_NONOTIFY,
    TPM_RETURNCMD, WM_APP, WM_CONTEXTMENU as WM_CONTEXTMENU_MSG, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

use crate::monitor::{State, Supervisor};
use crate::ui::flyout::{self, Flyout};
use crate::ui::render::Row;
use crate::ui::tray::{self, Tray};

const WM_TRAY: u32 = WM_APP + 1;
const WM_TOGGLE: u32 = WM_APP + 2;
/// Dikirim flyout saat slider bergerak: `wparam` = index baris, `lparam` = persen.
pub const WM_SET_BRIGHTNESS: u32 = WM_APP + 3;

const CONTROLLER_CLASS: PCWSTR = w!("BrightflyController");

struct Controller {
    hwnd: HWND,
    flyout: Flyout,
    tray: Tray,
    supervisor: Supervisor,
    /// Urutan id monitor yang sedang tampil di flyout, sejajar dengan index baris.
    shown_ids: Vec<String>,
}

impl Controller {
    fn toggle(&mut self) {
        if self.flyout.is_visible() {
            self.flyout.hide();
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
            })
            .collect();

        let anchor = self.tray.rect().unwrap_or_else(tray::cursor_anchor);
        self.flyout.show(&anchor, rows);
    }

    fn set_brightness(&mut self, index: usize, pct: u8) {
        let Some(id) = self.shown_ids.get(index) else {
            return;
        };
        self.supervisor.set_pct(id, pct.min(100));
    }

    fn menu(&mut self) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return;
            };
            let _ = AppendMenuW(menu, MF_STRING, 1, w!("Keluar"));

            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);

            let _ = SetForegroundWindow(self.hwnd);
            let chosen =
                TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, point.x, point.y, None, self.hwnd, None);
            let _ = DestroyMenu(menu);

            if chosen.0 == 1 {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
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

    let Some(tray_icon) = Tray::add(hwnd, WM_TRAY, "Brightfly — brightness monitor") else {
        eprintln!("Gagal memasang icon tray.");
        return ExitCode::FAILURE;
    };

    let controller = Box::new(Controller {
        hwnd,
        flyout: flyout_window,
        tray: tray_icon,
        supervisor: Supervisor::new(),
        shown_ids: Vec::new(),
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
                    let controller = Box::from_raw(pointer);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
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
                    NIN_SELECT | WM_LBUTTONDOWN | WM_LBUTTONUP => controller.toggle(),
                    WM_CONTEXTMENU_MSG | WM_RBUTTONUP => controller.menu(),
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
                (*pointer).supervisor.rescan();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}