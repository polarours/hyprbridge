# Platform Support

## Linux

- Runtime session detection distinguishes X11 and Wayland.
- X11 and Wayland are implemented behind separate crates and capability reports.
- X11 currently has the most complete runtime path: edge detection, pointer entry, relative pointer relay, and target-side button/key/wheel injection.
- Hyprland currently has the strongest Wayland path: pointer inspection, edge detection, cached `evdev`-backed controller key/button/wheel sampling, and target injection that prefers a virtual `uinput` device with compositor-specific fallback.
- Full Hyprland controller relay requires readable `/dev/input/event*` devices.
- Active remote ownership on Hyprland now attempts `evdev` grabs to suppress local controller side effects where permissions allow.
- Writable `/dev/uinput` upgrades Hyprland target-side pointer, button, key, and wheel injection beyond the dispatcher fallback path.
- Hyprland runtime diagnostics now expose structured `controller_capture` and `target_injection` state to the CLI, daemon API, desktop shell, and web UI.
- Linux controller capture explicitly filters out HyprBridge's own virtual `uinput` device to prevent self-feedback.
- X11 controller capture is still an MVP path and does not yet fully suppress every local side effect on the source machine.
- Wayland support is capability-driven and explicit about compositor restrictions.

## macOS

- Accessibility and input-monitoring permissions are first-class diagnostics.
- Backend integration points are modeled but not yet feature-complete.

## Windows

- Desktop session requirements and native injection constraints are surfaced through the capability model.
- Backend integration points are modeled but not yet feature-complete.
