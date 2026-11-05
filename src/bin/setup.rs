#![windows_subsystem = "windows"]

use std::ffi::c_void;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use windows::core::{HSTRING, Interface, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistFile,
};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RegDeleteKeyValueW, RegDeleteTreeW, RegSetKeyValueW,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetWindowThreadProcessId, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MessageBoxW,
    PostMessageW, WM_CLOSE,
};

include!(concat!(env!("OUT_DIR"), "/setup_payload.rs"));

const APP_NAME: &str = "Brightfly";
const EXE_NAME: &str = "brightfly.exe";
const UNINSTALL_NAME: &str = "uninstall.exe";
const CONTROLLER_CLASS: &str = "BrightflyController";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const UNINSTALL_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Brightfly";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if has_flag(&args, "--help") || has_flag(&args, "-h") {
        message(&usage(), false);
        return;
    }

    let result = if has_flag(&args, "--uninstall") || own_name_is_uninstall() {
        uninstall(&args)
    } else {
        install(&args)
    };

    if let Err(error) = result {
        if !silent(&args) {
            message(&format!("Brightfly: {error}"), true);
        }
        std::process::exit(1);
    }
}

fn usage() -> String {
    format!(
        "Brightfly Setup {}\n\n\
         setup.exe                      pasang dan jalankan\n\
         setup.exe --no-autostart       pasang tanpa autostart\n\
         setup.exe --no-start           pasang tanpa menjalankan aplikasi\n\
         setup.exe --quiet              tanpa dialog\n\n\
         uninstall.exe                  hapus (config dipertahankan)\n\
         uninstall.exe --purge-config   hapus termasuk config",
        env!("CARGO_PKG_VERSION")
    )
}

fn install(args: &[String]) -> Result<(), String> {
    if !READY {
        return Err(
            "payload tidak tertanam. Buat paket dengan scripts\\package.ps1.".into(),
        );
    }

    let no_autostart = has_flag(args, "--no-autostart");
    let no_start = has_flag(args, "--no-start");

    let dir = install_dir()?;
    stop_running();

    fs::create_dir_all(&dir).map_err(|error| format!("membuat {}: {error}", dir.display()))?;

    let app = dir.join(EXE_NAME);
    write_file(&app, APP_EXE)?;

    if !README.is_empty() {
        write_file(&dir.join("README.md"), README)?;
    }

    let uninstaller = dir.join(UNINSTALL_NAME);
    let source = current_exe()?;
    if source != uninstaller {
        copy_file(&source, &uninstaller)?;
    }

    create_shortcut(&app, &dir)?;
    register(&app, &dir)?;

    if no_autostart {
        remove_run()?;
    } else {
        set_run(&app)?;
    }

    if !no_start {
        Command::new(&app)
            .current_dir(&dir)
            .spawn()
            .map_err(|error| format!("menjalankan {}: {error}", app.display()))?;
    }

    if !silent(args) {
        let autostart = if no_autostart { "tidak aktif" } else { "aktif" };
        message(
            &format!(
                "Brightfly {} terpasang.\n\nFolder  : {}\nAutostart: {}\n\nUninstall lewat Settings > Apps, atau jalankan uninstall.exe di folder tersebut.",
                env!("CARGO_PKG_VERSION"),
                dir.display(),
                autostart
            ),
            false,
        );
    }

    Ok(())
}

fn uninstall(args: &[String]) -> Result<(), String> {
    let purge_config = has_flag(args, "--purge-config");
    let dir = install_dir()?;

    stop_running();

    remove_run()?;
    remove_uninstall_key()?;

    if let Ok(shortcut) = shortcut_path()
        && shortcut.exists()
    {
        fs::remove_file(&shortcut)
            .map_err(|error| format!("menghapus {}: {error}", shortcut.display()))?;
    }

    if purge_config {
        let config = config_dir()?;
        if config.exists() {
            fs::remove_dir_all(&config)
                .map_err(|error| format!("menghapus {}: {error}", config.display()))?;
        }
    }

    if !silent(args) {
        message(
            if purge_config {
                "Brightfly sudah di-uninstall; config juga dihapus."
            } else {
                "Brightfly sudah di-uninstall. Config dipertahankan."
            },
            false,
        );
    }

    if dir.exists() {
        schedule_remove_dir(&dir);
    }

    Ok(())
}

fn install_dir() -> Result<PathBuf, String> {
    Ok(env_dir("LOCALAPPDATA")?.join("Programs").join(APP_NAME))
}

fn config_dir() -> Result<PathBuf, String> {
    Ok(env_dir("APPDATA")?.join(APP_NAME))
}

fn shortcut_path() -> Result<PathBuf, String> {
    Ok(env_dir("APPDATA")?
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(format!("{APP_NAME}.lnk")))
}

fn env_dir(variable: &str) -> Result<PathBuf, String> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{variable} tidak ditemukan"))
}

fn register(app: &Path, dir: &Path) -> Result<(), String> {
    let uninstaller = dir.join(UNINSTALL_NAME);
    let uninstall_command = format!("\"{}\"", uninstaller.display());

    set_string(UNINSTALL_KEY, "DisplayName", APP_NAME)?;
    set_string(UNINSTALL_KEY, "DisplayVersion", env!("CARGO_PKG_VERSION"))?;
    set_string(
        UNINSTALL_KEY,
        "DisplayIcon",
        &format!("{},0", app.display()),
    )?;
    set_string(
        UNINSTALL_KEY,
        "InstallLocation",
        &dir.display().to_string(),
    )?;
    set_string(UNINSTALL_KEY, "UninstallString", &uninstall_command)?;
    set_string(
        UNINSTALL_KEY,
        "QuietUninstallString",
        &format!("{uninstall_command} --quiet"),
    )?;
    set_dword(UNINSTALL_KEY, "NoModify", 1)?;
    set_dword(UNINSTALL_KEY, "NoRepair", 1)?;
    set_dword(
        UNINSTALL_KEY,
        "EstimatedSize",
        (APP_EXE.len() / 1024) as u32,
    )?;

    Ok(())
}

fn set_run(app: &Path) -> Result<(), String> {
    set_string(RUN_KEY, APP_NAME, &format!("\"{}\"", app.display()))
}

fn remove_run() -> Result<(), String> {
    let status = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(APP_NAME),
        )
    };
    ignore_missing(status, "menghapus autostart")
}

fn remove_uninstall_key() -> Result<(), String> {
    let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL_KEY)) };
    ignore_missing(status, "menghapus entri Apps & features")
}

fn set_string(key: &str, name: &str, value: &str) -> Result<(), String> {
    let mut data: Vec<u16> = value.encode_utf16().collect();
    data.push(0);

    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            &HSTRING::from(name),
            REG_SZ.0,
            Some(data.as_ptr() as *const c_void),
            (data.len() * size_of::<u16>()) as u32,
        )
    };

    status
        .ok()
        .map_err(|error| format!("menulis registry {key}\\{name}: {error}"))
}

fn set_dword(key: &str, name: &str, value: u32) -> Result<(), String> {
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            &HSTRING::from(name),
            REG_DWORD.0,
            Some((&value as *const u32).cast()),
            size_of::<u32>() as u32,
        )
    };

    status
        .ok()
        .map_err(|error| format!("menulis registry {key}\\{name}: {error}"))
}

fn ignore_missing(status: windows::Win32::Foundation::WIN32_ERROR, action: &str) -> Result<(), String> {
    if status.is_ok() || status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(format!("{action}: {status:?}"))
    }
}

fn create_shortcut(app: &Path, dir: &Path) -> Result<(), String> {
    let shortcut = shortcut_path()?;
    if let Some(parent) = shortcut.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("membuat {}: {error}", parent.display()))?;
    }

    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                    .map_err(|error| format!("shortcut: {error}"))?;

            let target = HSTRING::from(app.as_os_str().to_string_lossy().as_ref());
            let working = HSTRING::from(dir.as_os_str().to_string_lossy().as_ref());
            let icon = HSTRING::from(format!("{},0", app.display()));

            link.SetPath(&target)
                .map_err(|error| format!("shortcut target: {error}"))?;
            link.SetWorkingDirectory(&working)
                .map_err(|error| format!("shortcut working dir: {error}"))?;
            link.SetIconLocation(&icon, 0)
                .map_err(|error| format!("shortcut icon: {error}"))?;
            link.SetDescription(&HSTRING::from(
                "Brightness slider untuk monitor eksternal",
            ))
            .map_err(|error| format!("shortcut description: {error}"))?;

            let persist: IPersistFile = link
                .cast()
                .map_err(|error| format!("shortcut persist: {error}"))?;
            let path = HSTRING::from(shortcut.as_os_str().to_string_lossy().as_ref());
            persist
                .Save(&path, true)
                .map_err(|error| format!("menyimpan shortcut: {error}"))
        })();

        if initialized {
            CoUninitialize();
        }

        result
    }
}

fn stop_running() {
    let class = HSTRING::from(CONTROLLER_CLASS);
    let Ok(hwnd) = (unsafe { FindWindowW(&class, PCWSTR::null()) }) else {
        return;
    };

    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }

    let deadline = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < deadline {
        if unsafe { FindWindowW(&class, PCWSTR::null()) }.is_err() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }

    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != 0
            && let Ok(process) = OpenProcess(PROCESS_TERMINATE, false, pid)
        {
            let _ = TerminateProcess(process, 1);
            let _ = CloseHandle(process);
        }
    }

    thread::sleep(Duration::from_millis(250));
}

// Windows cannot delete a running exe, so a detached cmd removes the folder.
fn schedule_remove_dir(dir: &Path) {
    let command = format!(
        "/c ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"",
        dir.display()
    );
    let _ = Command::new("cmd.exe")
        .raw_arg(command)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    retry("menulis", path, || fs::write(path, bytes).map(|_| ()))
}

fn copy_file(source: &Path, target: &Path) -> Result<(), String> {
    retry("menyalin ke", target, || fs::copy(source, target).map(|_| ()))
}

fn retry(
    action: &str,
    path: &Path,
    mut operation: impl FnMut() -> std::io::Result<()>,
) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..30 {
        match operation() {
            Ok(()) => return Ok(()),
            Err(error) => {
                last = error.to_string();
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Err(format!("{action} {}: {last}", path.display()))
}

fn current_exe() -> Result<PathBuf, String> {
    let mut buffer = vec![0u16; 1024];
    let len = unsafe { GetModuleFileNameW(None, &mut buffer) } as usize;
    if len == 0 || len >= buffer.len() {
        return Err("tidak bisa membaca path installer".into());
    }
    buffer.truncate(len);
    Ok(PathBuf::from(String::from_utf16_lossy(&buffer)))
}

fn own_name_is_uninstall() -> bool {
    current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().eq_ignore_ascii_case("uninstall"))
        })
        .unwrap_or(false)
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn silent(args: &[String]) -> bool {
    has_flag(args, "--quiet") || has_flag(args, "--silent")
}

fn message(text: &str, error: bool) {
    let text = HSTRING::from(text);
    let caption = HSTRING::from(APP_NAME);
    let icon = if error { MB_ICONERROR } else { MB_ICONINFORMATION };
    unsafe {
        MessageBoxW(None, &text, &caption, MB_OK | icon);
    }
}