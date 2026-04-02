# Linux: X11 And Wayland

## Architecture Choice

HyprBridge treats Linux sessions as two separate backend families:

- `platform-x11`: traditional global input/control model
- `platform-wayland`: compositor-aware, capability-constrained model

## Runtime Detection

The Linux platform entrypoint inspects:

- `XDG_SESSION_TYPE`
- `WAYLAND_DISPLAY`
- `DISPLAY`
- desktop/compositor session hints

## Support Matrix

- X11:
  - global input capture is a practical MVP target
  - injection is a practical MVP target
  - current repository includes edge-triggered ownership transfer
  - active remote sessions relay pointer motion plus pointer-button, wheel, and key state changes
  - target-side execution uses native X11 pointer placement and XTEST-backed button/key/wheel injection
- Wayland:
  - support depends on compositor and protocol capabilities
  - current repository includes a Hyprland-first backend using Hyprland IPC for pointer inspection and entry placement
  - Hyprland controller-side keyboard, mouse-button, and wheel state sampling currently uses cached Linux `evdev` devices, with periodic refresh and self-healing rediscovery
  - target-side pointer, wheel, key, and button injection now prefer a virtual `uinput` device when `/dev/uinput` is writable
  - Hyprland IPC and dispatcher remain the fallback path when `uinput` is unavailable
  - full controller-side relay therefore depends on readable `/dev/input/event*` devices
  - unrestricted compositor-agnostic keyboard capture is still not assumed

## Fallback Behavior

When capabilities are missing, HyprBridge:

- reports the exact unsupported area
- explains the likely reason
- keeps the daemon operational for discovery, pairing, and diagnostics

On X11, the current controller relay loop is intentionally conservative:

- pointer motion is collected by polling the local pointer around a recentered anchor
- button and key transitions are detected from X11 state snapshots
- local-event suppression is not fully complete yet, so some environments may still observe local side effects while relaying

On Hyprland Wayland, the current backend is intentionally explicit:

- pointer position and desktop bounds come from Hyprland IPC queries
- pointer entry placement uses the `movecursor` dispatcher path
- controller-side keyboard, mouse-button, and wheel state sampling uses cached `evdev` snapshots from readable `/dev/input/event*` devices
- controller-side cache refreshes periodically, refreshes eagerly after device read failures, and preserves intended grab state across rediscovery
- target-side relative pointer, wheel, keyboard, and button injection prefer a virtual `uinput` device when `/dev/uinput` is writable
- during active remote ownership, the controller attempts `evdev` grabs to reduce local side effects on the source machine
- Hyprland dispatcher fallback remains in place for key/button injection and pointer movement when `uinput` is unavailable
- HyprBridge's own virtual `uinput` device is excluded from controller capture to avoid feedback loops
- the daemon and control surfaces expose structured `controller_capture` and `target_injection` runtime snapshots for diagnostics
- this makes Hyprland materially more usable than generic Wayland, but still not equivalent to the X11 path yet

## Future Work

- compositor-specific integrations
- stronger portal workflows
- per-compositor capability refinements
