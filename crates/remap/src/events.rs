//! Event-name helpers shared by binding resolution and raw event display.

/// Common mouse buttons and modifier keys used in peripheral configs.
const NAMED_CODES: &[(&str, u16)] = &[
    ("BTN_LEFT", 0x110),
    ("BTN_RIGHT", 0x111),
    ("BTN_MIDDLE", 0x112),
    ("BTN_SIDE", 0x113),
    ("BTN_EXTRA", 0x114),
    ("BTN_FORWARD", 0x115),
    ("BTN_BACK", 0x116),
    ("BTN_TASK", 0x117),
    ("KEY_ESC", 1),
    ("KEY_1", 2),
    ("KEY_A", 30),
    ("KEY_S", 31),
    ("KEY_D", 32),
    ("KEY_F", 33),
    ("KEY_LEFTCTRL", 29),
    ("KEY_LEFTSHIFT", 42),
    ("KEY_LEFTALT", 56),
    ("KEY_LEFTMETA", 125),
    ("KEY_RIGHTCTRL", 97),
    ("KEY_RIGHTSHIFT", 54),
    ("KEY_RIGHTALT", 100),
    ("KEY_RIGHTMETA", 126),
    ("KEY_SPACE", 57),
    ("KEY_ENTER", 28),
    ("KEY_TAB", 15),
    ("KEY_CAPSLOCK", 58),
    ("KEY_F13", 183),
    ("KEY_F14", 184),
    ("KEY_F15", 185),
    ("KEY_F16", 186),
];

/// Linux letter key codes follow QWERTY rows, not alphabetical order.
const LETTER_RUNS: &[(u16, &str)] = &[(16, "QWERTYUIOP"), (30, "ASDFGHJKL"), (44, "ZXCVBNM")];

/// Resolve a binding event name to an evdev key code.
///
/// Numbers in decimal (`"113"`) or hex (`"0x71"`) are accepted too, which
/// makes newly discovered buttons usable before they have a symbolic name.
#[must_use]
pub fn parse_event_code(name: &str) -> Option<u16> {
    let upper = name.trim().to_uppercase();
    if let Some((_, code)) = NAMED_CODES.iter().find(|(key, _)| *key == upper) {
        return Some(*code);
    }
    if let Some(letter) = upper.strip_prefix("KEY_") {
        let bytes = letter.as_bytes();
        if bytes.len() == 1 && bytes[0].is_ascii_uppercase() {
            for (start, run) in LETTER_RUNS {
                if let Some(index) = run.find(bytes[0] as char) {
                    return Some(start + index as u16);
                }
            }
        }
    }
    if let Some(hex) = upper.strip_prefix("0X") {
        return u16::from_str_radix(hex, 16).ok();
    }
    upper.parse::<u16>().ok()
}

/// Human-readable name for an evdev key code.
#[must_use]
pub fn event_name(code: u16) -> String {
    NAMED_CODES
        .iter()
        .find(|(_, value)| *value == code)
        .map(|(name, _)| (*name).to_string())
        .unwrap_or_else(|| {
            for (start, run) in LETTER_RUNS {
                if let Some(index) = code.checked_sub(*start) {
                    if let Some(letter) = run.chars().nth(index as usize) {
                        return format!("KEY_{letter}");
                    }
                }
            }
            if (2..=11).contains(&code) {
                format!("KEY_{}", code - 1)
            } else {
                format!("KEY_{code}")
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_named_and_numeric_codes() {
        assert_eq!(parse_event_code("BTN_SIDE"), Some(0x113));
        assert_eq!(parse_event_code("btn_extra"), Some(0x114));
        assert_eq!(parse_event_code("0x113"), Some(0x113));
        assert_eq!(parse_event_code("275"), Some(275));
        assert_eq!(parse_event_code("KEY_V"), Some(47));
        assert_eq!(parse_event_code("KEY_C"), Some(46));
        assert_eq!(parse_event_code("KEY_Q"), Some(16));
        assert_eq!(parse_event_code("NOPE"), None);
    }

    #[test]
    fn round_trips_named_codes() {
        assert_eq!(event_name(47), "KEY_V");
        assert_eq!(event_name(46), "KEY_C");
        assert_eq!(event_name(0x113), "BTN_SIDE");
        assert_eq!(parse_event_code(&event_name(0x114)), Some(0x114));
    }
}
