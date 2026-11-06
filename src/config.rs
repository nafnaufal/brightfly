use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::monitor::Method;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorConfig {
    pub brightness: u8,
    pub method: Method,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            brightness: 50,
            method: Method::Ddc,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub sync: bool,
    pub start_with_windows: bool,
    pub step: u8,
    pub monitors: BTreeMap<String, MonitorConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sync: true,
            start_with_windows: true,
            step: 2,
            monitors: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        std::env::var_os("APPDATA")
            .map(|base| PathBuf::from(base).join("Brighty").join("config.ini"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, self.render())
    }

    pub fn parse(text: &str) -> Self {
        let mut config = Config::default();
        let mut section: Option<String> = None;

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    let name = name.trim().trim_matches('"');
                    section = if name.eq_ignore_ascii_case("general") {
                        None
                    } else {
                        Some(name.to_string())
                    };
                }
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();

            match &section {
                None => match key {
                    "sync" => config.sync = parse_bool(value),
                    "start_with_windows" => config.start_with_windows = parse_bool(value),
                    "step" => {
                        config.step = value
                            .parse::<u8>()
                            .unwrap_or(2)
                            .clamp(1, 25);
                    }
                    _ => {}
                },
                Some(id) => {
                    let entry = config
                        .monitors
                        .entry(id.clone())
                        .or_insert_with(MonitorConfig::default);
                    match key {
                        "brightness" => {
                            entry.brightness = value.parse::<u8>().unwrap_or(50).min(100);
                        }
                        "method" => {
                            if let Some(method) = Method::from_key(value) {
                                entry.method = method;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        config
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("[general]\n");
        out.push_str(&format!("sync={}\n", u8::from(self.sync)));
        out.push_str(&format!(
            "start_with_windows={}\n",
            u8::from(self.start_with_windows)
        ));
        out.push_str(&format!("step={}\n", self.step));

        for (id, monitor) in &self.monitors {
            out.push('\n');
            out.push_str(&format!("[{id}]\n"));
            out.push_str(&format!("brightness={}\n", monitor.brightness));
            out.push_str(&format!("method={}\n", monitor.method.key()));
        }

        out
    }
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_input_empty() {
        assert_eq!(Config::parse(""), Config::default());
    }

    #[test]
    fn parses_general_section() {
        let cfg = Config::parse("[general]\nsync=0\nstart_with_windows=0\nstep=5\n");
        assert!(!cfg.sync);
        assert!(!cfg.start_with_windows);
        assert_eq!(cfg.step, 5);
    }

    #[test]
    fn parses_monitor_section_with_device_path() {
        let id = r"\\?\DISPLAY#GSM5B7C#5&2b0f7f1&0&UID4352#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        let text = format!("[general]\nsync=1\n\n[{id}]\nbrightness=64\nmethod=ddc\n");
        let cfg = Config::parse(&text);

        let monitor = cfg.monitors.get(id).expect("monitor section missing");
        assert_eq!(monitor.brightness, 64);
        assert_eq!(monitor.method, Method::Ddc);
    }

    #[test]
    fn ignores_unknown_keys_and_garbage() {
        let cfg = Config::parse("color=red\n[general]\nfoo=bar\nstep=not-a-number\n");
        assert_eq!(cfg.step, 2, "nilai rusak harus jatuh ke default");
        assert_eq!(cfg.monitors.len(), 0);
    }

    #[test]
    fn clamps_out_of_range_values() {
        let text = "[x]\nbrightness=250\nmethod=gamma\n";
        let cfg = Config::parse(text);
        assert_eq!(cfg.monitors["x"].brightness, 100);
        assert_eq!(cfg.monitors["x"].method, Method::Gamma);
    }

    #[test]
    fn comments_are_skipped() {
        let cfg = Config::parse("# note\n[general]\n; other\nsync=0\n");
        assert!(!cfg.sync);
    }

    #[test]
    fn render_then_parse_is_identity() {
        let mut cfg = Config::default();
        cfg.sync = false;
        cfg.step = 3;
        cfg.monitors.insert(
            r"\\?\DISPLAY#ABC#1&2&3#{guid}".to_string(),
            MonitorConfig {
                brightness: 77,
                method: Method::Gamma,
            },
        );

        let roundtrip = Config::parse(&cfg.render());
        assert_eq!(roundtrip, cfg);
    }
}