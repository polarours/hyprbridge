//! Pure profile resolution and action dispatch (no I/O).

use crate::model::{Action, DeviceConfig, Profile};

/// Pick the profile for the focused window, falling back to the default.
#[must_use]
pub fn resolve_profile<'a>(device: &'a DeviceConfig, window_class: Option<&str>) -> &'a Profile {
    if let Some(class) = window_class {
        let needle = class.to_lowercase();
        for profile in &device.profiles {
            if let Some(candidate) = &profile.window_class {
                if candidate.to_lowercase() == needle {
                    return profile;
                }
            }
        }
    }
    device
        .profiles
        .iter()
        .find(|profile| profile.name == device.default_profile)
        .unwrap_or(&device.profiles[0])
}

/// Find the action bound to an event name.
#[must_use]
pub fn dispatch<'a>(profile: &'a Profile, event: &str) -> Option<&'a Action> {
    profile
        .bindings
        .iter()
        .find(|binding| binding.event == event)
        .map(|binding| &binding.action)
}

/// Human-readable action description for CLI output.
#[must_use]
pub fn describe(action: &Action) -> String {
    match action {
        Action::Key { keys } | Action::Macro { keys } => keys.join("+"),
        Action::Exec { argv } => argv.join(" "),
        Action::Dpi { value } => value.to_string(),
        Action::Profile { value } => value.clone(),
        Action::Pass => "pass".into(),
        Action::Disable => "disable".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Binding, MatchRule};

    fn device() -> DeviceConfig {
        DeviceConfig {
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
                    name: "firefox".into(),
                    window_class: Some("Firefox".into()),
                    ..Profile::default()
                },
            ],
        }
    }

    #[test]
    fn resolves_window_profile_case_insensitively() {
        assert_eq!(resolve_profile(&device(), Some("firefox")).name, "firefox");
    }

    #[test]
    fn falls_back_to_default() {
        assert_eq!(resolve_profile(&device(), Some("chromium")).name, "base");
        assert_eq!(resolve_profile(&device(), None).name, "base");
    }

    #[test]
    fn dispatches_actions() {
        let config = device();
        let profile = resolve_profile(&config, None);
        let action = dispatch(profile, "BTN_SIDE").expect("binding exists");
        assert_eq!(describe(action), "KEY_LEFTMETA");
        assert!(dispatch(profile, "BTN_EXTRA").is_none());
    }
}
