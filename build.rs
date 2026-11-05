//! Build script: menanamkan icon dan version info ke exe.
//!
//! `rc.exe` tidak ada di PATH pada mesin tanpa Visual Studio build tools,
//! jadi lokasinya dicari di Windows SDK. Kalau tetap tidak ditemukan,
//! build dilanjutkan tanpa resource dan tray memakai icon bawaan Windows.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ICON_PATH: &str = "assets/brightfly.ico";

fn main() {
    println!("cargo:rerun-if-changed={ICON_PATH}");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR tidak ada"));
    emit_setup_payload(&out_dir);

    let Some(rc) = find_rc() else {
        println!(
            "cargo:warning=rc.exe tidak ditemukan; exe dibangun tanpa icon resource"
        );
        return;
    };

    let icon = fs::canonicalize(ICON_PATH).expect("icon tidak ditemukan");
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());

    link_resource(&rc, &out_dir, &icon, &version, "brightfly", "Brightfly", "brightfly.exe");
    link_resource(
        &rc,
        &out_dir,
        &icon,
        &version,
        "brightfly-setup",
        "Brightfly Setup",
        "setup.exe",
    );
}

/// Membuat `setup_payload.rs` yang di-`include!` oleh `src/bin/setup.rs`.
///
/// Payload hanya terisi kalau `BRIGHTFLY_PAYLOAD_DIR` diarahkan ke folder yang
/// berisi `brightfly.exe` dan `README.md`. `scripts/package.ps1` yang mengatur
/// variabel itu; `cargo build` biasa tetap berhasil dengan payload kosong.
fn emit_setup_payload(out_dir: &Path) {
    println!("cargo:rerun-if-env-changed=BRIGHTFLY_PAYLOAD_DIR");

    let dir = env::var_os("BRIGHTFLY_PAYLOAD_DIR").map(PathBuf::from);
    let mut code = String::new();
    let mut ready = true;

    for (name, file) in [("APP_EXE", "brightfly.exe"), ("README", "README.md")] {
        let path = dir.as_ref().map(|dir| dir.join(file)).filter(|path| path.is_file());
        match path {
            Some(path) => {
                let path = fs::canonicalize(&path).unwrap_or(path);
                println!("cargo:rerun-if-changed={}", path.display());
                let literal = path.to_string_lossy().replace('\\', "/");
                code.push_str(&format!(
                    "pub static {name}: &[u8] = include_bytes!(r#\"{literal}\"#);\n"
                ));
            }
            None => {
                ready = false;
                code.push_str(&format!("pub static {name}: &[u8] = &[];\n"));
            }
        }
    }

    code.push_str(&format!("pub const READY: bool = {ready};\n"));
    fs::write(out_dir.join("setup_payload.rs"), code).expect("gagal menulis setup_payload.rs");
}

fn link_resource(
    rc: &Path,
    out_dir: &Path,
    icon: &Path,
    version: &str,
    bin: &str,
    description: &str,
    original: &str,
) {
    let rc_file = out_dir.join(format!("{bin}.rc"));
    let script = resource_script(icon, version, description, original);
    fs::write(&rc_file, script).expect("gagal menulis .rc");

    let res_file = out_dir.join(format!("{bin}.res"));
    let status = Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_file)
        .arg(&rc_file)
        .status();

    match status {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg-bin={bin}={}", res_file.display());
        }
        Ok(status) => println!(
            "cargo:warning=rc.exe keluar dengan status {status}; resource {bin} dilewati"
        ),
        Err(error) => println!("cargo:warning=gagal menjalankan rc.exe: {error}"),
    }
}

fn resource_script(icon: &Path, version: &str, description: &str, original: &str) -> String {
    // Resource ID 1 dipakai `LoadImageW` di tray.
    let icon = icon.display().to_string().replace('\\', "\\\\");
    let numeric = numeric_version(version);

    format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "brightfly"
      VALUE "OriginalFilename", "{original}"
      VALUE "ProductName", "Brightfly"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}

/// `1.2.3` → `1,2,3,0`; bagian non-angka diabaikan.
fn numeric_version(version: &str) -> String {
    let mut parts = [0u16; 4];
    for (slot, part) in parts.iter_mut().zip(version.split(['.', '-', '+'])) {
        *slot = part.parse().unwrap_or(0);
    }
    format!("{},{},{},{}", parts[0], parts[1], parts[2], parts[3])
}

fn find_rc() -> Option<PathBuf> {
    if let Some(path) = env::var_os("RC") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    for root in kit_roots() {
        let bin = root.join("bin");
        let Ok(entries) = fs::read_dir(&bin) else {
            continue;
        };

        let mut versions: Vec<PathBuf> =
            entries.filter_map(|entry| entry.ok()).map(|entry| entry.path()).collect();
        versions.sort();
        versions.reverse();

        for dir in versions {
            for arch in host_arches() {
                let candidate = dir.join(arch).join("rc.exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    let path = PathBuf::from("rc.exe");
    Command::new(&path)
        .arg("/?")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| path)
}

fn kit_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = env::var_os(var) {
            let root = PathBuf::from(base).join("Windows Kits").join("10");
            if root.is_dir() {
                roots.push(root);
            }
        }
    }
    roots
}

fn host_arches() -> &'static [&'static str] {
    match env::var("HOST").unwrap_or_default().as_str() {
        host if host.starts_with("aarch64") => &["arm64", "x64"],
        host if host.starts_with("i686") => &["x86", "x64"],
        _ => &["x64", "x86"],
    }
}