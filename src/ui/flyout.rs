//! Window flyout.
//!
//! Fase 2 memakai GDI untuk placeholder. Renderer Direct2D + DirectComposition
//! masuk di fase 3, bersama backdrop acrylic dan slider.

use std::cell::Cell;
use std::mem::size_of;
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetMonitorInfoW,
    GetStockObject, InvalidateRect, MonitorFromPoint, SelectObject, SetBkMode, SetTextColor,
    UpdateWindow, DEFAULT_GUI_FONT, DT_CENTER, DT_SINGLELINE, DT_VCENTER, HGDIOBJ, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, TRANSPARENT,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, GetClientRect, GetForegroundWindow,
    GetWindowThreadProcessId, IsWindowVisible, RegisterClassW, SetForegroundWindow, SetWindowPos,
    ShowWindow, CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
    SW_SHOW, WA_INACTIVE, WM_ACTIVATE, WM_DESTROY, WM_KEYDOWN, WM_PAINT, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// Ukuran kartu dalam DIP (96 DPI). Konversi ke piksel memakai DPI monitor.
pub const CARD_WIDTH_DIP: f32 = 360.0;
pub const CARD_HEIGHT_DIP: f32 = 76.0;

const VK_ESCAPE: usize = 0x1B;
const CLASS_NAME: windows::core::PCWSTR = w!("BrightflyFlyout");

thread_local! {
    static HIDDEN_AT: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub struct Flyout {
    hwnd: HWND,
}

impl Flyout {
    pub fn create(hinstance: windows::Win32::Foundation::HINSTANCE) -> Option<Self> {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                CLASS_NAME,
                w!("Brightfly"),
                WS_POPUP,
                0,
                0,
                CARD_WIDTH_DIP as i32,
                CARD_HEIGHT_DIP as i32,
                None,
                None,
                Some(hinstance),
                None,
            )
            .ok()?;
            Some(Self { hwnd })
        }
    }

    pub fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub fn show(&self, anchor: &RECT) {
        let (x, y, width, height) = placement(anchor);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height,
                SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            force_foreground(self.hwnd);
            let _ = InvalidateRect(Some(self.hwnd), None, true);
            let _ = UpdateWindow(self.hwnd);
        }
    }

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        HIDDEN_AT.with(|cell| cell.set(Some(Instant::now())));
    }
}

/// Baru saja disembunyikan (mis. karena kehilangan fokus)?
///
/// Dipakai untuk mencegah flyout terbuka lagi saat user mengklik icon tray:
/// klik itu membuat flyout kehilangan fokus lalu event tray datang sesudahnya.
pub fn hidden_recently() -> bool {
    HIDDEN_AT.with(|cell| {
        cell.get()
            .map(|at| at.elapsed() < Duration::from_millis(250))
            .unwrap_or(false)
    })
}

/// Hitung posisi & ukuran dalam piksel fisik untuk monitor tempat anchor berada.
pub fn placement(anchor: &RECT) -> (i32, i32, i32, i32) {
    let point = POINT {
        x: (anchor.left + anchor.right) / 2,
        y: (anchor.top + anchor.bottom) / 2,
    };

    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    let mut work = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);

        let mut info = MONITORINFO::default();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            work = info.rcWork;
        }
    }

    let scale = dpi_x as f32 / 96.0;
    let width = (CARD_WIDTH_DIP * scale).round() as i32;
    let height = (CARD_HEIGHT_DIP * scale).round() as i32;
    let margin = (8.0 * scale).round() as i32;

    // Tengah terhadap icon tray.
    let mut x = (anchor.left + anchor.right) / 2 - width / 2;
    let mut y = anchor.top - height - margin;

    if y < work.top {
        y = anchor.bottom + margin;
    }

    let min_x = work.left + margin;
    let max_x = (work.right - width - margin).max(min_x);
    x = x.clamp(min_x, max_x);

    (x, y, width, height)
}

/// Paksa window mendapat fokus keyboard.
///
/// `SetForegroundWindow` ditolak Windows kalau proses pemanggil bukan penerima
/// input terakhir. Attach ke thread foreground membuatnya lolos.
fn force_foreground(hwnd: HWND) {
    unsafe {
        let current = GetCurrentThreadId();
        let foreground = GetForegroundWindow();
        let owner = GetWindowThreadProcessId(foreground, None);

        if owner != 0 && owner != current {
            let _ = AttachThreadInput(current, owner, true);
            let _ = SetForegroundWindow(hwnd);
            let _ = BringWindowToTop(hwnd);
            let _ = AttachThreadInput(current, owner, false);
        } else {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

pub fn register_class(hinstance: windows::Win32::Foundation::HINSTANCE) -> bool {
    unsafe {
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&class) != 0
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                paint(hwnd);
                LRESULT(0)
            }
            WM_KEYDOWN if wparam.0 == VK_ESCAPE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                HIDDEN_AT.with(|cell| cell.set(Some(Instant::now())));
                LRESULT(0)
            }
            WM_ACTIVATE if (wparam.0 & 0xFFFF) == WA_INACTIVE as usize => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                HIDDEN_AT.with(|cell| cell.set(Some(Instant::now())));
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn paint(hwnd: HWND) {
    unsafe {
        let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);

        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);

        let brush = CreateSolidBrush(COLORREF(0x0020_2020));
        FillRect(hdc, &client, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));

        let _ = SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(0x00FF_FFFF));

        let font = GetStockObject(DEFAULT_GUI_FONT);
        let previous = SelectObject(hdc, font);

        let mut text: Vec<u16> = "BRIGHTFLY".encode_utf16().chain(std::iter::once(0)).collect();
        let mut area = client;
        DrawTextW(hdc, &mut text, &mut area, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

        SelectObject(hdc, previous);
        let _ = EndPaint(hwnd, &ps);
    }
}