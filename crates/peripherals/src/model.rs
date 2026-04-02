//! Immutable-ish domain model for local peripheral configuration.
//!
//! These types are shared by discovery, config validation, the profile engine,
//! the CLI and the daemon API. Keep them free of platform-specific logic.

use serde::{Deserialize, Serialize};

/// What a device can actually do (not what the vendor advertises).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Adjustable CPI/DPI steps.
    Dpi,
    /// Polling / report rate in Hz.
    ReportRate,
    /// Physical buttons can be remapped.
    Buttons,
    /// Sequences of keys can be replayed.
    Macros,
    /// Programmable RGB / indicator LEDs.
    Rgb,
    /// Battery level is readable.
    Battery,
    /// Settings can be written to device flash.
    OnboardProfiles,
}

/// An action emitted when a configured input event fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Emit a single key.
    Key {
        /// Keys to emit.
        keys: Vec<String>,
    },
    /// Emit a key sequence.
    Macro {
        /// Keys to emit in order.
        keys: Vec<String>,
    },
    /// Run a command.
    Exec {
        /// Program and arguments.
        argv: Vec<String>,
    },
    /// Switch to a DPI value.
    Dpi {
        /// Target DPI value.
        value: u32,
    },
    /// Switch to another profile.
    Profile {
        /// Target profile name.
        value: String,
    },
    /// Pass the event through unchanged.
    Pass,
    /// Swallow the event.
    Disable,
}

/// A single event -> action mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// evdev event name, e.g. `BTN_SIDE`.
    pub event: String,
    /// Action to perform.
    pub action: Action,
}

/// A named set of settings and bindings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Profile name, unique inside a device.
    pub name: String,
    /// Available DPI steps.
    #[serde(default)]
    pub dpi: Vec<u32>,
    /// Index into `dpi` that is active by default.
    #[serde(default)]
    pub active_dpi: usize,
    /// Polling rate in Hz.
    #[serde(default)]
    pub report_rate: Option<u32>,
    /// Hyprland/Sway window class that selects this profile.
    #[serde(default)]
    pub window_class: Option<String>,
    /// Button/key bindings.
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

/// How a config entry identifies a physical device.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRule {
    /// Case-insensitive substring of the evdev device name.
    #[serde(default)]
    pub name: Option<String>,
    /// USB vendor id, e.g. `046d`.
    #[serde(default)]
    pub vendor: Option<String>,
    /// USB product id, e.g. `4074`.
    #[serde(default)]
    pub product: Option<String>,
    /// Bus id from `/proc/bus/input/devices`.
    #[serde(default)]
    pub bus: Option<String>,
}

impl MatchRule {
    /// True when at least one match field is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.vendor.is_none() && self.product.is_none() && self.bus.is_none()
    }
}

/// Config for one physical device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Human-readable name used in CLI output.
    pub name: String,
    /// Device selection rule (TOML key is `match`).
    #[serde(rename = "match")]
    pub match_rule: MatchRule,
    /// Backend name, e.g. `hidpp20` or `evdev`.
    pub backend: String,
    /// Profile used when no per-app profile matches.
    pub default_profile: String,
    /// Profiles available for this device.
    pub profiles: Vec<Profile>,
}

/// Root peripheral configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeripheralConfig {
    /// Schema version; only 1 is supported.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Configured devices.
    pub devices: Vec<DeviceConfig>,
}

fn default_version() -> u32 {
    1
}

/// A node from `/proc/bus/input/devices`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeripheralDevice {
    /// evdev device name.
    pub name: String,
    /// Bus id.
    pub bus: String,
    /// USB vendor id.
    pub vendor: String,
    /// USB product id.
    pub product: String,
    /// Physical path.
    pub phys: String,
    /// Sysfs path.
    pub sysfs: String,
    /// Kernel handler names.
    pub handlers: Vec<String>,
}

impl PeripheralDevice {
    /// Event node names (without `/dev/input/`).
    #[must_use]
    pub fn event_nodes(&self) -> Vec<String> {
        self.handlers
            .iter()
            .filter(|handler| handler.starts_with("event"))
            .cloned()
            .collect()
    }

    /// Case-insensitive match against a rule; all set fields must match.
    #[must_use]
    pub fn matches(&self, rule: &MatchRule) -> bool {
        if let Some(name) = &rule.name {
            if !self.name.to_lowercase().contains(&name.to_lowercase()) {
                return false;
            }
        }
        if let Some(vendor) = &rule.vendor {
            if !self.vendor.eq_ignore_ascii_case(vendor) {
                return false;
            }
        }
        if let Some(product) = &rule.product {
            if !self.product.eq_ignore_ascii_case(product) {
                return false;
            }
        }
        if let Some(bus) = &rule.bus {
            if !self.bus.eq_ignore_ascii_case(bus) {
                return false;
            }
        }
        true
    }
}
