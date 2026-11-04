mod app;
mod autostart;
mod config;
mod monitor;
mod ui;

use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use config::{Config, MonitorConfig};
use monitor::{Method, State, Supervisor};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str);

    match command {
        None => app::run(),
        Some("probe") => probe(),
        Some("set") => {
            let Some(raw) = args.get(1) else {
                eprintln!("brightfly set: butuh nilai 0..100");
                return ExitCode::from(2);
            };
            let Ok(pct) = raw.parse::<u8>() else {
                eprintln!("brightfly set: '{raw}' bukan angka 0..100");
                return ExitCode::from(2);
            };
            set(pct)
        }
        Some("help") | Some("-h") | Some("--help") => {
            usage();
            ExitCode::SUCCESS
        }
        Some("stress") => stress(),
        Some("--version") | Some("-V") => {
            println!("brightfly {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("perintah tidak dikenal: {other}\n");
            usage();
            ExitCode::from(2)
        }
    }
}

fn usage() {
    println!(
        "brightfly {} — kontrol brightness monitor eksternal

Pemakaian:
  brightfly probe          Tampilkan monitor, metode kontrol, dan nilai sekarang
  brightfly set <0..100>   Set semua monitor lalu baca ulang untuk verifikasi
  brightfly stress         Kirim 200 perubahan cepat untuk menguji coalescing
  brightfly --version

Tanpa argumen, aplikasi tray akan dijalankan.",
        env!("CARGO_PKG_VERSION")
    );
}

fn probe() -> ExitCode {
    let supervisor = Supervisor::new();
    let infos = supervisor.monitors();

    if infos.is_empty() {
        eprintln!("Tidak ada monitor terdeteksi.");
        return ExitCode::FAILURE;
    }

    for (index, monitor) in infos.iter().enumerate() {
        println!("== Monitor {} ==", index + 1);
        println!("  name     : {}", monitor.name);
        println!("  device   : {}", monitor.device);
        println!("  id       : {}", monitor.id);
        println!("  physical : #{}", monitor.physical_index);
        println!("  method   : {}", monitor.method.key());
        println!("  range    : 0..{}", monitor.max_brightness);
        println!("  current  : {}%", monitor.pct);
        println!("  state    : {}", monitor.state.key());
        println!();
    }

    let ddc = infos.iter().filter(|m| m.method == Method::Ddc).count();
    let gamma = infos.iter().filter(|m| m.method == Method::Gamma).count();
    let none = infos.iter().filter(|m| m.method == Method::Unsupported).count();

    println!(
        "Ringkasan: {} monitor — {} DDC/CI, {} gamma-only, {} tidak didukung",
        infos.len(),
        ddc,
        gamma,
        none
    );

    if ddc == 0 {
        println!();
        println!("PERINGATAN: tidak ada monitor yang merespons DDC/CI.");
        println!("Cek: DDC/CI aktif di OSD monitor, dan kabel DP/HDMI langsung ke GPU.");
    }

    supervisor.stop_all();
    ExitCode::SUCCESS
}

fn set(pct: u8) -> ExitCode {
    let pct = pct.min(100);
    let supervisor = Supervisor::new();

    if supervisor.is_empty() {
        eprintln!("Tidak ada monitor terdeteksi.");
        return ExitCode::FAILURE;
    }

    supervisor.set_all(pct);

    // Beri waktu worker menyelesaikan panggilan DDC (bisa lambat).
    thread::sleep(Duration::from_millis(600));
    supervisor.refresh_all();
    thread::sleep(Duration::from_millis(400));

    let infos = supervisor.monitors();
    let mut failures = 0;

    for monitor in &infos {
        println!(
            "{:<32} {:>3}%   method={}  state={}",
            monitor.name,
            monitor.pct,
            monitor.method.key(),
            monitor.state.key()
        );
        if monitor.state != State::Ready {
            failures += 1;
        }
    }

    // Simpan supaya sesi berikutnya memakai nilai yang sama.
    let mut config = Config::load();
    for monitor in &infos {
        config.monitors.insert(
            monitor.id.clone(),
            MonitorConfig {
                brightness: monitor.pct,
                method: monitor.method,
            },
        );
    }
    if let Err(error) = config.save() {
        eprintln!("Gagal menyimpan config: {error}");
    }

    supervisor.stop_all();

    if failures > 0 {
        eprintln!("{failures} monitor tidak mengonfirmasi perubahan.");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Simulasi drag slider: 200 perubahan beruntun.
///
/// Yang diuji bukan kecepatan DDC, tapi apakah pengiriman tetap non-blocking
/// dan apakah worker hanya mengeksekusi nilai terakhir (coalescing).
fn stress() -> ExitCode {
    const SEND_COUNT: u32 = 200;
    const FINAL: u8 = 62;

    let supervisor = Supervisor::new();

    if supervisor.is_empty() {
        eprintln!("Tidak ada monitor terdeteksi.");
        return ExitCode::FAILURE;
    }

    let started = Instant::now();
    for i in 0..SEND_COUNT {
        let pct = if i % 2 == 0 { 60 } else { FINAL };
        supervisor.set_all(pct);
        thread::sleep(Duration::from_millis(1));
    }
    let sending = started.elapsed();

    thread::sleep(Duration::from_millis(700));
    supervisor.refresh_all();
    thread::sleep(Duration::from_millis(400));

    let infos = supervisor.monitors();
    let worst = infos.iter().map(|m| m.pct.abs_diff(FINAL)).max().unwrap_or(0);

    println!(
        "Mengirim {SEND_COUNT} perubahan dalam {} ms (harus < 2000 ms)",
        sending.as_millis()
    );
    for monitor in &infos {
        println!(
            "  {:<32} {:>3}%  state={}",
            monitor.name,
            monitor.pct,
            monitor.state.key()
        );
    }
    println!("Selisih akhir terhadap target {FINAL}%: {worst} poin");

    supervisor.stop_all();

    let sending_ok = sending.as_millis() < 2000;
    let value_ok = worst <= 1;

    if sending_ok && value_ok {
        println!("LULUS");
        ExitCode::SUCCESS
    } else {
        eprintln!("GAGAL");
        ExitCode::FAILURE
    }
}