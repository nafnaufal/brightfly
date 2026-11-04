//! Worker thread per monitor.
//!
//! Panggilan DDC/CI memblokir dan tidak bisa dibatalkan. Karena itu tiap
//! monitor mendapat thread sendiri: monitor yang menggantung tidak boleh
//! membekukan UI atau monitor lain.
//!
//! Antrian sengaja diganti dengan satu slot "desired" (coalescing). Kalau
//! pengguna menggeser slider cepat, worker hanya mengerjakan nilai terakhir,
//! bukan seluruh riwayat.

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use super::ddc::{self, RawMonitor};
use super::gamma::{self, Ramp};
use super::{Error, Method, MonitorInfo, State};

/// Batas waktu sebelum sebuah monitor dianggap macet.
const STUCK_MS: u128 = 2000;

#[derive(Clone, Copy)]
enum Cmd {
    Set(u8),
    Refresh,
}

struct Inner {
    desired: Option<Cmd>,
    stopping: bool,
    pct: u8,
    state: State,
    method: Method,
    max: u32,
    started: Option<Instant>,
}

struct Shared {
    inner: Mutex<Inner>,
    cv: Condvar,
}

struct Ctx {
    hmonitor: isize,
    index: u32,
    max: u32,
    device: String,
    method: Method,
}

pub struct MonitorWorker {
    id: String,
    name: String,
    device: String,
    index: u32,
    shared: Arc<Shared>,
    /// Tidak pernah di-join: proses harus tetap bisa keluar walau ada
    /// panggilan DDC yang menggantung.
    _join: Option<JoinHandle<()>>,
}

impl MonitorWorker {
    pub fn spawn(m: RawMonitor, initial_pct: u8) -> Self {
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner {
                desired: None,
                stopping: false,
                pct: initial_pct.min(100),
                state: if m.method == Method::Unsupported {
                    State::Unsupported
                } else {
                    State::Ready
                },
                method: m.method,
                max: m.max,
                started: None,
            }),
            cv: Condvar::new(),
        });

        let ctx = Ctx {
            hmonitor: m.hmonitor,
            index: m.physical_index,
            max: m.max,
            device: m.device.clone(),
            method: m.method,
        };

        let thread_shared = Arc::clone(&shared);
        let join = thread::Builder::new()
            .name(format!("brightfly-{}", m.physical_index))
            .spawn(move || run(thread_shared, ctx))
            .ok();

        Self {
            id: m.id,
            name: m.name,
            device: m.device,
            index: m.physical_index,
            shared,
            _join: join,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.shared.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    pub fn pct(&self) -> u8 {
        self.lock().pct
    }

    pub fn info(&self) -> MonitorInfo {
        let guard = self.lock();
        MonitorInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            device: self.device.clone(),
            physical_index: self.index,
            max_brightness: guard.max,
            method: guard.method,
            pct: guard.pct,
            state: guard.state,
            stuck: guard
                .started
                .map(|t| t.elapsed().as_millis() > STUCK_MS)
                .unwrap_or(false),
        }
    }

    pub fn set_pct(&self, pct: u8) {
        let mut guard = self.lock();
        if guard.state == State::Unsupported {
            return;
        }
        guard.desired = Some(Cmd::Set(pct.min(100)));
        drop(guard);
        self.shared.cv.notify_one();
    }

    pub fn refresh(&self) {
        let mut guard = self.lock();
        if guard.state == State::Unsupported {
            return;
        }
        guard.desired = Some(Cmd::Refresh);
        drop(guard);
        self.shared.cv.notify_one();
    }

    pub fn stop(&self) {
        let mut guard = self.lock();
        guard.stopping = true;
        guard.desired = None;
        drop(guard);
        self.shared.cv.notify_all();
    }
}

fn run(shared: Arc<Shared>, ctx: Ctx) {
    // Ramp asli dibaca sekali, sebelum ada perubahan apa pun.
    let original: Option<Ramp> = if ctx.method == Method::Gamma {
        gamma::read(&ctx.device)
    } else {
        None
    };

    let mut guard = shared.inner.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        while guard.desired.is_none() && !guard.stopping {
            guard = shared.cv.wait(guard).unwrap_or_else(|e| e.into_inner());
        }

        if guard.desired.is_none() && guard.stopping {
            break;
        }

        let Some(cmd) = guard.desired.take() else {
            continue;
        };
        let current_pct = guard.pct;

        guard.started = Some(Instant::now());
        drop(guard);

        let outcome = execute(&ctx, cmd, original.as_ref(), current_pct);

        guard = shared.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.started = None;
        match outcome {
            Ok((pct, max)) => {
                guard.pct = pct;
                if max > 0 {
                    guard.max = max;
                }
                guard.state = State::Ready;
            }
            Err(e) => guard.state = State::from(e),
        }
    }
}

fn execute(ctx: &Ctx, cmd: Cmd, original: Option<&Ramp>, current_pct: u8) -> Result<(u8, u32), Error> {
    match ctx.method {
        Method::Ddc => match cmd {
            Cmd::Set(pct) => {
                ddc::set_pct(ctx.hmonitor, ctx.index, ctx.max, pct)?;
                Ok((pct, ctx.max))
            }
            Cmd::Refresh => ddc::get_pct(ctx.hmonitor, ctx.index),
        },
        Method::Gamma => {
            let Some(original) = original else {
                return Err(Error::Unsupported);
            };
            let pct = match cmd {
                Cmd::Set(pct) => pct,
                // Gamma tidak bisa dibaca balik; pertahankan nilai terakhir.
                Cmd::Refresh => current_pct,
            };
            let ramp = gamma::scaled(original, gamma::factor_for(pct));
            if !gamma::write(&ctx.device, &ramp) {
                return Err(Error::NoResponse);
            }
            Ok((pct, 100))
        }
        Method::Unsupported => Err(Error::Unsupported),
    }
}

/// Kumpulan worker untuk semua monitor yang terdeteksi.
pub struct Supervisor {
    workers: Vec<MonitorWorker>,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Supervisor {
    pub fn new() -> Self {
        let mut me = Self {
            workers: Vec::new(),
        };
        me.rescan();
        me
    }

    /// Bangun ulang daftar monitor. Dipanggil saat `WM_DISPLAYCHANGE` atau resume.
    pub fn rescan(&mut self) {
        let previous: Vec<(String, u8)> =
            self.workers.iter().map(|w| (w.id(), w.pct())).collect();

        for worker in &self.workers {
            worker.stop();
        }
        self.workers.clear();

        for raw in ddc::enumerate() {
            let pct = previous
                .iter()
                .find(|(id, _)| *id == raw.id)
                .map(|(_, pct)| *pct)
                .unwrap_or(raw.pct);
            self.workers.push(MonitorWorker::spawn(raw, pct));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.workers.is_empty()
    }

    pub fn monitors(&self) -> Vec<MonitorInfo> {
        self.workers.iter().map(|w| w.info()).collect()
    }

    /// Dipakai flyout per-monitor (multi monitor).
    #[allow(dead_code)]
    pub fn set_pct(&self, id: &str, pct: u8) -> bool {
        for worker in &self.workers {
            if worker.id() == id {
                worker.set_pct(pct);
                return true;
            }
        }
        false
    }

    pub fn set_all(&self, pct: u8) {
        for worker in &self.workers {
            worker.set_pct(pct);
        }
    }

    pub fn refresh_all(&self) {
        for worker in &self.workers {
            worker.refresh();
        }
    }

    pub fn stop_all(&self) {
        for worker in &self.workers {
            worker.stop();
        }
    }
}