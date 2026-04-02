//! Local peripheral configuration subsystem for HyprBridge.
//!
//! This crate handles configuring the devices themselves: DPI, report rate,
//! button mappings, macros and per-application profiles. Cross-machine input
//! sharing remains the daemon's job; the two subsystems are complementary.
//!
//! Current scope: discovery, config validation, capability reporting,
//! profile resolution and diagnostics. HID++ writes and the evdev/uinput
//! remap loop are intentionally not implemented yet and are reported as such.

pub mod config;
pub mod discovery;
pub mod doctor;
pub mod engine;
pub mod hidpp;
pub mod model;

use serde::Serialize;
use thiserror::Error;

use crate::doctor::PeripheralCheck;
use crate::model::PeripheralDevice;

/// Errors from the peripheral subsystem.
#[derive(Debug, Error)]
pub enum PeripheralError {
    /// Filesystem error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// TOML parse error.
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
    /// Invalid configuration.
    #[error("invalid peripheral config: {0}")]
    Config(String),
    /// Error code returned by the HID++ device.
    #[error("HID++ device error 0x{0:02X}")]
    HidppError(u8),
    /// No matching HID++ reply arrived in time.
    #[error("HID++ request timed out")]
    HidppTimeout,
    /// The device does not implement the requested feature.
    #[error("HID++ feature 0x{0:04X} is not supported by this device")]
    FeatureUnsupported(u16),
    /// Malformed reply payload.
    #[error("unexpected HID++ reply: {0}")]
    BadReply(String),
}

/// A point-in-time snapshot for the CLI and daemon API.
#[derive(Debug, Clone, Serialize)]
pub struct PeripheralSnapshot {
    /// Config path used by this process.
    pub config_path: String,
    /// Whether that file exists.
    pub config_present: bool,
    /// Visible input devices.
    pub devices: Vec<PeripheralDevice>,
    /// Detected Logitech Lightspeed receivers.
    pub receivers: Vec<(String, String)>,
    /// Diagnostic rows.
    pub checks: Vec<PeripheralCheck>,
}

/// Build a snapshot of local peripheral state.
#[must_use]
pub fn snapshot() -> PeripheralSnapshot {
    let path = config::default_config_path();
    let config = config::load(&path).ok();
    PeripheralSnapshot {
        config_path: path.display().to_string(),
        config_present: path.exists(),
        devices: discovery::list_devices().unwrap_or_default(),
        receivers: discovery::logitech_receivers(),
        checks: doctor::checks(config.as_ref()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn snapshot_builds_without_config() {
        let snapshot = super::snapshot();
        assert!(!snapshot.config_path.is_empty());
        assert!(!snapshot.checks.is_empty());
    }
}
