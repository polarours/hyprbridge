//! Profile resolution and action lookup, kept separate from device I/O.

use hyprbridge_peripherals::engine::{dispatch, resolve_profile};
use hyprbridge_peripherals::model::{Action, DeviceConfig, Profile};

use crate::events::event_name;
use crate::ipc;

/// Resolves the active profile and the action bound to each event.
pub struct BindingEngine {
    device: DeviceConfig,
    active_profile: String,
}

impl BindingEngine {
    /// Create an engine with the device's default profile selected.
    #[must_use]
    pub fn new(device: DeviceConfig) -> Self {
        let active_profile = device.default_profile.clone();
        Self {
            device,
            active_profile,
        }
    }

    /// Re-evaluate the focused window class and switch profiles if needed.
    pub fn refresh_window(&mut self) {
        let Some(class) = ipc::active_window_class() else {
            return;
        };
        let profile = resolve_profile(&self.device, Some(&class));
        self.active_profile = profile.name.clone();
    }

    /// The currently selected profile.
    #[must_use]
    pub fn profile(&self) -> &Profile {
        self.device
            .profiles
            .iter()
            .find(|profile| profile.name == self.active_profile)
            .unwrap_or_else(|| resolve_profile(&self.device, None))
    }

    /// Look up the action for a raw evdev key code.
    #[must_use]
    pub fn action_for(&self, code: u16) -> Option<&Action> {
        dispatch(self.profile(), &event_name(code))
    }

    /// Switch to a named profile, if the device defines it.
    pub fn select_profile(&mut self, name: &str) -> bool {
        if self
            .device
            .profiles
            .iter()
            .any(|profile| profile.name == name)
        {
            self.active_profile = name.to_string();
            true
        } else {
            false
        }
    }

    /// Configured device name.
    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyprbridge_peripherals::model::{Binding, MatchRule, Profile};

    fn engine() -> BindingEngine {
        let device = DeviceConfig {
            name: "test".into(),
            match_rule: MatchRule {
                name: Some("test".into()),
                ..MatchRule::default()
            },
            backend: "evdev".into(),
            default_profile: "base".into(),
            profiles: vec![
                Profile {
                    name: "base".into(),
                    bindings: vec![Binding {
                        event: "BTN_SIDE".into(),
                        action: Action::Key {
                            keys: vec!["KEY_LEFTMETA".into()],
                        },
                    }],
                    ..Profile::default()
                },
                Profile {
                    name: "browser".into(),
                    window_class: Some("firefox".into()),
                    bindings: Vec::new(),
                    ..Profile::default()
                },
            ],
        };
        BindingEngine::new(device)
    }

    #[test]
    fn finds_binding_by_raw_code() {
        let engine = engine();
        let action = engine.action_for(0x113).expect("BTN_SIDE binding");
        assert_eq!(
            action,
            &Action::Key {
                keys: vec!["KEY_LEFTMETA".into()]
            }
        );
    }

    #[test]
    fn switches_profiles_by_name() {
        let mut engine = engine();
        assert!(engine.select_profile("browser"));
        assert_eq!(engine.profile().name, "browser");
        assert!(!engine.select_profile("missing"));
        assert_eq!(engine.profile().name, "browser");
    }
}
