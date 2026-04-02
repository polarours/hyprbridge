//! Dependency-free device discovery.
//!
//! `/proc/bus/input/devices` provides evdev nodes; `/sys/bus/usb/devices`
//! provides USB ids. Neither needs root or a third-party crate, so discovery
//! works before evdev/uinput permissions are sorted out.

use std::fs;
use std::path::{Path, PathBuf};

use crate::PeripheralError;
use crate::hidpp;
use crate::model::{MatchRule, PeripheralDevice};

/// Parse the full text of `/proc/bus/input/devices`.
#[must_use]
pub fn parse_input_devices(text: &str) -> Vec<PeripheralDevice> {
    let mut devices = Vec::new();
    let mut current: Option<PeripheralDevice> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            if let Some(device) = current.take() {
                if !device.name.is_empty() {
                    devices.push(device);
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("I:") {
            if let Some(device) = current.take() {
                if !device.name.is_empty() {
                    devices.push(device);
                }
            }
            let mut device = PeripheralDevice::default();
            for part in rest.split_whitespace() {
                if let Some((key, value)) = part.split_once('=') {
                    match key {
                        "Bus" => device.bus = value.to_string(),
                        "Vendor" => device.vendor = value.to_string(),
                        "Product" => device.product = value.to_string(),
                        _ => {}
                    }
                }
            }
            current = Some(device);
            continue;
        }
        let Some(device) = current.as_mut() else {
            continue;
        };
        if let Some(rest) = line.strip_prefix("N:") {
            if let Some(name) = rest.trim_start().strip_prefix("Name=") {
                device.name = name.trim_matches('"').to_string();
            }
        } else if let Some(rest) = line.strip_prefix("P:") {
            if let Some(phys) = rest.trim_start().strip_prefix("Phys=") {
                device.phys = phys.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("S:") {
            if let Some(sysfs) = rest.trim_start().strip_prefix("Sysfs=") {
                device.sysfs = sysfs.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("H:") {
            if let Some(handlers) = rest.trim_start().strip_prefix("Handlers=") {
                device.handlers = handlers.split_whitespace().map(str::to_string).collect();
            }
        }
    }
    if let Some(device) = current.take() {
        if !device.name.is_empty() {
            devices.push(device);
        }
    }
    devices
}

/// Read and parse the live device table.
pub fn list_devices() -> Result<Vec<PeripheralDevice>, PeripheralError> {
    let text = fs::read_to_string("/proc/bus/input/devices")?;
    Ok(parse_input_devices(&text))
}

/// Filter live devices by a config match rule.
pub fn find_devices(rule: &MatchRule, devices: &[PeripheralDevice]) -> Vec<PeripheralDevice> {
    devices
        .iter()
        .filter(|device| device.matches(rule))
        .cloned()
        .collect()
}

/// USB `(vendor, product)` pairs currently attached.
pub fn usb_ids(root: &Path) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(vendor) = fs::read_to_string(path.join("idVendor")) else {
            continue;
        };
        let Ok(product) = fs::read_to_string(path.join("idProduct")) else {
            continue;
        };
        result.push((vendor.trim().to_lowercase(), product.trim().to_lowercase()));
    }
    result
}

/// hidraw nodes belonging to Logitech Lightspeed receivers.
///
/// Solaar addresses the paired mouse at device index 1 through the receiver's
/// hidraw node (the first interface), not through the mouse's own hidraw node.
#[must_use]
pub fn receiver_hidraw_nodes() -> Vec<PathBuf> {
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hidraw") else {
        return result;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(target) = fs::canonicalize(entry.path()) else {
            continue;
        };
        if target
            .to_string_lossy()
            .to_uppercase()
            .contains("046D:C53F")
        {
            result.push(Path::new("/dev").join(name));
        }
    }
    result.sort();
    result
}

/// Logitech Lightspeed receivers currently attached.
#[must_use]
pub fn logitech_receivers() -> Vec<(String, String)> {
    usb_ids(Path::new("/sys/bus/usb/devices"))
        .into_iter()
        .filter(|(vendor, product)| vendor == "046d" && hidpp::is_lightspeed_receiver(product))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "I: Bus=0003 Vendor=046d Product=c53f Version=0111\n\
N: Name=\"Logitech G304\"\n\
P: Phys=usb-0000:00:14.0-6/input0\n\
S: Sysfs=/devices/pci0000:00/usb1/1-6/input/input4\n\
H: Handlers=mouse0 event5\n\
B: EV=17\n\
\n\
I: Bus=0001 Vendor=0000 Product=0001 Version=0000\n\
N: Name=\"Power Button\"\n\
H: Handlers=kbd event2\n\
B: EV=3\n";

    #[test]
    fn parses_devices_and_event_nodes() {
        let devices = parse_input_devices(FIXTURE);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "Logitech G304");
        assert_eq!(devices[0].vendor, "046d");
        assert_eq!(devices[0].event_nodes(), vec!["event5".to_string()]);
    }

    #[test]
    fn matches_rule_case_insensitively() {
        let devices = parse_input_devices(FIXTURE);
        let rule = MatchRule {
            vendor: Some("046D".into()),
            product: Some("c53f".into()),
            ..MatchRule::default()
        };
        assert_eq!(find_devices(&rule, &devices).len(), 1);
        let name_rule = MatchRule {
            name: Some("g304".into()),
            ..MatchRule::default()
        };
        assert_eq!(find_devices(&name_rule, &devices).len(), 1);
    }
}
