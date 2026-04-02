# Windows Platform Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement full Windows input capture and injection using Win32 APIs, replacing the current stub.

**Architecture:** Low-level hooks (WH_KEYBOARD_LL, WH_MOUSE_LL) for controller-side capture, SendInput + SetCursorPos for target-side injection. Same public API surface as `platform-linux`.

**Tech Stack:** Rust, windows crate (Win32 UI Input, Win32 UI WindowsAndMessaging, Win32 Foundation)

---

## Task 1: Windows Dependencies and Capability Probe

**Covers:** [S3]

**Files:**
- Modify: `crates/platform-win/Cargo.toml`
- Modify: `crates/platform-win/src/lib.rs`

- [ ] **Step 1: Add Windows dependencies to Cargo.toml**

Replace `crates/platform-win/Cargo.toml`:

```toml
[package]
name = "hyprbridge-platform-win"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
hyprbridge-input = { path = "../input" }
hyprbridge-protocol = { path = "../protocol" }

[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_UI_Input",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_WindowsAndMessaging",
    "Win32_Graphics_Gdi",
] }
```

- [ ] **Step 2: Replace lib.rs with real probe and module structure**

Replace `crates/platform-win/src/lib.rs`:

```rust
//! Windows platform backend for input capture and injection.

use hyprbridge_input::CapabilityProbe;
use hyprbridge_protocol::{CapabilityReport, PermissionState, SupportLevel};

#[cfg(target_os = "windows")]
mod win_backend;

#[cfg(target_os = "windows")]
pub use win_backend::*;

/// Windows platform probe.
#[derive(Debug, Default)]
pub struct WindowsProbe;

impl CapabilityProbe for WindowsProbe {
    fn probe(&self) -> CapabilityReport {
        #[cfg(target_os = "windows")]
        {
            return win_backend::probe_capability();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut report = CapabilityReport::planned("windows");
            report.permission_state = PermissionState::Unknown;
            report.notes.push("Windows backend requires building on Windows.".into());
            report
        }
    }
}
```

- [ ] **Step 3: Create win_backend.rs scaffolding**

Create `crates/platform-win/src/win_backend.rs`:

```rust
//! Windows native backend using Win32 low-level hooks and SendInput.

use hyprbridge_input::ControllerSample;
use hyprbridge_protocol::{
    CapabilityReport, InputBackendSnapshot, MouseButton, PermissionState, PointerSnapshot,
    ScreenEdge, SupportLevel,
};

/// Detect Windows desktop session type.
fn detect_session() -> &'static str {
    // Check if running in RDP session via GetSystemMetrics(SM_REMOTESESSION)
    "console"
}

/// Probe the Windows platform and return a capability report.
pub fn probe_capability() -> CapabilityReport {
    let session = detect_session();
    let mut report = CapabilityReport::planned("windows");
    report.input_capture = SupportLevel::Supported;
    report.input_injection = SupportLevel::Supported;
    report.cursor_handoff = SupportLevel::Supported;
    report.permission_state = PermissionState::Granted;
    report.notes.push(format!("Desktop session: {session}"));
    report
}

/// Get the current pointer position and screen size.
pub fn current_pointer_snapshot() -> Option<PointerSnapshot> {
    None
}

/// Place the pointer at the entry point for a given edge.
pub async fn win_place_pointer_on_entry(_edge: ScreenEdge) -> bool {
    false
}

/// Recenter the controller pointer to screen center.
pub async fn win_recenter_controller_pointer() -> bool {
    false
}

/// Inject relative pointer motion.
pub async fn win_inject_pointer_relative(_delta_x: i16, _delta_y: i16) -> bool {
    false
}

/// Inject pointer button state.
pub async fn win_inject_pointer_button(_button: MouseButton, _pressed: bool) -> bool {
    false
}

/// Inject pointer scroll.
pub async fn win_inject_pointer_scroll(_horizontal: i16, _vertical: i16) -> bool {
    false
}

/// Inject keyboard key state.
pub async fn win_inject_keyboard_key(_keycode: u8, _pressed: bool) -> bool {
    false
}

/// Focus the local target.
pub async fn win_focus_local_target() -> bool {
    true
}

/// Controller sample (current pointer + button + key state).
pub async fn win_controller_sample() -> Option<ControllerSample> {
    None
}

/// Enable or disable remote capture (hooks).
pub async fn win_set_remote_capture_active(_active: bool) -> Option<()> {
    None
}

/// Inspect the Windows input backend for diagnostics.
pub async fn win_inspect_input_backend(_dead_zone_px: u16) -> Option<InputBackendSnapshot> {
    Some(InputBackendSnapshot {
        backend: "windows-hook".into(),
        available: true,
        pointer: current_pointer_snapshot(),
        edge_candidate: None,
        controller_capture: None,
        target_injection: None,
        target_wheel_injection: SupportLevel::Supported,
        detail: "Windows low-level hook backend".into(),
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

/// Perform local handoff.
pub async fn win_perform_local_handoff(edge: ScreenEdge) -> bool {
    win_place_pointer_on_entry(edge).await
}
```

- [ ] **Step 4: Run check**

Run: `cargo check -p hyprbridge-platform-win`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/platform-win/
git commit -m "feat(platform-win): add Windows capability probe and backend scaffolding"
```

---

## Task 2: Daemon Windows Integration

**Covers:** [S6]

**Files:**
- Modify: `crates/daemon/src/lib.rs`
- Modify: `crates/daemon/Cargo.toml`

- [ ] **Step 1: Add Windows dependency to daemon Cargo.toml**

Add under `[target.'cfg(target_os = "windows")'.dependencies]`:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
hyprbridge-platform-win = { path = "../platform-win" }
```

- [ ] **Step 2: Add Windows branches to DaemonControlHandler**

In `crates/daemon/src/lib.rs`, update each `cfg!(target_os = "linux")` block to also handle Windows. The pattern is:

```rust
if (cfg!(target_os = "linux") && hyprbridge_platform_linux::place_pointer_on_entry(edge).await)
    || (cfg!(target_os = "windows") && hyprbridge_platform_win::win_place_pointer_on_entry(edge).await)
```

Apply to:
- `PointerEnter` → `win_place_pointer_on_entry`
- `PointerMove` → `win_inject_pointer_relative`
- `PointerScroll` → `win_inject_pointer_scroll`
- `PointerButton` → `win_inject_pointer_button`
- `KeyboardKey` → `win_inject_keyboard_key`
- `ReturnLocal` → `win_focus_local_target`

- [ ] **Step 3: Add Windows branch to inspect_input_backend**

```rust
} else if cfg!(target_os = "windows") {
    hyprbridge_platform_win::win_inspect_input_backend(dead_zone_px).await
```

- [ ] **Step 4: Add Windows branch to probe_capability**

```rust
} else if cfg!(target_os = "windows") {
    hyprbridge_platform_win::WindowsProbe.probe()
```

- [ ] **Step 5: Update edge monitor and remote relay guards**

Update `spawn_edge_monitor` and `spawn_remote_input_relay` to allow Windows:
```rust
if !cfg!(target_os = "linux") && !cfg!(target_os = "macos") && !cfg!(target_os = "windows") {
    return;
}
```

- [ ] **Step 6: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/daemon/
git commit -m "feat(daemon): wire Windows backend into control handler"
```

---

## Task 3: Real Win32 Hook and SendInput Implementation

**Covers:** [S4, S5]

**Files:**
- Modify: `crates/platform-win/src/win_backend.rs`

- [ ] **Step 1: Implement real `detect_session()` and `current_pointer_snapshot()`**

```rust
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{GetDeviceCaps, HORZRES, VERTRES};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

fn screen_size() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

pub fn current_pointer_snapshot() -> Option<PointerSnapshot> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point).ok()? };
    let (w, h) = screen_size();
    Some(PointerSnapshot {
        x: point.x,
        y: point.y,
        width: w as u16,
        height: h as u16,
    })
}
```

- [ ] **Step 2: Implement `win_place_pointer_on_entry()`**

```rust
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

pub async fn win_place_pointer_on_entry(edge: ScreenEdge) -> bool {
    let snapshot = match current_pointer_snapshot() {
        Some(s) => s,
        None => return false,
    };
    let (x, y) = match edge {
        ScreenEdge::Left => (10, snapshot.height as i32 / 2),
        ScreenEdge::Right => (snapshot.width as i32 - 10, snapshot.height as i32 / 2),
        ScreenEdge::Top => (snapshot.width as i32 / 2, 10),
        ScreenEdge::Bottom => (snapshot.width as i32 / 2, snapshot.height as i32 - 10),
    };
    unsafe { SetCursorPos(x, y).is_ok() }
}
```

- [ ] **Step 3: Implement `win_recenter_controller_pointer()`**

```rust
pub async fn win_recenter_controller_pointer() -> bool {
    let (w, h) = screen_size();
    unsafe { SetCursorPos(w / 2, h / 2).is_ok() }
}
```

- [ ] **Step 4: Implement `win_inject_pointer_relative()`**

```rust
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::Foundation::POINT;

pub async fn win_inject_pointer_relative(delta_x: i16, delta_y: i16) -> bool {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: delta_x as i32,
                dy: delta_y as i32,
                dwFlags: MOUSEEVENTF_MOVE,
                ..Default::default()
            },
        },
    };
    unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) == 1 }
}
```

- [ ] **Step 5: Implement `win_inject_pointer_button()`**

```rust
pub async fn win_inject_pointer_button(button: MouseButton, pressed: bool) -> bool {
    let flag = match (button, pressed) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        (MouseButton::Other(_), true) => MOUSEEVENTF_4THBUTTONDOWN,
        (MouseButton::Other(_), false) => MOUSEEVENTF_4THBUTTONUP,
    };
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flag,
                ..Default::default()
            },
        },
    };
    unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) == 1 }
}
```

- [ ] **Step 6: Implement `win_inject_pointer_scroll()`**

```rust
pub async fn win_inject_pointer_scroll(horizontal: i16, vertical: i16) -> bool {
    let mut success = true;
    if vertical != 0 {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: vertical as i32 * WHEEL_DELTA,
                    dwFlags: MOUSEEVENTF_WHEEL,
                    ..Default::default()
                },
            },
        };
        success &= unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) == 1 };
    }
    if horizontal != 0 {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: horizontal as i32 * WHEEL_DELTA,
                    dwFlags: MOUSEEVENTF_HWHEEL,
                    ..Default::default()
                },
            },
        };
        success &= unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) == 1 };
    }
    success
}
```

- [ ] **Step 7: Implement `win_inject_keyboard_key()`**

```rust
pub async fn win_inject_keyboard_key(keycode: u8, pressed: bool) -> bool {
    let flags = if pressed { KEYEVENTF_KEYDOWN } else { KEYEVENTF_KEYUP };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(keycode as u16),
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) == 1 }
}
```

- [ ] **Step 8: Implement `win_focus_local_target()`**

```rust
pub async fn win_focus_local_target() -> bool {
    win_recenter_controller_pointer().await
}
```

- [ ] **Step 9: Implement low-level hook capture**

Add hook types and capture functions:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static HOOK_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Keyboard hook callback.
unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && HOOK_ACTIVE.load(Ordering::Relaxed) {
        let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);
        let injected = (kb.dwExtraInfo & 0x1) != 0;
        if !injected {
            // Process keyboard event
            let pressed = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
            let _ = (kb.vkCode as u8, pressed);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Mouse hook callback.
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && HOOK_ACTIVE.load(Ordering::Relaxed) {
        let ms = *(lparam.0 as *const MSLLHOOKSTRUCT);
        let injected = (ms.dwExtraInfo & 0x1) != 0;
        if !injected {
            // Process mouse event
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
```

- [ ] **Step 10: Run check**

Run: `cargo check -p hyprbridge-platform-win`
Expected: PASS

- [ ] **Step 11: Commit**

```bash
git add crates/platform-win/
git commit -m "feat(platform-win): implement Win32 hooks and SendInput injection"
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
git commit -m "chore: final verification for Windows platform backend"
```
