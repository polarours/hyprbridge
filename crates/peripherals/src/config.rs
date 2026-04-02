//! TOML configuration loading and strict validation.
//!
//! A bad config is refused outright. Silently ignoring a binding is worse
//! than a driver that refuses to start.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::PeripheralError;
use crate::model::{Action, DeviceConfig, PeripheralConfig, Profile};

/// Default config location: `$XDG_CONFIG_HOME/hyprbridge/peripherals.toml`.
#[must_use]
pub fn default_config_path() -> PathBuf {
    let base = env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_else(|_| "/".to_string());
            Path::new(&home).join(".config")
        });
    base.join("hyprbridge").join("peripherals.toml")
}

/// Load and validate a config file.
pub fn load(path: &Path) -> Result<PeripheralConfig, PeripheralError> {
    let text = fs::read_to_string(path)?;
    let config: PeripheralConfig = toml::from_str(&text)?;
    validate(&config)?;
    Ok(config)
}

/// Validate a parsed config without touching the filesystem.
pub fn validate(config: &PeripheralConfig) -> Result<(), PeripheralError> {
    if config.version != 1 {
        return Err(PeripheralError::Config(format!(
            "unsupported version {}; expected 1",
            config.version
        )));
    }
    if config.devices.is_empty() {
        return Err(PeripheralError::Config("no [[devices]] entries".into()));
    }
    for (index, device) in config.devices.iter().enumerate() {
        validate_device(device)
            .map_err(|error| PeripheralError::Config(format!("devices[{index}]: {error}")))?;
    }
    Ok(())
}

fn validate_device(device: &DeviceConfig) -> Result<(), PeripheralError> {
    if device.name.trim().is_empty() {
        return Err(PeripheralError::Config("name must not be empty".into()));
    }
    if device.match_rule.is_empty() {
        return Err(PeripheralError::Config(
            "match must set at least one field".into(),
        ));
    }
    if device.backend.trim().is_empty() {
        return Err(PeripheralError::Config("backend must not be empty".into()));
    }
    if device.profiles.is_empty() {
        return Err(PeripheralError::Config("profiles must not be empty".into()));
    }
    let names: Vec<&str> = device.profiles.iter().map(|p| p.name.as_str()).collect();
    if names.iter().collect::<std::collections::HashSet<_>>().len() != names.len() {
        return Err(PeripheralError::Config("duplicate profile names".into()));
    }
    if !names.contains(&device.default_profile.as_str()) {
        return Err(PeripheralError::Config(format!(
            "default_profile {:?} is not in {names:?}",
            device.default_profile
        )));
    }
    for profile in &device.profiles {
        validate_profile(profile)?;
    }
    Ok(())
}

fn validate_profile(profile: &Profile) -> Result<(), PeripheralError> {
    if profile.name.trim().is_empty() {
        return Err(PeripheralError::Config(
            "profile name must not be empty".into(),
        ));
    }
    if profile.dpi.iter().any(|value| *value == 0) {
        return Err(PeripheralError::Config(
            "dpi values must be positive".into(),
        ));
    }
    if !profile.dpi.is_empty() && profile.active_dpi >= profile.dpi.len() {
        return Err(PeripheralError::Config(format!(
            "active_dpi {} out of range ({} steps)",
            profile.active_dpi,
            profile.dpi.len()
        )));
    }
    for binding in &profile.bindings {
        if binding.event.trim().is_empty() {
            return Err(PeripheralError::Config(
                "binding event must not be empty".into(),
            ));
        }
        match &binding.action {
            Action::Key { keys } | Action::Macro { keys } if keys.is_empty() => {
                return Err(PeripheralError::Config(
                    "key/macro action needs keys".into(),
                ));
            }
            Action::Exec { argv } if argv.is_empty() => {
                return Err(PeripheralError::Config("exec action needs argv".into()));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MatchRule;

    const VALID: &str = r#"
version = 1

[[devices]]
name = "Logitech G304"
match = { name = "Logitech G304" }
backend = "hidpp20"
default_profile = "desktop"

[[devices.profiles]]
name = "desktop"
dpi = [800, 1600]
active_dpi = 1
report_rate = 1000

[[devices.profiles.bindings]]
event = "BTN_SIDE"
action = { type = "key", keys = ["KEY_LEFTMETA"] }

[[devices.profiles]]
name = "browser"
window_class = "firefox"

[[devices.profiles.bindings]]
event = "BTN_EXTRA"
action = { type = "exec", argv = ["notify-send", "hi"] }
"#;

    #[test]
    fn parses_valid_config() {
        let config: PeripheralConfig = toml::from_str(VALID).expect("valid toml");
        validate(&config).expect("valid config");
        assert_eq!(config.devices.len(), 1);
        let device = &config.devices[0];
        assert_eq!(device.backend, "hidpp20");
        assert_eq!(device.profiles[0].dpi, vec![800, 1600]);
        assert_eq!(
            device.profiles[0].bindings[0].action,
            Action::Key {
                keys: vec!["KEY_LEFTMETA".into()]
            }
        );
    }

    #[test]
    fn rejects_bad_active_dpi() {
        let broken = VALID.replace("active_dpi = 1", "active_dpi = 9");
        let config: PeripheralConfig = toml::from_str(&broken).expect("valid toml");
        assert!(validate(&config).is_err());
    }

    #[test]
    fn rejects_duplicate_profile_names() {
        let broken = VALID.replace("name = \"browser\"", "name = \"desktop\"");
        let config: PeripheralConfig = toml::from_str(&broken).expect("valid toml");
        assert!(validate(&config).is_err());
    }

    #[test]
    fn rejects_empty_match_rule() {
        let config = PeripheralConfig {
            devices: vec![DeviceConfig {
                name: "x".into(),
                match_rule: MatchRule::default(),
                backend: "evdev".into(),
                default_profile: "p".into(),
                profiles: vec![Profile {
                    name: "p".into(),
                    ..Profile::default()
                }],
            }],
            ..PeripheralConfig::default()
        };
        assert!(validate(&config).is_err());
    }
}
