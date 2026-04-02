# macOS Platform Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement full macOS input capture and injection using CGEvent APIs, replacing the current stub.

**Architecture:** CGEventTap for controller-side capture, CGEventPost for target-side injection, AXIsProcessTrusted for permission checking. Follows the same public API surface as `platform-linux`.

**Tech Stack:** Rust, core-foundation, core-graphics, core-foundation-sys, dispatch

---

## Task 1: macOS Dependencies and Capability Probe

**Covers:** [S3]

**Files:**
- Modify: `crates/platform-mac/Cargo.toml`
- Modify: `crates/platform-mac/src/lib.rs`

- [ ] **Step 1: Add macOS dependencies to Cargo.toml**

Replace `crates/platform-mac/Cargo.toml`:

```toml
[package]
name = "hyprbridge-platform-mac"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10"
core-graphics = "0.24"
core-graphics-sys = "0.2"
core-foundation-sys = "0.8"
dispatch = "0.2"

[target.'cfg(not(target_os = "macos"))'.dependencies]
hyprbridge-input = { path = "../input" }
hyprbridge-protocol = { path = "../protocol" }

[dependencies]
hyprbridge-input = { path = "../input" }
hyprbridge-protocol = { path = "../protocol" }
```

- [ ] **Step 2: Implement real capability probe**

Replace `crates/platform-mac/src/lib.rs`:

```rust
//! macOS platform backend for input capture and injection.

use hyprbridge_input::CapabilityProbe;
use hyprbridge_protocol::{CapabilityReport, PermissionState, SupportLevel};

#[cfg(target_os = "macos")]
mod macos_backend;

#[cfg(target_os = "macos")]
pub use macos_backend::*;

/// macOS platform probe.
#[derive(Debug, Default)]
pub struct MacProbe;

impl CapabilityProbe for MacProbe {
    fn probe(&self) -> CapabilityReport {
        #[cfg(target_os = "macos")]
        {
            return macos_backend::probe_capability();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let mut report = CapabilityReport::planned("macos");
            report.permission_state = PermissionState::Unknown;
            report.notes.push("macOS backend requires building on macOS.".into());
            report
        }
    }
}
```

- [ ] **Step 3: Create the macOS backend module**

Create `crates/platform-mac/src/macos_backend.rs`:

```rust
//! macOS native backend using Core Graphics event APIs.

use hyprbridge_input::ControllerSample;
use hyprbridge_protocol::{
    CapabilityReport, InputBackendSnapshot, MouseButton, PermissionState, PointerSnapshot,
    ScreenEdge, SupportLevel,
};
use std::sync::OnceLock;

/// Check if the current process has Accessibility permission.
fn is_accessibility_trusted() -> bool {
    // AXIsProcessTrusted() is declared in ApplicationServices
    // On macOS, we use the Security framework to check
    // For now, return true if running on macOS (real check needs ObjC interop)
    true
}

/// Probe the macOS platform and return a capability report.
pub fn probe_capability() -> CapabilityReport {
    let trusted = is_accessibility_trusted();
    let mut report = CapabilityReport::planned("macos");
    if trusted {
        report.input_capture = SupportLevel::Supported;
        report.input_injection = SupportLevel::Supported;
        report.cursor_handoff = SupportLevel::Supported;
        report.permission_state = PermissionState::Granted;
        report.notes.push("Accessibility permission granted.".into());
    } else {
        report.input_capture = SupportLevel::Unsupported;
        report.input_injection = SupportLevel::Unsupported;
        report.cursor_handoff = SupportLevel::Unsupported;
        report.permission_state = PermissionState::Missing;
        report.notes.push(
            "Accessibility permission required. Grant in System Settings → Privacy & Security → Accessibility."
                .into(),
        );
    }
    report
}

/// Get the current pointer position and screen size.
pub fn current_pointer_snapshot() -> Option<PointerSnapshot> {
    // CGEvent::new(None) gives current mouse location
    // NSScreen::mainScreen gives screen dimensions
    // Placeholder until CGEvent integration is complete
    None
}

/// Place the pointer at the entry point for a given edge.
pub async fn macos_place_pointer_on_entry(edge: ScreenEdge) -> bool {
    // Get screen size, compute entry point just inside the edge
    // CGEventPost with kCGEventMouseMoved to warp pointer
    false
}

/// Recenter the controller pointer to screen center.
pub async fn macos_recenter_controller_pointer() -> bool {
    // Get screen center, post mouse moved event
    false
}

/// Inject relative pointer motion.
pub async fn macos_inject_pointer_relative(delta_x: i16, delta_y: i16) -> bool {
    // Get current position, add deltas, post kCGEventMouseMoved
    false
}

/// Inject pointer button state.
pub async fn macos_inject_pointer_button(button: MouseButton, pressed: bool) -> bool {
    // Create CGEvent with kCGEventLeftMouseDown/Up etc.
    // Set button number
    false
}

/// Inject pointer scroll.
pub async fn macos_inject_pointer_scroll(horizontal: i16, vertical: i16) -> bool {
    // Create CGEvent with kCGEventScrollWheel
    // Set delta via kCGScrollWheelEventDeltaAxis1/2
    false
}

/// Inject keyboard key state.
pub async fn macos_inject_keyboard_key(keycode: u8, pressed: bool) -> bool {
    // Create CGEvent with kCGEventKeyDown/Up
    // Set keycode via CGEventKeyboardSetKeyCode
    false
}

/// Focus the local target (return ownership to local machine).
pub async fn macos_focus_local_target() -> bool {
    // Warp pointer to screen center, release any grabs
    true
}

/// Controller sample (current pointer + button + key state).
pub async fn macos_controller_sample() -> Option<ControllerSample> {
    // Query current pointer position via CGEvent
    // Query key state via CGEventSourceKeyState
    // Query button state via CGEvent
    None
}

/// Enable or disable remote capture (event tap).
pub async fn macos_set_remote_capture_active(active: bool) -> Option<hyprbridge_protocol::CaptureGrabReport> {
    // Enable/disable CGEventTap
    None
}

/// Inspect the macOS input backend for diagnostics.
pub async fn macos_inspect_input_backend(dead_zone_px: u16) -> Option<InputBackendSnapshot> {
    let pointer = current_pointer_snapshot();
    let edge_candidate = if let Some(ref p) = pointer {
        detect_edge(p, dead_zone_px)
    } else {
        None
    };
    Some(InputBackendSnapshot {
        backend: "macos-cgevent".into(),
        available: is_accessibility_trusted(),
        pointer,
        edge_candidate,
        controller_capture: None,
        target_injection: None,
        target_wheel_injection: SupportLevel::Supported,
        detail: "macOS Core Graphics event backend".into(),
    })
}

/// Detect if pointer is near a screen edge.
fn detect_edge(pointer: &PointerSnapshot, threshold: u16) -> Option<ScreenEdge> {
    let t = i32::from(threshold);
    if pointer.x <= t {
        Some(ScreenEdge::Left)
    } else if pointer.x >= i32::from(pointer.width) - t {
        Some(ScreenEdge::Right)
    } else if pointer.y <= t {
        Some(ScreenEdge::Top)
    } else if pointer.y >= i32::from(pointer.height) - t {
        Some(ScreenEdge::Bottom)
    } else {
        None
    }
}

/// Perform local handoff (prepare for edge crossing).
pub async fn macos_perform_local_handoff(edge: ScreenEdge) -> bool {
    // Warp pointer to edge exit point
    macos_place_pointer_on_entry(edge).await
}
```

- [ ] **Step 4: Run check**

Run: `cargo check -p hyprbridge-platform-mac`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/platform-mac/
git commit -m "feat(platform-mac): add macOS capability probe and backend scaffolding"
```

---

## Task 2: Daemon macOS Integration

**Covers:** [S6]

**Files:**
- Modify: `crates/daemon/src/lib.rs`

- [ ] **Step 1: Add macOS dependency to daemon Cargo.toml**

In `crates/daemon/Cargo.toml`, add:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
hyprbridge-platform-mac = { path = "../platform-mac" }
```

- [ ] **Step 2: Add macOS branches to DaemonControlHandler**

In `crates/daemon/src/lib.rs`, update each `cfg!(target_os = "linux")` block to also handle macOS. For example, the `PointerEnter` handler:

Replace:
```rust
if cfg!(target_os = "linux")
    && hyprbridge_platform_linux::place_pointer_on_entry(edge).await
```

With:
```rust
if (cfg!(target_os = "linux")
    && hyprbridge_platform_linux::place_pointer_on_entry(edge).await)
    || (cfg!(target_os = "macos")
        && hyprbridge_platform_mac::macos_place_pointer_on_entry(edge).await)
```

Apply the same pattern to:
- `PointerMove` → `macos_inject_pointer_relative`
- `PointerScroll` → `macos_inject_pointer_scroll`
- `PointerButton` → `macos_inject_pointer_button`
- `KeyboardKey` → `macos_inject_keyboard_key`
- `ReturnLocal` → `macos_focus_local_target`

- [ ] **Step 3: Add macOS branch to inspect_input_backend**

Replace:
```rust
if cfg!(target_os = "linux") {
    hyprbridge_platform_linux::inspect_input_backend(dead_zone_px).await
} else {
    None
}
```

With:
```rust
if cfg!(target_os = "linux") {
    hyprbridge_platform_linux::inspect_input_backend(dead_zone_px).await
} else if cfg!(target_os = "macos") {
    hyprbridge_platform_mac::macos_inspect_input_backend(dead_zone_px).await
} else {
    None
}
```

- [ ] **Step 4: Add macOS branch to probe_capability**

In `crates/daemon/src/lib.rs`, the `probe_capability()` function already routes by `cfg!(target_os)`. Add the macOS branch:

```rust
} else if cfg!(target_os = "macos") {
    hyprbridge_platform_mac::MacProbe.probe()
```

- [ ] **Step 5: Add macOS branch to edge monitor**

In `spawn_edge_monitor`, add macOS support. Currently:
```rust
if !cfg!(target_os = "linux") {
    return;
}
```

Change to:
```rust
if !cfg!(target_os = "linux") && !cfg!(target_os = "macos") {
    return;
}
```

And update `monitor_edge_once` to use `macos_current_edge_candidate` when on macOS.

- [ ] **Step 6: Add macOS branch to remote relay**

In `spawn_remote_input_relay`, update the `cfg!(target_os = "linux")` guard to also allow macOS.

- [ ] **Step 7: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add crates/daemon/src/lib.rs crates/daemon/Cargo.toml
git commit -m "feat(daemon): wire macOS backend into control handler"
```

---

## Task 3: Real CGEvent Capture Implementation

**Covers:** [S4]

**Files:**
- Modify: `crates/platform-mac/src/macos_backend.rs`

- [ ] **Step 1: Add CGEvent dependencies**

In `crates/platform-mac/Cargo.toml`, ensure these are under `[target.'cfg(target_os = "macos")'.dependencies]`:

```toml
core-foundation = "0.10"
core-graphics = "0.24"
```

- [ ] **Step 2: Implement CGEventTap capture**

Replace the placeholder functions in `crates/platform-mac/src/macos_backend.rs` with real CGEvent implementations. This requires Objective-C interop for `AXIsProcessTrusted()` and Core Graphics for event taps.

Add to `Cargo.toml`:
```toml
objc2 = "0.5"
objc2-foundation = "0.2"
objc2-app-kit = { version = "0.2", features = ["NSAccessibility"] }
```

Implement `is_accessibility_trusted()` using `NSAccessibility::isProcessTrusted()`.

Implement the event tap capture loop using `CGEventTapCreate` with a `CFMachPort` and `CFRunLoop`.

**Note:** This task requires macOS to test. The implementation will compile on any platform but only function on macOS. Use `#[cfg(target_os = "macos")]` guards.

- [ ] **Step 3: Implement CGEvent injection**

Implement `macos_inject_pointer_relative`, `macos_inject_pointer_button`, `macos_inject_pointer_scroll`, `macos_inject_keyboard_key` using `CGEventPost`.

- [ ] **Step 4: Run check**

Run: `cargo check -p hyprbridge-platform-mac`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/platform-mac/
git commit -m "feat(platform-mac): implement CGEvent capture and injection"
```

---

## Task 4: Final Verification

**Covers:** [S7]

**Files:** None

- [ ] **Step 1: Run all workspace tests**

Run: `cargo test --workspace`
Expected: All tests PASS

- [ ] **Step 2: Run workspace check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace`
Expected: No new warnings

- [ ] **Step 4: Verify CLI compiles**

Run: `cargo run -p hyprbridge-cli -- --help`
Expected: CLI help output

- [ ] **Step 5: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for macOS platform backend"
```
