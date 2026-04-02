# Button Remapping (evdev -> uinput)

`hyprbridge-remap` gives the same class of functionality people usually install
G HUB / Options+ for: a side button becomes a key or shortcut, a macro fires on
click, and a profile can change with the focused application.

## Why userspace

The G304/G305 has no `REPROG CONTROLS (0x1B04)` feature, so its buttons cannot
be reprogrammed in firmware. HyprBridge grabs the evdev node, rewrites the
events and re-injects them through a virtual uinput device. This works on
Wayland/Hyprland where `xinput` and `xbindkeys` do nothing.

## Commands

```bash
# 1. find the exact button code (read-only, no grab)
cargo run -p hyprbridge-cli -- remap events --device "Logitech G304"
# press the side button and note the code, e.g. BTN_SIDE / 275

# 2. validate config and device selection without grabbing
cargo run -p hyprbridge-cli -- remap start \
  --config ~/Projects/Personal/hyprbridge/config/peripherals.example.toml \
  --device "Logitech G304" --dry-run

# 3. start remapping (Ctrl-C stops; the grab is released by the kernel)
cargo run -p hyprbridge-cli -- remap start \
  --config ~/.config/hyprbridge/peripherals.toml \
  --device "Logitech G304"
```

## Supported actions

| Action | Behaviour |
| --- | --- |
| `key` | Hold the configured keys while the button is held; release after |
| `macro` | Tap a key sequence on press |
| `exec` | Spawn a command on press |
| `dpi` | Write DPI over HID++ (real G304 DPI write path) |
| `profile` | Switch the active profile by name |
| `pass` / `disable` | Pass the event through / swallow it |

Unbound events (pointer motion, wheel, ordinary clicks) are always passed
through, so the mouse keeps behaving normally apart from the configured
buttons.

## Onboard-mapped side buttons

A G304 can have its side buttons programmed in G HUB to emit shortcuts such as
`Ctrl+C` / `Ctrl+V`. Those arrive on the mouse evdev node as `KEY_LEFTCTRL` +
`KEY_C` / `KEY_V`, not as `BTN_SIDE` / `BTN_EXTRA`. Bind the letter that
identifies the button and swallow the modifier so the compositor does not see a
stray Ctrl:

```toml
[[devices.profiles.bindings]]
event = "KEY_LEFTCTRL"
action = { type = "disable" }

[[devices.profiles.bindings]]
event = "KEY_V"
action = { type = "key", keys = ["KEY_LEFTMETA"] }
```

This is why `remap events` is the first step: it tells you what the hardware
actually emits. Linux letter key codes follow the QWERTY rows (`KEY_V` = 47,
`KEY_C` = 46), so use the printed name rather than guessing a numeric code.

## Per-application profiles

On every button press the engine queries Hyprland's command socket for the
focused window class (`j/activewindow`) and selects the profile whose
`window_class` matches (case-insensitive). If none matches, `default_profile`
is used. Config example:

```toml
[[devices.profiles]]
name = "browser"
window_class = "firefox"

[[devices.profiles.bindings]]
event = "BTN_SIDE"
action = { type = "profile", value = "desktop" }
```

## Exclusivity and safety

Only one process can grab an evdev node. Stop `makima`, `input-remapper`,
`keyd` and any other remapper for the same device first. If the grab fails the
CLI reports the exact error rather than silently running without remapping.

The virtual device is named `HyprBridge Remap Virtual Device` and is never
grabbed, avoiding self-feedback. `/dev/uinput` must be writable (the shipped
udev rule covers it; the existing `input` group also works on this machine).

Verified on this workspace: dry-run resolves the G304 config and prints its
active profile/binding count; the raw event viewer discovers the device and
prints button codes. The grab-and-inject loop has not been exercised with a
physical button press yet — that requires the user to press a button while the
remapper is running.
