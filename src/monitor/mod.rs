pub mod ddc;
pub mod edid;
pub mod gamma;
pub mod worker;

pub use worker::Supervisor;

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    #[default]
    Unsupported,
    Ddc,
    Gamma,
}

impl Method {
    pub fn key(self) -> &'static str {
        match self {
            Method::Ddc => "ddc",
            Method::Gamma => "gamma",
            Method::Unsupported => "none",
        }
    }

    pub fn from_key(s: &str) -> Option<Method> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ddc" => Some(Method::Ddc),
            "gamma" => Some(Method::Gamma),
            "none" | "unsupported" => Some(Method::Unsupported),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    Unresponsive,
    MonitorOff,
    Unsupported,
}

impl State {
    pub fn key(self) -> &'static str {
        match self {
            State::Ready => "ready",
            State::Unresponsive => "unresponsive",
            State::MonitorOff => "off",
            State::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub id: String,
    pub path: String,
    pub name: String,
    pub device: String,
    pub physical_index: u32,
    pub max_brightness: u32,
    pub method: Method,
    pub pct: u8,
    pub state: State,
    #[allow(dead_code)]
    pub stuck: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Unsupported,
    NoResponse,
    MonitorOff,
    #[allow(dead_code)]
    Enumeration,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Error::Unsupported => "monitor tidak mendukung DDC/CI",
            Error::NoResponse => "monitor tidak merespons",
            Error::MonitorOff => "monitor tidak tersedia",
            Error::Enumeration => "gagal enumerasi monitor",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for Error {}

impl From<Error> for State {
    fn from(e: Error) -> State {
        match e {
            Error::Unsupported => State::Unsupported,
            Error::NoResponse => State::Unresponsive,
            Error::MonitorOff | Error::Enumeration => State::MonitorOff,
        }
    }
}