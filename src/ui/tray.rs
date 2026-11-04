//! Tray icon.
//!
//! Memakai `NOTIFYICON_VERSION_4` supaya `Shell_NotifyIconGetRect` tersedia
//! (dibutuhkan untuk menempatkan flyout tepat di atas icon).

use std::mem::size_of;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP,
    NIM_ADD, NIM_DELETE, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
    NOTIFYICONIDENTIFIER,
};
use windows::Win32::UI::WindowsAndMessaging::{LoadIconW, IDI_APPLICATION};

pub const TRAY_ID: u32 = 1;

pub struct Tray {
    hwnd: HWND,
    added: bool,
}

impl Tray {
    pub fn add(hwnd: HWND, callback_message: u32, tooltip: &str) -> Option<Self> {
        unsafe {
            let icon = LoadIconW(None, IDI_APPLICATION).ok()?;

            let mut data = NOTIFYICONDATAW::default();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = hwnd;
            data.uID = TRAY_ID;
            data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
            data.uCallbackMessage = callback_message;
            data.hIcon = icon;
            write_tip(&mut data.szTip, tooltip);

            if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
                return None;
            }

            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);

            Some(Self { hwnd, added: true })
        }
    }

    /// Posisi icon di taskbar. `None` kalau icon sedang di overflow area.
    pub fn rect(&self) -> Option<RECT> {
        let identifier = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            guidItem: windows::core::GUID::zeroed(),
        };
        unsafe { Shell_NotifyIconGetRect(&identifier).ok() }
    }

    pub fn remove(&mut self) {
        if !self.added {
            return;
        }
        unsafe {
            let mut data = NOTIFYICONDATAW::default();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.hwnd;
            data.uID = TRAY_ID;
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
        self.added = false;
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.remove();
    }
}

fn write_tip(buffer: &mut [u16], text: &str) {
    let mut i = 0;
    for unit in text.encode_utf16() {
        if i + 1 >= buffer.len() {
            break;
        }
        buffer[i] = unit;
        i += 1;
    }
    buffer[i] = 0;
}

/// Dipakai saat window belum punya icon tray (fallback posisi).
pub fn cursor_anchor() -> RECT {
    let mut point = windows::Win32::Foundation::POINT::default();
    if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point) }.is_err() {
        return RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
    }
    RECT {
        left: point.x,
        top: point.y,
        right: point.x,
        bottom: point.y,
    }
}