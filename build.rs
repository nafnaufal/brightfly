use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ICON_PATH: &str = "assets/brightfly.ico";

fn main() {
    println!("cargo:rerun-if-changed={ICON_PATH}");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));

    let Some(rc) = find_rc() else {
        println!(
            "cargo:warning=rc.exe not found; building without an icon resource"
        );
        return;
    };

    let icon = fs::canonicalize(ICON_PATH).expect("icon not found");
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());

    link_resource(&rc, &out_dir, &icon, &version, "brightfly", "Brightfly", "brightfly.exe");
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
    fs::write(&rc_file, script).expect("failed to write .rc");

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
            "cargo:warning=rc.exe exited with status {status}; skipping resource {bin}"
        ),
        Err(error) => println!("cargo:warning=failed to run rc.exe: {error}"),
    }
}

fn resource_script(icon: &Path, version: &str, description: &str, original: &str) -> String {
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