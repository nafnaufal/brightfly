use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use super::ddc::{self, RawMonitor};
use super::gamma::{self, Ramp};
use super::{Error, Method, MonitorInfo, State};

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
    path: String,
    name: String,
    device: String,
    index: u32,
    shared: Arc<Shared>,
    // Never joined: the process must be able to exit even if a DDC call hangs.
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
            path: m.path,
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

    pub fn method(&self) -> Method {
        self.lock().method
    }

    pub fn info(&self) -> MonitorInfo {
        let guard = self.lock();
        MonitorInfo {
            id: self.id.clone(),
            path: self.path.clone(),
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

fn resolve_pct(
    previous: &[(String, u8)],
    saved: &BTreeMap<String, u8>,
    monitor: &RawMonitor,
) -> u8 {
    previous
        .iter()
        .find(|(id, _)| *id == monitor.id)
        .map(|(_, pct)| *pct)
        .or_else(|| saved.get(&monitor.id).copied())
        .unwrap_or(monitor.pct)
}

pub struct Supervisor {
    workers: Vec<MonitorWorker>,
    saved: Mutex<BTreeMap<String, u8>>,
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
            saved: Mutex::new(BTreeMap::new()),
        };
        let raw = ddc::enumerate();
        me.apply_scan(raw);
        me
    }

    pub fn apply_config(&self, values: &BTreeMap<String, u8>) {
        {
            let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
            saved.clear();
            for (id, pct) in values {
                saved.insert(id.clone(), (*pct).min(100));
            }
        }

        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        for worker in &self.workers {
            if let Some(pct) = saved.get(&worker.id()).copied() {
                worker.set_pct(pct);
            }
        }
    }

    pub fn apply_scan(&mut self, raw: Vec<RawMonitor>) {
        let previous: Vec<(String, u8)> =
            self.workers.iter().map(|w| (w.id(), w.pct())).collect();

        for worker in &self.workers {
            worker.stop();
        }
        self.workers.clear();

        let saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        for monitor in raw {
            let pct = resolve_pct(&previous, &saved, &monitor);
            self.workers.push(MonitorWorker::spawn(monitor, pct));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.workers.is_empty()
    }

    pub fn monitors(&self) -> Vec<MonitorInfo> {
        self.workers.iter().map(|w| w.info()).collect()
    }

    pub fn set_pct(&self, id: &str, pct: u8) -> bool {
        let pct = pct.min(100);
        for worker in &self.workers {
            if worker.id() == id {
                self.saved
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id.to_string(), pct);
                worker.set_pct(pct);
                return true;
            }
        }
        false
    }

    pub fn method_of(&self, id: &str) -> Option<Method> {
        self.workers
            .iter()
            .find(|worker| worker.id() == id)
            .map(|worker| worker.method())
    }

    pub fn set_all(&self, pct: u8) {
        let pct = pct.min(100);
        {
            let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
            for worker in &self.workers {
                saved.insert(worker.id(), pct);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(id: &str, pct: u8) -> RawMonitor {
        RawMonitor {
            id: id.to_string(),
            path: format!(r"\\?\DISPLAY#TEST#{id}"),
            name: "Test".to_string(),
            device: r"\\.\DISPLAY1".to_string(),
            hmonitor: 0,
            physical_index: 0,
            max: 100,
            pct,
            method: Method::Ddc,
        }
    }

    #[test]
    fn scan_keeps_value_currently_shown() {
        let previous = vec![("a".to_string(), 63)];
        let saved = BTreeMap::from([("a".to_string(), 20)]);
        assert_eq!(resolve_pct(&previous, &saved, &raw("a", 80)), 63);
    }

    #[test]
    fn scan_falls_back_to_saved_value() {
        let previous = Vec::new();
        let saved = BTreeMap::from([("a".to_string(), 20)]);
        assert_eq!(resolve_pct(&previous, &saved, &raw("a", 80)), 20);
    }

    #[test]
    fn scan_falls_back_to_hardware_value() {
        let previous = Vec::new();
        let saved = BTreeMap::new();
        assert_eq!(resolve_pct(&previous, &saved, &raw("a", 80)), 80);
    }
}