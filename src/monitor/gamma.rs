use std::ffi::c_void;

use windows::core::{w, PCWSTR};
use windows::Win32::Graphics::Gdi::{CreateDCW, DeleteDC, HDC};
use windows::Win32::UI::ColorSystem::{GetDeviceGammaRamp, SetDeviceGammaRamp};

pub type Ramp = [u16; 768];

fn open_dc(device: &str) -> Option<HDC> {
    let wide: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
    let dc = unsafe { CreateDCW(w!("DISPLAY"), PCWSTR(wide.as_ptr()), PCWSTR::null(), None) };
    if dc.is_invalid() { None } else { Some(dc) }
}

pub fn read(device: &str) -> Option<Ramp> {
    let dc = open_dc(device)?;
    let mut ramp: Ramp = [0; 768];
    let ok = unsafe { GetDeviceGammaRamp(dc, ramp.as_mut_ptr() as *mut c_void) };
    unsafe {
        let _ = DeleteDC(dc);
    }
    if ok.as_bool() { Some(ramp) } else { None }
}

pub fn write(device: &str, ramp: &Ramp) -> bool {
    let Some(dc) = open_dc(device) else {
        return false;
    };
    let ok = unsafe { SetDeviceGammaRamp(dc, ramp.as_ptr() as *const c_void) };
    unsafe {
        let _ = DeleteDC(dc);
    }
    ok.as_bool()
}

pub fn scaled(original: &Ramp, factor: f32) -> Ramp {
    let factor = factor.clamp(0.0, 1.0);
    if factor >= 1.0 {
        return *original;
    }
    let mut out = *original;
    for v in out.iter_mut() {
        *v = ((*v as f32) * factor).round().clamp(0.0, 65535.0) as u16;
    }
    out
}

pub fn factor_for(pct: u8) -> f32 {
    let p = (pct.min(100) as f32) / 100.0;
    0.25 + 0.75 * p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_pct_returns_original() {
        let ramp: Ramp = [1000; 768];
        assert_eq!(scaled(&ramp, factor_for(100)), ramp);
    }

    #[test]
    fn zero_pct_is_quarter() {
        let ramp: Ramp = [1000; 768];
        let out = scaled(&ramp, factor_for(0));
        assert_eq!(out[0], 250, "1000 * 0.25");
    }

    #[test]
    fn halfway() {
        let ramp: Ramp = [65535; 768];
        let out = scaled(&ramp, 0.5);
        assert_eq!(out[0], 32768);
    }
}