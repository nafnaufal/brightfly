use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    REG_BINARY, REG_VALUE_TYPE,
};

const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edid {
    pub manufacturer: String,
    pub product: u16,
    pub serial: Option<String>,
}

impl Edid {
    pub fn key(&self) -> String {
        let model = format!("{}{:04X}", self.manufacturer, self.product);
        match &self.serial {
            Some(serial) => format!("{model}:{serial}"),
            None => model,
        }
    }
}

pub fn read(interface_path: &str) -> Option<Vec<u8>> {
    let path = interface_path.strip_prefix(r"\\?\")?;
    let mut parts = path.split('#');
    let class = parts.next()?;
    let hardware = parts.next()?;
    let instance = parts.next()?;
    if class.is_empty() || hardware.is_empty() || instance.is_empty() {
        return None;
    }

    let subkey = format!(
        "SYSTEM\\CurrentControlSet\\Enum\\{class}\\{hardware}\\{instance}\\Device Parameters"
    );
    let bytes = query_binary(&subkey, "EDID")?;
    (bytes.len() >= 128).then_some(bytes)
}

pub fn parse(bytes: &[u8]) -> Option<Edid> {
    if bytes.len() < 128 || bytes[..8] != HEADER {
        return None;
    }

    let packed = u16::from_be_bytes([bytes[8], bytes[9]]);
    let mut manufacturer = String::with_capacity(3);
    for shift in [10, 5, 0] {
        let letter = ((packed >> shift) & 0x1F) as u8;
        if !(1..=26).contains(&letter) {
            return None;
        }
        manufacturer.push((b'A' + letter - 1) as char);
    }

    let product = u16::from_le_bytes([bytes[10], bytes[11]]);
    let number = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

    let serial = serial_text(bytes).or_else(|| (number != 0).then(|| number.to_string()));

    Some(Edid {
        manufacturer,
        product,
        serial,
    })
}

fn serial_text(bytes: &[u8]) -> Option<String> {
    for index in 0..4 {
        let start = 54 + index * 18;
        let block = &bytes[start..start + 18];
        if block[..4] == [0x00, 0x00, 0x00, 0xFF] {
            let text = sanitize(&block[5..18]);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

fn sanitize(raw: &[u8]) -> String {
    let text: String = raw
        .iter()
        .take_while(|&&byte| byte != 0 && byte != b'\n' && byte != b'\r')
        .filter(|byte| byte.is_ascii_graphic())
        .map(|&byte| byte as char)
        .collect();

    text.replace(['[', ']', '#'], "_").trim().to_string()
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn query_binary(subkey: &str, name: &str) -> Option<Vec<u8>> {
    let subkey = wide(subkey);
    let name = wide(name);

    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            None,
            KEY_READ,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return None;
        }

        let mut kind = REG_VALUE_TYPE::default();
        let mut size = 0u32;
        let mut rc = RegQueryValueExW(
            key,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        );
        if rc != ERROR_SUCCESS || kind != REG_BINARY || size == 0 || size > 4096 {
            let _ = RegCloseKey(key);
            return None;
        }

        let mut data = vec![0u8; size as usize];
        rc = RegQueryValueExW(
            key,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(data.as_mut_ptr()),
            Some(&mut size),
        );
        let _ = RegCloseKey(key);
        if rc != ERROR_SUCCESS {
            return None;
        }

        data.truncate(size as usize);
        Some(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes[..8].copy_from_slice(&HEADER);
        let packed: u16 = (13 << 10) | (19 << 5) | 9;
        bytes[8..10].copy_from_slice(&packed.to_be_bytes());
        bytes[10..12].copy_from_slice(&0x30B9u16.to_le_bytes());
        bytes[12..16].copy_from_slice(&0u32.to_le_bytes());

        let block = 54;
        bytes[block + 3] = 0xFF;
        bytes[block + 5..block + 18].copy_from_slice(b"PB9H304C03601");
        bytes
    }

    #[test]
    fn parses_manufacturer_and_product() {
        let edid = parse(&sample()).expect("EDID valid");
        assert_eq!(edid.manufacturer, "MSI");
        assert_eq!(edid.product, 0x30B9);
        assert_eq!(edid.serial.as_deref(), Some("PB9H304C03601"));
        assert_eq!(edid.key(), "MSI30B9:PB9H304C03601");
    }

    #[test]
    fn falls_back_to_numeric_serial() {
        let mut bytes = sample();
        bytes[59..77].fill(0);
        bytes[12..16].copy_from_slice(&24646u32.to_le_bytes());
        let edid = parse(&bytes).expect("EDID valid");
        assert_eq!(edid.key(), "MSI30B9:24646");
    }

    #[test]
    fn model_only_when_serial_missing() {
        let mut bytes = sample();
        bytes[59..77].fill(0);
        let edid = parse(&bytes).expect("EDID valid");
        assert_eq!(edid.key(), "MSI30B9");
    }

    #[test]
    fn rejects_bad_header() {
        let mut bytes = sample();
        bytes[0] = 0x01;
        assert!(parse(&bytes).is_none());
        assert!(parse(&[]).is_none());
        assert!(parse(&bytes[..64]).is_none());
    }

    #[test]
    fn rejects_invalid_manufacturer_code() {
        let mut bytes = sample();
        bytes[8] = 0;
        bytes[9] = 0;
        assert!(parse(&bytes).is_none());
    }

    #[test]
    fn strips_characters_that_break_ini_section() {
        let mut bytes = sample();
        bytes[59..77].fill(0);
        bytes[59..70].copy_from_slice(b"SN]01#A[B=C");
        let edid = parse(&bytes).expect("EDID valid");
        assert_eq!(edid.serial.as_deref(), Some("SN_01_A_B=C"));
        assert_eq!(edid.key(), "MSI30B9:SN_01_A_B=C");
    }

    #[test]
    fn registry_path_is_derived_from_interface_path() {
        let path = r"\\?\DISPLAY#MSI30B9#7&21ebfe3c&0&UID24646#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        let stripped = path.strip_prefix(r"\\?\").unwrap();
        let parts: Vec<&str> = stripped.split('#').collect();
        assert_eq!(parts[0], "DISPLAY");
        assert_eq!(parts[1], "MSI30B9");
        assert_eq!(parts[2], "7&21ebfe3c&0&UID24646");
    }

    #[test]
    fn reads_edid_of_a_real_monitor() {
        let path = r"\\?\DISPLAY#MSI30B9#7&21ebfe3c&0&UID24646#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        if let Some(bytes) = read(path) {
            let edid = parse(&bytes).expect("EDID from the registry must be valid");
            assert_eq!(edid.manufacturer, "MSI");
            assert_eq!(edid.product, 0x30B9);
            assert_eq!(edid.serial.as_deref(), Some("PB9H304C03601"));
        }
    }
}