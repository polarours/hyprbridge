# Windows Platform Backend Design

## [S1] Problem

The `platform-win` crate is a pure stub — it returns `CapabilityReport::planned("windows")` with no actual input capture or injection. Windows is a first-class target but has zero native backend coverage.

## [S2] Solution Overview

Implement full Windows input capture and injection using Win32 APIs:
- Capability probe with desktop session detection
- Controller-side input capture via low-level hooks (WH_KEYBOARD_LL, WH_MOUSE_LL)
- Target-side input injection via SendInput + SetCursorPos
- Daemon integration with `cfg!(target_os = "windows")` branches

## [S3] Capability Probe

Replace the stub `WindowsProbe::probe()` with a real implementation:
- Detect Windows version via `GetVersionExW`
- Check desktop session type (console vs RDP vs service)
- Return `SupportLevel::Supported` for `input_capture` and `input_injection`
- Report `PermissionState::Granted` (no special permission grants needed on Windows)

**Files:** `crates/platform-win/src/lib.rs`

## [S4] Input Capture (Controller Side)

Implement controller-side input capture using low-level hooks:

- **Keyboard hook**: `SetWindowsHookExW(WH_KEYBOARD_LL, hook_proc, hInstance, 0)` — captures all keyboard events globally
- **Mouse hook**: `SetWindowsHookExW(WH_MOUSE_LL, hook_proc, hInstance, 0)` — captures all mouse events globally
- **Hook callback**: Receives `KBDLLHOOKSTRUCT` / `MSLLHOOKSTRUCT` with event details
- **Message loop**: Dedicated thread running `GetMessageW` loop to process hook callbacks
- **State tracking**: Maintain current pointer position, button mask, key state
- **Delta computation**: On each event, compute deltas and emit `ControlFrame` frames

Key functions:
- `win_controller_sample() -> Option<ControllerSample>` — current pointer/key state
- `win_set_remote_capture_active(active: bool) -> Option<CaptureGrabReport>` — enable/disable hooks
- `win_recenter_controller_pointer() -> bool` — warp pointer to screen center

**Feedback loop avoidance**: Check `LLMHF_INJECTED` flag on incoming hook events — skip events that were posted by `SendInput` to avoid feedback loops.

**Files:** `crates/platform-win/src/win_backend.rs`

## [S5] Input Injection (Target Side)

Implement target-side input injection using SendInput + SetCursorPos:

- **Pointer movement**: `SetCursorPos(x, y)` for absolute positioning
- **Pointer buttons**: `SendInput` with `MOUSEEVENTF_LEFTDOWN/UP`, `MOUSEEVENTF_RIGHTDOWN/UP`, `MOUSEEVENTF_MIDDLEDOWN/UP`
- **Keyboard**: `SendInput` with `KEYEVENTF_KEYDOWN/UP` and virtual key code
- **Scroll**: `SendInput` with `MOUSEEVENTF_WHEEL` and `WHEEL_DELTA` units

Key functions:
- `win_inject_pointer_relative(delta_x: i16, delta_y: i16) -> bool`
- `win_inject_pointer_button(button: MouseButton, pressed: bool) -> bool`
- `win_inject_pointer_scroll(horizontal: i16, vertical: i16) -> bool`
- `win_inject_keyboard_key(keycode: u8, pressed: bool) -> bool`
- `win_place_pointer_on_entry(edge: ScreenEdge) -> bool`
- `win_focus_local_target() -> bool`

**Files:** `crates/platform-win/src/win_backend.rs`

## [S6] Daemon Integration

Wire the Windows backend into the daemon:

1. Update `platform-win/Cargo.toml` — Add `windows` crate with features: `Win32_UI_Input`, `Win32_UI_WindowsAndMessaging`, `Win32_Foundation`
2. Update `crates/daemon/src/lib.rs` — Add `cfg!(target_os = "windows")` branches in `DaemonControlHandler`
3. Update `crates/daemon/Cargo.toml` — Add `hyprbridge-platform-win` under Windows cfg

**Files:** `crates/platform-win/Cargo.toml`, `crates/daemon/src/lib.rs`, `crates/daemon/Cargo.toml`

## [S7] Testing Strategy

- Manual test on Windows: `cargo run -p hyprbridge-cli -- status` shows real permissions
- Manual test: Pointer injection via daemon API
- Cross-compilation: `cargo check --target x86_64-pc-windows-msvc` to verify compilation
- No unit tests possible without Windows SDK

## [S8] Non-Goals

- Windows-specific edge detection (can be added later)
- Windows-specific cursor behavior (acceleration, snap)
- Windows Service mode (daemon as Windows service)
- DPI awareness handling (defer to future iteration)
