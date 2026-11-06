use std::ffi::c_void;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("Brighty");

pub fn is_enabled() -> bool {
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
        .is_ok()
    }
}

pub fn set_enabled(enabled: bool) -> bool {
    if !enabled {
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME) };
        return status.is_ok() || status == ERROR_FILE_NOT_FOUND;
    }

    let Some(path) = executable_path() else {
        return false;
    };

    // Quote the path so spaces in the folder name do not split the command line.
    let mut command: Vec<u16> = Vec::with_capacity(path.len() + 3);
    command.push(b'"' as u16);
    command.extend(path.encode_utf16());
    command.push(b'"' as u16);
    command.push(0);

    let bytes = (command.len() * std::mem::size_of::<u16>()) as u32;
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            REG_SZ.0,
            Some(command.as_ptr() as *const c_void),
            bytes,
        )
    };
    status.is_ok()
}

fn executable_path() -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    let len = unsafe { GetModuleFileNameW(None, &mut buffer) } as usize;
    if len == 0 || len >= buffer.len() {
        return None;
    }
    buffer.truncate(len);
    Some(String::from_utf16_lossy(&buffer))
}