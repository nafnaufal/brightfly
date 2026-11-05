use std::mem::size_of;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, RECT};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP,
    NIM_ADD, NIM_DELETE, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
    NOTIFYICONIDENTIFIER,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetSystemMetrics, LoadIconW, LoadImageW, HICON, IDI_APPLICATION, IMAGE_ICON,
    LR_DEFAULTCOLOR, SM_CXSMICON,
};

pub const TRAY_ID: u32 = 1;

const RESOURCE_ICON: u16 = 1;

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    owned_icon: bool,
    added: bool,
}

impl Tray {
    pub fn add(
        instance: HINSTANCE,
        hwnd: HWND,
        callback_message: u32,
        tooltip: &str,
    ) -> Option<Self> {
        let (icon, owned_icon) = load_icon(instance)?;

        unsafe {
            let mut data = NOTIFYICONDATAW::default();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = hwnd;
            data.uID = TRAY_ID;
            data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
            data.uCallbackMessage = callback_message;
            data.hIcon = icon;
            write_tip(&mut data.szTip, tooltip);

            if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
                if owned_icon {
                    let _ = DestroyIcon(icon);
                }
                return None;
            }

            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
        }

        Some(Self {
            hwnd,
            icon,
            owned_icon,
            added: true,
        })
    }

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
        if self.owned_icon && !self.icon.is_invalid() {
            unsafe {
                let _ = DestroyIcon(self.icon);
            }
        }
    }
}

fn load_icon(instance: HINSTANCE) -> Option<(HICON, bool)> {
    unsafe {
        let size = GetSystemMetrics(SM_CXSMICON);
        if let Ok(handle) = LoadImageW(
            Some(instance),
            PCWSTR(RESOURCE_ICON as usize as *const u16),
            IMAGE_ICON,
            size,
            size,
            LR_DEFAULTCOLOR,
        ) {
            return Some((HICON(handle.0), true));
        }

        LoadIconW(None, IDI_APPLICATION)
            .ok()
            .map(|icon| (icon, false))
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