use std::ffi::c_void;
use std::mem::size_of;
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateRoundRectRgn, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromPoint,
    ScreenToClient, SetWindowRgn, UpdateWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos,
    GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible, LoadCursorW, PostMessageW,
    RegisterClassW, SetForegroundWindow, SetWindowPos, ShowWindow, CS_DROPSHADOW,
    EVENT_SYSTEM_FOREGROUND, HWND_TOPMOST, IDC_ARROW, SWP_NOACTIVATE, SW_HIDE, SW_SHOW, WA_INACTIVE,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_ACTIVATE, WM_DESTROY, WM_ERASEBKGND,
    WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_SETCURSOR, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::app::{WM_FLYOUT_HIDDEN, WM_SET_BRIGHTNESS};
use crate::ui::render::{self, Renderer, Row, Theme};

const CLASS_NAME: windows::core::PCWSTR = w!("BrightflyFlyout");

const VK_ESCAPE: usize = 0x1B;
const VK_LEFT: usize = 0x25;
const VK_UP: usize = 0x26;
const VK_RIGHT: usize = 0x27;
const VK_DOWN: usize = 0x28;

const WM_MOUSELEAVE: u32 = 0x02A3;

// A tray click that just opened the flyout must not close it immediately.
const ACTIVATION_GRACE: Duration = Duration::from_millis(200);

const WM_OUTSIDE_FOREGROUND: u32 = 0x8000 + 6;

thread_local! {
    static HIDDEN_AT: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    static HOOK_TARGET: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    static HOOK: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
}

struct FlyoutData {
    hwnd: HWND,
    controller: HWND,
    renderer: Option<Renderer>,
    rows: Vec<Row>,
    hover: Option<usize>,
    dragging: Option<usize>,
    theme: Theme,
    scale: f32,
    step: u8,
    shown_at: Instant,
    tracking_mouse: bool,
}

impl FlyoutData {
    fn layout(&self) -> render::Layout {
        render::layout(self.rows.len(), self.scale)
    }

    fn redraw(&mut self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        let l = self.layout();
        for index in 0..self.rows.len() {
            let top = l.slider_top(index);
            let bottom = l.row_top(index) + l.row_height;
            if y < top || y >= bottom {
                continue;
            }
            if x >= l.track_left - (l.track_height * 2.0) && x <= l.track_right + (l.track_height * 2.0)
            {
                return Some(index);
            }
        }
        None
    }

    fn sync_hover_to_cursor(&mut self) {
        let mut point = POINT::default();
        unsafe {
            if GetCursorPos(&mut point).is_err() || !ScreenToClient(self.hwnd, &mut point).as_bool()
            {
                return;
            }
        }

        let hover = self.row_at(point.x as f32, point.y as f32);
        if hover != self.hover {
            self.hover = hover;
            self.redraw();
        }
    }

    fn drag_to(&mut self, index: usize, x: f32) {
        let l = self.layout();
        let span = (l.track_right - l.track_left).max(1.0);
        let ratio = ((x - l.track_left) / span).clamp(0.0, 1.0);
        let pct = (ratio * 100.0).round() as u8;

        let Some(row) = self.rows.get_mut(index) else {
            return;
        };
        if !row.enabled || row.pct == pct {
            return;
        }
        row.pct = pct;

        unsafe {
            let _ = PostMessageW(
                Some(self.controller),
                WM_SET_BRIGHTNESS,
                WPARAM(index),
                LPARAM(pct as isize),
            );
        }
    }

    fn nudge(&mut self, index: usize, delta: i32) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let pct = (row.pct as i32 + delta).clamp(0, 100) as u8;
        let l = self.layout();
        let x = l.thumb_x(pct);
        self.drag_to(index, x);
    }

    fn set_all_pct(&mut self, pct: u8) {
        let pct = pct.min(100);
        let mut changed = false;
        for row in &mut self.rows {
            if row.enabled && row.pct != pct {
                row.pct = pct;
                changed = true;
            }
        }
        if changed {
            self.redraw();
        }
    }
}

pub struct Flyout {
    hwnd: HWND,
}

impl Flyout {
    pub fn create(
        hinstance: windows::Win32::Foundation::HINSTANCE,
        controller: HWND,
    ) -> Option<Self> {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                CLASS_NAME,
                w!("Brightfly"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(hinstance),
                None,
            )
            .ok()?;

            let data = Box::new(FlyoutData {
                hwnd,
                controller,
                renderer: None,
                rows: Vec::new(),
                hover: None,
                dragging: None,
                theme: Theme::system(),
                scale: 1.0,
                step: 2,
                shown_at: Instant::now(),
                tracking_mouse: false,
            });
            windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                Box::into_raw(data) as isize,
            );

            Some(Self { hwnd })
        }
    }

    pub fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub fn show(&self, anchor: &RECT, rows: Vec<Row>, step: u8) {
        let Some(data) = self.data() else {
            return;
        };
        let placement = placement(anchor, rows.len());

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                placement.x,
                placement.y,
                placement.width,
                placement.height,
                SWP_NOACTIVATE,
            );
            apply_round_region(
                self.hwnd,
                placement.width,
                placement.height,
                placement.scale,
            );
        }

        data.rows = rows;
        data.hover = None;
        data.dragging = None;
        data.scale = placement.scale;
        data.theme = Theme::system();
        data.step = step.max(1);
        data.shown_at = Instant::now();

        data.ensure_renderer();
        data.sync_hover_to_cursor();

        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
            let _ = UpdateWindow(self.hwnd);
        }
        force_foreground(self.hwnd);
        watch_foreground(self.hwnd);
    }

    pub fn hide(&self) {
        if let Some(data) = self.data() {
            hide_now(self.hwnd, data.controller);
        } else {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            HIDDEN_AT.with(|cell| cell.set(Some(Instant::now())));
        }
    }

    pub fn set_all_pct(&self, pct: u8) {
        if let Some(data) = self.data() {
            data.set_all_pct(pct);
        }
    }

    pub fn destroy(&self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }

    fn data(&self) -> Option<&'static mut FlyoutData> {
        let pointer = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                self.hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            )
        } as *mut FlyoutData;
        if pointer.is_null() {
            None
        } else {
            Some(unsafe { &mut *pointer })
        }
    }
}

impl FlyoutData {
    fn ensure_renderer(&mut self) {
        if self.renderer.is_none() {
            self.renderer = Renderer::new();
        }
    }
}

pub fn hidden_recently() -> bool {
    HIDDEN_AT.with(|cell| {
        cell.get()
            .map(|at| at.elapsed() < Duration::from_millis(250))
            .unwrap_or(false)
    })
}

fn hide_now(hwnd: HWND, controller: HWND) {
    unwatch_foreground();
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = PostMessageW(
            Some(controller),
            WM_FLYOUT_HIDDEN,
            WPARAM(0),
            LPARAM(0),
        );
    }
    HIDDEN_AT.with(|cell| cell.set(Some(Instant::now())));
}

// WM_ACTIVATE(WA_INACTIVE) is not delivered when force_foreground() bypasses
// normal activation, so the foreground event is the primary close signal.
fn watch_foreground(hwnd: HWND) {
    unwatch_foreground();

    HOOK_TARGET.with(|cell| cell.set(hwnd.0 as isize));
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    HOOK.with(|cell| cell.set(hook.0 as isize));
}

fn unwatch_foreground() {
    HOOK.with(|cell| {
        let raw = cell.replace(0);
        if raw != 0 {
            unsafe {
                let _ = UnhookWinEvent(HWINEVENTHOOK(raw as *mut c_void));
            }
        }
    });

    HOOK_TARGET.with(|cell| cell.set(0));
}

unsafe extern "system" fn foreground_changed(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if event != EVENT_SYSTEM_FOREGROUND {
        return;
    }

    let target = HOOK_TARGET.with(|cell| cell.get());
    if target == 0 || hwnd.0 as isize == target {
        return;
    }

    unsafe {
        let _ = PostMessageW(
            Some(HWND(target as *mut c_void)),
            WM_OUTSIDE_FOREGROUND,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
}

pub fn placement(anchor: &RECT, row_count: usize) -> Placement {
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
    let (width, height) = Renderer::measure(row_count.max(1), scale);
    let margin = (8.0 * scale).round() as i32;

    let mut x = (anchor.left + anchor.right) / 2 - width / 2;
    let mut y = anchor.top - height - margin;

    if y < work.top {
        y = anchor.bottom + margin;
    }

    let min_x = work.left + margin;
    let max_x = (work.right - width - margin).max(min_x);
    x = x.clamp(min_x, max_x);

    Placement {
        x,
        y,
        width,
        height,
        scale,
    }
}

fn apply_round_region(hwnd: HWND, width: i32, height: i32, scale: f32) {
    let diameter = (render::CARD_RADIUS_DIP * 2.0 * scale).round() as i32;
    unsafe {
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, diameter, diameter);
        if !region.is_invalid() {
            SetWindowRgn(hwnd, Some(region), true);
        }
    }
}

fn force_foreground(hwnd: HWND) {
    unsafe {
        let current = GetCurrentThreadId();
        let foreground = GetForegroundWindow();
        let owner = GetWindowThreadProcessId(foreground, None);

        if owner != 0 && owner != current {
            let _ = AttachThreadInput(current, owner, true);
            let _ = SetForegroundWindow(hwnd);
            let _ = BringWindowToTop(hwnd);
            let _ = SetFocus(Some(hwnd));
            let _ = AttachThreadInput(current, owner, false);
        } else {
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
        }
    }
}

pub fn register_class(hinstance: windows::Win32::Foundation::HINSTANCE) -> bool {
    unsafe {
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let class = WNDCLASSW {
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hCursor: cursor,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&class) != 0
    }
}

fn mouse_point(lparam: LPARAM) -> (f32, f32) {
    let raw = lparam.0 as u32;
    let x = (raw & 0xFFFF) as u16 as i16 as f32;
    let y = ((raw >> 16) & 0xFFFF) as u16 as i16 as f32;
    (x, y)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let pointer =
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ) as *mut FlyoutData;

        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                if !pointer.is_null() {
                    let data = &mut *pointer;
                    data.ensure_renderer();
                    if let Some(renderer) = &mut data.renderer {
                        renderer.draw(
                            hdc,
                            &data.rows,
                            data.hover,
                            data.dragging,
                            &data.theme,
                            data.scale,
                        );
                    }
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => {
                LRESULT(1)
            }
            WM_KEYDOWN if !pointer.is_null() => {
                let data = &mut *pointer;
                match wparam.0 {
                    VK_ESCAPE => {
                        hide_now(hwnd, data.controller);
                    }
                    VK_LEFT | VK_DOWN | VK_RIGHT | VK_UP => {
                        let step = data.step as i32;
                        let delta = if wparam.0 == VK_LEFT || wparam.0 == VK_DOWN {
                            -step
                        } else {
                            step
                        };
                        if let Some(index) = data.hover.or(if data.rows.len() == 1 {
                            Some(0)
                        } else {
                            None
                        }) {
                            data.nudge(index, delta);
                            data.redraw();
                        }
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_ACTIVATE if !pointer.is_null() => {
                let active = (wparam.0 & 0xFFFF) as u32;
                if active == WA_INACTIVE && (*pointer).shown_at.elapsed() >= ACTIVATION_GRACE {
                    hide_now(hwnd, (*pointer).controller);
                }
                LRESULT(0)
            }
            WM_KILLFOCUS if !pointer.is_null() => {
                let data = &*pointer;
                let close = IsWindowVisible(hwnd).as_bool()
                    && data.shown_at.elapsed() >= ACTIVATION_GRACE;
                let controller = data.controller;
                if close {
                    hide_now(hwnd, controller);
                }
                LRESULT(0)
            }
            WM_OUTSIDE_FOREGROUND if !pointer.is_null() => {
                let data = &*pointer;
                let close = IsWindowVisible(hwnd).as_bool()
                    && data.shown_at.elapsed() >= ACTIVATION_GRACE
                    && GetForegroundWindow() != hwnd;
                let controller = data.controller;
                if close {
                    hide_now(hwnd, controller);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE if !pointer.is_null() => {
                let data = &mut *pointer;
                let (x, y) = mouse_point(lparam);
                let mut dirty = false;

                let hover = data.row_at(x, y);
                if hover != data.hover {
                    data.hover = hover;
                    dirty = true;
                }

                if let Some(index) = data.dragging {
                    data.drag_to(index, x);
                    dirty = true;
                }

                if !data.tracking_mouse {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    if TrackMouseEvent(&mut track).is_ok() {
                        data.tracking_mouse = true;
                    }
                }

                if dirty {
                    data.redraw();
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE if !pointer.is_null() => {
                let data = &mut *pointer;
                data.tracking_mouse = false;
                if data.dragging.is_none() && data.hover.take().is_some() {
                    data.redraw();
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN if !pointer.is_null() => {
                let data = &mut *pointer;
                let (x, y) = mouse_point(lparam);
                if let Some(index) = data.row_at(x, y) {
                    SetCapture(hwnd);
                    data.dragging = Some(index);
                    data.hover = Some(index);
                    data.drag_to(index, x);
                    data.redraw();
                }
                LRESULT(0)
            }
            WM_LBUTTONUP if !pointer.is_null() => {
                let data = &mut *pointer;
                if data.dragging.take().is_some() {
                    let _ = ReleaseCapture();
                    data.redraw();
                }
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_CAPTURECHANGED if !pointer.is_null() => {
                let data = &mut *pointer;
                if data.dragging.take().is_some() {
                    data.redraw();
                }
                LRESULT(0)
            }
            WM_MOUSEWHEEL if !pointer.is_null() => {
                let data = &mut *pointer;
                if data.hover.is_none() {
                    data.sync_hover_to_cursor();
                }
                let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                let step = data.step as i32;
                let step = if delta > 0 { step } else { -step };
                if let Some(index) = data.hover {
                    data.nudge(index, step);
                    data.redraw();
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
                windows::Win32::UI::WindowsAndMessaging::SetCursor(Some(cursor));
                LRESULT(1)
            }
            WM_DESTROY => {
                unwatch_foreground();
                if !pointer.is_null() {
                    drop(Box::from_raw(pointer));
                    windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                        0,
                    );
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}