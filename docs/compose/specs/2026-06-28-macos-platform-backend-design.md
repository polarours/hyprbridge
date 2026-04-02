# macOS Platform Backend Design

## [S1] Problem

The `platform-mac` crate is a pure stub — it returns `CapabilityReport::planned("macos")` with no actual input capture or injection. macOS is a first-class target but has zero native backend coverage. The daemon's `cfg!(target_os = "macos")` branches fallback to no-ops.

## [S2] Solution Overview

Implement full macOS input capture and injection using CGEvent APIs:
- Capability probe with Accessibility permission check
- Controller-side input capture via CGEventTap
- Target-side input injection via CGEventPost
- Daemon integration with `cfg!(target_os = "macos")` branches

## [S3] Capability Probe

Replace the stub `MacProbe::probe()` with a real implementation:
- Call `AXIsProcessTrusted()` to check Accessibility permission
- Return `SupportLevel::Supported` if granted, `SupportLevel::Unsupported` if not
- Report `PermissionState::Granted` or `PermissionState::Missing` based on check
- Set `input_capture`, `input_injection`, `cursor_handoff` to `Supported` if permission granted

**Files:** `crates/platform-mac/src/lib.rs`

## [S4] Input Capture (Controller Side)

Implement controller-side input capture using CGEventTap:

- **Event tap creation**: `CGEventTapCreate` with `kCGHIDEventTap` location, `kCGHeadInsertEventTap` placement
- **Event types**: `kCGEventMouseMoved`, `kCGEventLeftMouseDown/Up`, `kCGEventRightMouseDown/Up`, `kCGEventOtherMouseDown/Up`, `kCGEventScrollWheel`, `kCGEventKeyDown`, `kCGEventKeyUp`
- **Tap activation**: `CGEventTapEnable` + `CFMachPort` run loop source
- **Threading**: Dedicated thread with its own `CFRunLoop`
- **Data flow**: Events diffed (pointer position, buttons, keys) → `ControlFrame` deltas → QUIC

Key functions:
- `macos_controller_sample() -> Option<ControllerSample>` — current pointer/key state
- `macos_set_remote_capture_active(active: bool) -> Option<CaptureGrabReport>` — enable/disable event tap
- `macos_recenter_controller_pointer() -> bool` — warp pointer to screen center

**Feedback loop avoidance**: Tag synthetic events with a custom CGEvent field (e.g., `kCGEventSourceUserData`) and skip them in the capture tap.

**Files:** `crates/platform-mac/src/lib.rs`

## [S5] Input Injection (Target Side)

Implement target-side input injection using CGEventPost:

- **Pointer movement**: `CGEvent` with `kCGEventMouseMoved`, set location via `CGEventSetLocation`
- **Pointer buttons**: `CGEvent` with `kCGEventLeftMouseDown/Up` etc., set button number
- **Keyboard**: `CGEvent` with `kCGEventKeyDown/Up`, set keycode via `CGEventKeyboardSetKeyCode`
- **Scroll**: `CGEvent` with `kCGEventScrollWheel`, set delta via `CGEventSetIntegerValueField(kCGScrollWheelEventDeltaAxis1)`

Key functions:
- `macos_inject_pointer_relative(delta_x: i16, delta_y: i16) -> bool`
- `macos_inject_pointer_button(button: MouseButton, pressed: bool) -> bool`
- `macos_inject_pointer_scroll(horizontal: i16, vertical: i16) -> bool`
- `macos_inject_keyboard_key(keycode: u8, pressed: bool) -> bool`
- `macos_place_pointer_on_entry(edge: ScreenEdge) -> bool`
- `macos_focus_local_target() -> bool`

**Thread safety**: `CGEventPost` must be called from the main thread or a thread with a running `CFRunLoop`. Use `dispatch::main` or a dedicated run loop thread.

**Files:** `crates/platform-mac/src/lib.rs`

## [S6] Daemon Integration

Wire the macOS backend into the daemon:

1. Update `platform-mac/Cargo.toml` — Add dependencies:
   - `core-foundation` (CFRunLoop, CFMachPort)
   - `core-graphics` (CGEvent, CGEventTap, CGEventPost)
   - `core-foundation-sys` (low-level types)
   - `dispatch` (for `dispatch::main`)

2. Update `crates/daemon/src/lib.rs` — Add `cfg!(target_os = "macos")` branches in `DaemonControlHandler` that call the new macOS functions, mirroring the existing `cfg!(target_os = "linux")` pattern.

3. The existing `probe_capability()` in daemon already routes by `cfg!(target_os)` — the macOS branch calls `hyprbridge_platform_mac::MacProbe.probe()`.

**Files:** `crates/platform-mac/Cargo.toml`, `crates/daemon/src/lib.rs`

## [S7] Testing Strategy

- Unit tests: Probe permission check can be tested with a mock
- Manual test on macOS: `cargo run -p hyprbridge-cli -- status` should show real permissions
- Manual test: Pointer injection via CLI or daemon API
- Cross-compilation: `cargo check --target aarch64-apple-darwin` to verify compilation

## [S8] Non-Goals

- macOS-specific edge detection (pointer near screen edge) — can be added later
- macOS-specific cursor warping behavior — defer to future iteration
- macOS Wayland equivalent (doesn't exist on macOS)
