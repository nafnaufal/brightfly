//! Enumerasi monitor dan I/O DDC/CI.
//!
//! Semua fungsi di sini memblokir dan harus dipanggil dari worker thread,
//! bukan dari thread UI.

use std::ffi::c_void;
use std::mem::size_of;
use std::thread;
use std::time::Duration;

use windows::core::{BOOL, PCWSTR};
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply, SetVCPFeature,
    PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::{HANDLE, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR,
    MONITORINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME;

use super::{Error, Method};

/// VCP code untuk brightness (MCCS standar).
pub const VCP_BRIGHTNESS: u8 = 0x10;

/// Hasil enumerasi mentah, belum diikat ke worker.
pub struct RawMonitor {
    pub id: String,
    pub name: String,
    pub device: String,
    pub hmonitor: isize,
    pub physical_index: u32,
    pub max: u32,
    pub pct: u8,
    pub method: Method,
}

#[inline]
fn as_hmonitor(raw: isize) -> HMONITOR {
    HMONITOR(raw as *mut c_void)
}

#[inline]
fn utf16z(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

unsafe extern "system" fn collect_cb(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    // SAFETY: lparam menunjuk ke Vec<isize> milik `enumerate`, yang hidup
    // selama EnumDisplayMonitors berjalan (sinkron).
    unsafe {
        let out = &mut *(lparam.0 as *mut Vec<isize>);
        out.push(hmon.0 as isize);
    }
    BOOL::from(true)
}

/// Daftar semua monitor fisik yang terpasang.
///
/// Monitor yang tidak mendukung DDC/CI tetap dikembalikan dengan
/// `method == Gamma`, supaya UI masih bisa menampilkannya.
pub fn enumerate() -> Vec<RawMonitor> {
    let mut handles: Vec<isize> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_cb),
            LPARAM(&mut handles as *mut Vec<isize> as isize),
        );
    }

    let mut out = Vec::new();
    for h in handles {
        out.extend(probe(h));
    }
    out
}

fn probe(hmonitor: isize) -> Vec<RawMonitor> {
    let mut found = Vec::new();
    unsafe {
        let hmon = as_hmonitor(hmonitor);

        let mut mi = MONITORINFOEXW::default();
        mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(hmon, &mut mi.monitorInfo).as_bool() {
            return found;
        }
        let device = utf16z(&mi.szDevice);

        let (id, name) = identity(&device);

        // Ambil handle physical monitor. Kalau gagal, monitor tetap ada di
        // daftar tapi hanya bisa dikendalikan lewat gamma.
        let mut count = 0u32;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &mut count).is_err() || count == 0 {
            found.push(RawMonitor {
                id,
                name,
                device,
                hmonitor,
                physical_index: 0,
                max: 0,
                pct: 100,
                method: Method::Gamma,
            });
            return found;
        }

        let mut buf = vec![PHYSICAL_MONITOR::default(); count as usize];
        if GetPhysicalMonitorsFromHMONITOR(hmon, &mut buf).is_err() {
            found.push(RawMonitor {
                id,
                name,
                device,
                hmonitor,
                physical_index: 0,
                max: 0,
                pct: 100,
                method: Method::Gamma,
            });
            return found;
        }

        for (index, pm) in buf.iter().enumerate() {
            let h: HANDLE = pm.hPhysicalMonitor;
            let mut current = 0u32;
            let mut max = 0u32;
            let mut ok = false;

            // Beberapa monitor butuh satu-dua percobaan setelah bangun/tancap.
            for attempt in 0..2 {
                let rc =
                    GetVCPFeatureAndVCPFeatureReply(h, VCP_BRIGHTNESS, None, &mut current, Some(&mut max));
                if rc != 0 && max > 0 {
                    ok = true;
                    break;
                }
                if attempt == 0 {
                    thread::sleep(Duration::from_millis(40));
                }
            }

            found.push(RawMonitor {
                id: if index == 0 { id.clone() } else { format!("{id}#{index}") },
                name: name.clone(),
                device: device.clone(),
                hmonitor,
                physical_index: index as u32,
                max: if ok { max } else { 0 },
                pct: if ok { to_pct(current, max) } else { 100 },
                method: if ok { Method::Ddc } else { Method::Gamma },
            });
        }

        let _ = DestroyPhysicalMonitors(&buf);
    }
    found
}

/// Kembalikan (id stabil, nama ramah) untuk sebuah `\\.\DISPLAYn`.
unsafe fn identity(device: &str) -> (String, String) {
    unsafe {
        let wide: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
        let mut dd = DISPLAY_DEVICEW::default();
        dd.cb = size_of::<DISPLAY_DEVICEW>() as u32;

        let ok = EnumDisplayDevicesW(
            PCWSTR(wide.as_ptr()),
            0,
            &mut dd,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
        .as_bool();

        if !ok {
            return (device.to_string(), device.to_string());
        }

        let id = utf16z(&dd.DeviceID);
        let name = utf16z(&dd.DeviceString);
        let id = if id.is_empty() { device.to_string() } else { id };
        let name = if name.is_empty() { device.to_string() } else { name };
        (id, name)
    }
}

fn to_pct(current: u32, max: u32) -> u8 {
    if max == 0 {
        return 100;
    }
    ((current as u64 * 100 / max as u64).min(100)) as u8
}

fn to_value(pct: u8, max: u32) -> u32 {
    let pct = pct.min(100) as u32;
    if max == 0 {
        return 0;
    }
    // Pembulatan: 50% dari max 100 tetap 50, 50% dari max 255 jadi 128.
    ((pct as u64 * max as u64 + 50) / 100) as u32
}

/// Bungkus handle physical monitor untuk satu operasi, lalu lepas kembali.
fn with_physical<R>(
    hmonitor: isize,
    index: u32,
    f: impl FnOnce(HANDLE) -> R,
) -> Result<R, Error> {
    unsafe {
        let hmon = as_hmonitor(hmonitor);

        let mut count = 0u32;
        GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &mut count).map_err(|_| Error::MonitorOff)?;
        if index >= count {
            return Err(Error::MonitorOff);
        }

        let mut buf = vec![PHYSICAL_MONITOR::default(); count as usize];
        GetPhysicalMonitorsFromHMONITOR(hmon, &mut buf).map_err(|_| Error::MonitorOff)?;

        let h = buf[index as usize].hPhysicalMonitor;
        let out = f(h);

        let _ = DestroyPhysicalMonitors(&buf);
        Ok(out)
    }
}

/// Tulis brightness (0–100%).
pub fn set_pct(hmonitor: isize, index: u32, max: u32, pct: u8) -> Result<(), Error> {
    if max == 0 {
        return Err(Error::Unsupported);
    }
    let value = to_value(pct, max);
    let ok = with_physical(hmonitor, index, |h| unsafe {
        SetVCPFeature(h, VCP_BRIGHTNESS, value) != 0
    })?;
    if ok { Ok(()) } else { Err(Error::NoResponse) }
}

/// Baca brightness sekarang. Mengembalikan (persen, max).
pub fn get_pct(hmonitor: isize, index: u32) -> Result<(u8, u32), Error> {
    let (rc, current, max) = with_physical(hmonitor, index, |h| {
        let mut current = 0u32;
        let mut max = 0u32;
        let rc = unsafe {
            GetVCPFeatureAndVCPFeatureReply(h, VCP_BRIGHTNESS, None, &mut current, Some(&mut max))
        };
        (rc, current, max)
    })?;

    if rc == 0 || max == 0 {
        return Err(Error::NoResponse);
    }
    Ok((to_pct(current, max), max))
}