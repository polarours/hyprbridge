//! Wayland capability model with a Hyprland-first fallback backend.

use std::{
    env,
    fs::OpenOptions,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
};

use evdev::{
    AttributeSet, InputEvent, KeyCode, RelativeAxisCode,
    event_variants::{KeyEvent, RelativeAxisEvent},
    uinput::VirtualDevice,
};
use hyprbridge_protocol::{
    CapabilityReport, InputBackendSnapshot, LinuxSessionKind, MouseButton, PermissionState,
    PointerSnapshot, ScreenEdge, SupportLevel, TargetInjectionSnapshot, WaylandSupportLevel,
};
use serde_json::Value;

/// Wayland capability probe with compositor-aware notes.
#[derive(Debug, Default)]
pub struct WaylandProbe;

/// Stable display name for the virtual target input device.
pub const HYPRBRIDGE_VIRTUAL_DEVICE_NAME: &str = "HyprBridge Virtual KVM";

impl hyprbridge_input::CapabilityProbe for WaylandProbe {
    fn probe(&self) -> CapabilityReport {
        match HyprlandBackend::detect() {
            Some(backend) => backend.capability_report(),
            None => generic_wayland_report(),
        }
    }
}

/// Hyprland-oriented Wayland backend used for honest fallback support.
#[derive(Debug, Clone)]
pub struct HyprlandBackend {
    runtime_dir: PathBuf,
    instance_signature: String,
    desktop: String,
}

impl HyprlandBackend {
    /// Detect whether this Wayland session is Hyprland with reachable IPC sockets.
    #[must_use]
    pub fn detect() -> Option<Self> {
        let instance_signature = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
        let runtime_dir = PathBuf::from(env::var("XDG_RUNTIME_DIR").ok()?);
        let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "Hyprland".into());
        let backend = Self {
            runtime_dir,
            instance_signature,
            desktop,
        };
        if backend.command_socket_path().exists() && backend.event_socket_path().exists() {
            Some(backend)
        } else {
            None
        }
    }

    /// Probe Hyprland state for daemon diagnostics.
    pub fn inspect(&self, threshold_px: u16) -> Result<InputBackendSnapshot, String> {
        let pointer = self.pointer_snapshot()?;
        let injection = target_injection_snapshot();
        Ok(InputBackendSnapshot {
            backend: "wayland-hyprland".into(),
            available: true,
            edge_candidate: edge_candidate_for(&pointer, threshold_px),
            pointer: Some(pointer),
            controller_capture: None,
            target_injection: Some(injection.clone()),
            target_wheel_injection: if injection.uinput_writable {
                SupportLevel::Supported
            } else {
                SupportLevel::Planned
            },
            detail: format!(
                "Hyprland IPC detected via {} and {}. Preferred target injection backend: {}.",
                self.command_socket_path().display(),
                self.event_socket_path().display(),
                injection.primary_backend
            ),
        })
    }

    /// Read the current global cursor position and composite desktop extent.
    pub fn pointer_snapshot(&self) -> Result<PointerSnapshot, String> {
        let cursor = self.request_json("cursorpos", false)?;
        let monitors = self.request_json("monitors", true)?;

        let x = cursor
            .get("x")
            .and_then(Value::as_f64)
            .ok_or_else(|| "Hyprland cursor position did not include x".to_string())?;
        let y = cursor
            .get("y")
            .and_then(Value::as_f64)
            .ok_or_else(|| "Hyprland cursor position did not include y".to_string())?;

        let (width, height) = monitor_bounds(&monitors)?;
        Ok(PointerSnapshot {
            x: round_to_i32(x),
            y: round_to_i32(y),
            width,
            height,
        })
    }

    /// Place the pointer just inside the specified entry edge.
    pub fn enter_from_edge(&self, edge: ScreenEdge) -> Result<(), String> {
        let pointer = self.pointer_snapshot()?;
        let center_x = i32::from(pointer.width) / 2;
        let center_y = i32::from(pointer.height) / 2;
        let (x, y) = match edge {
            ScreenEdge::Left => (1, center_y),
            ScreenEdge::Right => (i32::from(pointer.width.saturating_sub(2)), center_y),
            ScreenEdge::Top => (center_x, 1),
            ScreenEdge::Bottom => (center_x, i32::from(pointer.height.saturating_sub(2))),
        };
        self.move_cursor_absolute(x, y)
    }

    /// Recenter the pointer on the composite desktop.
    pub fn center_pointer(&self) -> Result<(), String> {
        let pointer = self.pointer_snapshot()?;
        self.move_cursor_absolute(i32::from(pointer.width) / 2, i32::from(pointer.height) / 2)
    }

    /// Move the pointer relative to its current global position.
    pub fn move_pointer_relative(&self, delta_x: i16, delta_y: i16) -> Result<(), String> {
        if uinput_move_pointer_relative(delta_x, delta_y).is_ok() {
            return Ok(());
        }
        let pointer = self.pointer_snapshot()?;
        self.move_cursor_absolute(
            clamp_axis(pointer.x + i32::from(delta_x), pointer.width),
            clamp_axis(pointer.y + i32::from(delta_y), pointer.height),
        )
    }

    /// Inject a keyboard or mouse button state through Hyprland's dispatcher fallback.
    pub fn send_key_state(&self, keycode: u8, pressed: bool) -> Result<(), String> {
        if uinput_send_key_state(keycode, pressed).is_ok() {
            return Ok(());
        }
        let selector = self.active_window_selector()?;
        self.dispatch(format!(
            "sendkeystate , code:{keycode}, {}, {selector}",
            if pressed { "down" } else { "up" },
        ))
    }

    /// Inject a pointer button state through Hyprland's dispatcher fallback.
    pub fn send_pointer_button(&self, button: MouseButton, pressed: bool) -> Result<(), String> {
        if uinput_send_pointer_button(button, pressed).is_ok() {
            return Ok(());
        }
        let selector = self.active_window_selector()?;
        self.dispatch(format!(
            "sendkeystate , {}, {}, {selector}",
            hyprland_button_name(button)?,
            if pressed { "down" } else { "up" },
        ))
    }

    fn capability_report(&self) -> CapabilityReport {
        let mut notes = vec![format!(
            "Hyprland session detected under `{}` with IPC sockets available.",
            self.desktop
        )];
        let injection = target_injection_snapshot();
        notes.push(
            if injection.uinput_writable {
                "Pointer, wheel, keyboard, and button injection prefer a virtual uinput device when /dev/uinput is writable; Hyprland IPC/dispatcher remains the fallback path."
                    .into()
            } else {
                "Pointer inspection and pointer injection use Hyprland IPC. Keyboard and pointer-button injection use Hyprland dispatcher fallback directed at the active window address."
                    .into()
            }
        );
        notes.push(
            if injection.uinput_writable {
                "Writable /dev/uinput is available, so Wayland target injection can run through a virtual input device."
                    .into()
            } else {
                "Writable /dev/uinput was not detected, so HyprBridge stays on Hyprland-specific fallback injection paths."
                    .into()
            }
        );
        notes.push(
            "Global keyboard capture remains constrained on Wayland; HyprBridge currently treats Hyprland as a partial-but-runnable backend."
                .into(),
        );

        CapabilityReport {
            platform: "linux".into(),
            linux_session: LinuxSessionKind::Wayland,
            wayland_support: WaylandSupportLevel::Partial,
            input_capture: SupportLevel::Planned,
            input_injection: SupportLevel::Supported,
            cursor_handoff: SupportLevel::Supported,
            permission_state: PermissionState::Granted,
            notes,
        }
    }

    fn command_socket_path(&self) -> PathBuf {
        self.runtime_dir
            .join("hypr")
            .join(&self.instance_signature)
            .join(".socket.sock")
    }

    fn event_socket_path(&self) -> PathBuf {
        self.runtime_dir
            .join("hypr")
            .join(&self.instance_signature)
            .join(".socket2.sock")
    }

    fn request_json(&self, command: &str, json: bool) -> Result<Value, String> {
        let payload = self.request_raw(command, json)?;
        serde_json::from_str(&payload)
            .map_err(|error| format!("failed to parse Hyprland response for {command}: {error}"))
    }

    fn dispatch(&self, command: String) -> Result<(), String> {
        let _ = self.request_raw(&format!("dispatch {command}"), false)?;
        Ok(())
    }

    fn active_window_selector(&self) -> Result<String, String> {
        let active = self.request_json("activewindow", true)?;
        match active.get("address").and_then(Value::as_str) {
            Some(address) if !address.is_empty() => Ok(format!("address:{address}")),
            _ => Ok("activewindow".into()),
        }
    }

    fn move_cursor_absolute(&self, x: i32, y: i32) -> Result<(), String> {
        self.dispatch(format!("movecursor {x} {y}"))
    }

    fn request_raw(&self, command: &str, json: bool) -> Result<String, String> {
        let mut stream = UnixStream::connect(self.command_socket_path()).map_err(|error| {
            format!(
                "failed to connect to Hyprland command socket {}: {error}",
                self.command_socket_path().display()
            )
        })?;
        let request = if json {
            format!("j/{command}")
        } else {
            format!("/{command}")
        };
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("failed to write Hyprland request `{command}`: {error}"))?;
        let _ = stream.shutdown(std::net::Shutdown::Write);

        let mut raw = String::new();
        stream.read_to_string(&mut raw).map_err(|error| {
            format!("failed to read Hyprland response for `{command}`: {error}")
        })?;
        Ok(raw)
    }
}

/// Inspect Hyprland native runtime state when available.
pub fn inspect_hyprland_backend(dead_zone_px: u16) -> Option<InputBackendSnapshot> {
    HyprlandBackend::detect()?.inspect(dead_zone_px).ok()
}

/// Return the current edge candidate using Hyprland cursor inspection.
pub fn hyprland_edge_candidate(dead_zone_px: u16) -> Option<ScreenEdge> {
    let pointer = HyprlandBackend::detect()?.pointer_snapshot().ok()?;
    edge_candidate_for(&pointer, dead_zone_px)
}

/// Place the Hyprland pointer on an entry edge.
pub fn hyprland_place_pointer_on_entry(edge: ScreenEdge) -> bool {
    HyprlandBackend::detect()
        .and_then(|backend| backend.enter_from_edge(edge).ok())
        .is_some()
}

/// Recenter the Hyprland cursor for relative relay loops.
pub fn hyprland_recenter_pointer() -> bool {
    HyprlandBackend::detect()
        .and_then(|backend| backend.center_pointer().ok())
        .is_some()
}

/// Inject relative pointer motion through Hyprland IPC.
pub fn hyprland_inject_pointer_relative(delta_x: i16, delta_y: i16) -> bool {
    HyprlandBackend::detect()
        .and_then(|backend| backend.move_pointer_relative(delta_x, delta_y).ok())
        .is_some()
}

/// Inject keyboard state through Hyprland's dispatcher fallback.
pub fn hyprland_inject_keyboard_key(keycode: u8, pressed: bool) -> bool {
    HyprlandBackend::detect()
        .and_then(|backend| backend.send_key_state(keycode, pressed).ok())
        .is_some()
}

/// Inject pointer button state through Hyprland's dispatcher fallback.
pub fn hyprland_inject_pointer_button(button: MouseButton, pressed: bool) -> bool {
    HyprlandBackend::detect()
        .and_then(|backend| backend.send_pointer_button(button, pressed).ok())
        .is_some()
}

/// Wheel injection is not yet implemented for Hyprland's dispatcher fallback.
pub fn hyprland_inject_pointer_scroll(horizontal_steps: i16, vertical_steps: i16) -> bool {
    uinput_scroll(horizontal_steps, vertical_steps).is_ok()
}

/// Collect a pointer-only sample from Hyprland.
pub fn hyprland_pointer_sample() -> Option<hyprbridge_input::ControllerSample> {
    let pointer = HyprlandBackend::detect()?.pointer_snapshot().ok()?;
    Some(hyprbridge_input::ControllerSample {
        pointer,
        buttons_mask: 0,
        keymap: [0; 32],
        scroll_horizontal_steps: 0,
        scroll_vertical_steps: 0,
    })
}

/// Generic Wayland fallback when no compositor-specific backend is available.
fn generic_wayland_report() -> CapabilityReport {
    let compositor = env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into());
    let mut notes = vec![format!(
        "Wayland session detected under `{compositor}`. Global input support remains compositor-dependent."
    )];
    if let Ok(display) = env::var("WAYLAND_DISPLAY") {
        notes.push(format!("WAYLAND_DISPLAY is set to `{display}`."));
    }

    CapabilityReport {
        platform: "linux".into(),
        linux_session: LinuxSessionKind::Wayland,
        wayland_support: WaylandSupportLevel::Unsupported,
        input_capture: SupportLevel::Planned,
        input_injection: if probe_uinput_access().is_ok() {
            SupportLevel::Supported
        } else {
            SupportLevel::Planned
        },
        cursor_handoff: SupportLevel::Planned,
        permission_state: PermissionState::Unknown,
        notes,
    }
}

fn probe_uinput_access() -> Result<(), String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput")
        .map(|_| ())
        .map_err(|error| format!("could not open /dev/uinput for read/write: {error}"))
}

fn target_injection_snapshot() -> TargetInjectionSnapshot {
    let state = lock_uinput_state();
    let uinput_probe = probe_uinput_access();
    TargetInjectionSnapshot {
        primary_backend: if uinput_probe.is_ok() {
            if state.device.is_some() {
                "uinput-active".into()
            } else {
                "uinput-ready".into()
            }
        } else {
            "hyprland-fallback".into()
        },
        fallback_backend: Some("hyprland-ipc-dispatcher".into()),
        uinput_writable: uinput_probe.is_ok(),
        last_error: state.last_error.clone().or_else(|| uinput_probe.err()),
    }
}

fn uinput_send_key_state(keycode: u8, pressed: bool) -> Result<(), String> {
    let evdev_keycode = x11_keycode_to_evdev(keycode)
        .ok_or_else(|| format!("X11 keycode {keycode} could not be translated to evdev"))?;
    emit_uinput_events(&[*KeyEvent::new(evdev_keycode, if pressed { 1 } else { 0 })])
}

fn uinput_send_pointer_button(button: MouseButton, pressed: bool) -> Result<(), String> {
    emit_uinput_events(&[*KeyEvent::new(
        mouse_button_to_evdev(button)?,
        if pressed { 1 } else { 0 },
    )])
}

fn uinput_move_pointer_relative(delta_x: i16, delta_y: i16) -> Result<(), String> {
    let mut events = Vec::new();
    if delta_x != 0 {
        events.push(*RelativeAxisEvent::new(
            RelativeAxisCode::REL_X,
            i32::from(delta_x),
        ));
    }
    if delta_y != 0 {
        events.push(*RelativeAxisEvent::new(
            RelativeAxisCode::REL_Y,
            i32::from(delta_y),
        ));
    }
    if events.is_empty() {
        return Ok(());
    }
    emit_uinput_events(&events)
}

fn uinput_scroll(horizontal_steps: i16, vertical_steps: i16) -> Result<(), String> {
    let mut events = Vec::new();
    if horizontal_steps != 0 {
        events.push(*RelativeAxisEvent::new(
            RelativeAxisCode::REL_HWHEEL,
            i32::from(horizontal_steps),
        ));
    }
    if vertical_steps != 0 {
        events.push(*RelativeAxisEvent::new(
            RelativeAxisCode::REL_WHEEL,
            i32::from(vertical_steps),
        ));
    }
    if events.is_empty() {
        return Ok(());
    }
    emit_uinput_events(&events)
}

fn emit_uinput_events(events: &[InputEvent]) -> Result<(), String> {
    let mut state = lock_uinput_state();
    if state.device.is_none() {
        match create_uinput_device() {
            Ok(device) => {
                state.device = Some(device);
                state.last_error = None;
            }
            Err(error) => {
                state.last_error = Some(error.clone());
                return Err(error);
            }
        }
    }

    let Some(device) = state.device.as_mut() else {
        return Err(state
            .last_error
            .clone()
            .unwrap_or_else(|| "uinput device unavailable".into()));
    };
    device.emit(events).map_err(|error| {
        let detail = format!("failed to emit uinput events: {error}");
        state.last_error = Some(detail.clone());
        state.device = None;
        detail
    })
}

fn create_uinput_device() -> Result<VirtualDevice, String> {
    let keys = supported_uinput_keys();
    let axes = supported_uinput_axes();
    VirtualDevice::builder()
        .map_err(|error| format!("failed to open /dev/uinput: {error}"))?
        .name(HYPRBRIDGE_VIRTUAL_DEVICE_NAME)
        .with_keys(&keys)
        .map_err(|error| format!("failed to configure uinput keys: {error}"))?
        .with_relative_axes(&axes)
        .map_err(|error| format!("failed to configure uinput relative axes: {error}"))?
        .build()
        .map_err(|error| format!("failed to build uinput virtual device: {error}"))
}

fn supported_uinput_keys() -> AttributeSet<KeyCode> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for code in 1_u16..=247_u16 {
        keys.insert(KeyCode::new(code));
    }
    for button in [
        KeyCode::BTN_LEFT,
        KeyCode::BTN_RIGHT,
        KeyCode::BTN_MIDDLE,
        KeyCode::BTN_SIDE,
        KeyCode::BTN_EXTRA,
    ] {
        keys.insert(button);
    }
    keys
}

fn supported_uinput_axes() -> AttributeSet<RelativeAxisCode> {
    [
        RelativeAxisCode::REL_X,
        RelativeAxisCode::REL_Y,
        RelativeAxisCode::REL_WHEEL,
        RelativeAxisCode::REL_HWHEEL,
    ]
    .into_iter()
    .collect()
}

fn mouse_button_to_evdev(button: MouseButton) -> Result<KeyCode, String> {
    match button {
        MouseButton::Left => Ok(KeyCode::BTN_LEFT),
        MouseButton::Middle => Ok(KeyCode::BTN_MIDDLE),
        MouseButton::Right => Ok(KeyCode::BTN_RIGHT),
        MouseButton::Other(code) => Ok(KeyCode::new(u16::from(code))),
    }
}

fn x11_keycode_to_evdev(keycode: u8) -> Option<KeyCode> {
    keycode
        .checked_sub(8)
        .map(|code| KeyCode::new(u16::from(code)))
}

fn uinput_state() -> &'static Mutex<UinputState> {
    static STATE: OnceLock<Mutex<UinputState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(UinputState::default()))
}

fn lock_uinput_state() -> MutexGuard<'static, UinputState> {
    match uinput_state().lock() {
        Ok(state) => state,
        Err(poisoned) => {
            let mut state = poisoned.into_inner();
            let recovery_note =
                "recovered from a poisoned uinput state lock; the virtual device will be recreated";
            state.device = None;
            state.last_error = Some(match state.last_error.take() {
                Some(previous) => format!("{previous}; {recovery_note}"),
                None => recovery_note.into(),
            });
            state
        }
    }
}

#[derive(Debug, Default)]
struct UinputState {
    device: Option<VirtualDevice>,
    last_error: Option<String>,
}

fn edge_candidate_for(pointer: &PointerSnapshot, threshold_px: u16) -> Option<ScreenEdge> {
    if pointer.x <= i32::from(threshold_px) {
        return Some(ScreenEdge::Left);
    }
    if pointer.y <= i32::from(threshold_px) {
        return Some(ScreenEdge::Top);
    }
    if pointer.x >= i32::from(pointer.width.saturating_sub(threshold_px.saturating_add(1))) {
        return Some(ScreenEdge::Right);
    }
    if pointer.y
        >= i32::from(
            pointer
                .height
                .saturating_sub(threshold_px.saturating_add(1)),
        )
    {
        return Some(ScreenEdge::Bottom);
    }
    None
}

fn monitor_bounds(monitors: &Value) -> Result<(u16, u16), String> {
    let monitors = monitors
        .as_array()
        .ok_or_else(|| "Hyprland monitors payload was not an array".to_string())?;
    if monitors.is_empty() {
        return Err("Hyprland did not report any monitors".into());
    }

    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for monitor in monitors {
        let x = monitor
            .get("x")
            .and_then(Value::as_i64)
            .ok_or_else(|| "monitor entry missing x".to_string())? as i32;
        let y = monitor
            .get("y")
            .and_then(Value::as_i64)
            .ok_or_else(|| "monitor entry missing y".to_string())? as i32;
        let width = monitor
            .get("width")
            .and_then(Value::as_u64)
            .ok_or_else(|| "monitor entry missing width".to_string())? as i32;
        let height = monitor
            .get("height")
            .and_then(Value::as_u64)
            .ok_or_else(|| "monitor entry missing height".to_string())? as i32;

        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + width);
        max_y = max_y.max(y + height);
    }

    let width = (max_x - min_x).max(1);
    let height = (max_y - min_y).max(1);
    Ok((
        u16::try_from(width).unwrap_or(u16::MAX),
        u16::try_from(height).unwrap_or(u16::MAX),
    ))
}

fn round_to_i32(value: f64) -> i32 {
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn clamp_axis(value: i32, dimension: u16) -> i32 {
    value.clamp(1, i32::from(dimension.saturating_sub(2)).max(1))
}

fn hyprland_button_name(button: MouseButton) -> Result<String, String> {
    match button {
        MouseButton::Left => Ok("mouse:272".into()),
        MouseButton::Right => Ok("mouse:273".into()),
        MouseButton::Middle => Ok("mouse:274".into()),
        MouseButton::Other(code) if code > 0 => Ok(format!("mouse:{code}")),
        MouseButton::Other(_) => Err("Hyprland button code 0 is invalid".into()),
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use serde_json::json;

    use super::{edge_candidate_for, monitor_bounds};
    use hyprbridge_protocol::{PointerSnapshot, ScreenEdge};

    #[test]
    fn edge_detection_matches_wayland_pointer_snapshots() {
        let pointer = PointerSnapshot {
            x: 1918,
            y: 15,
            width: 1920,
            height: 1080,
        };
        assert_eq!(edge_candidate_for(&pointer, 8), Some(ScreenEdge::Right));
    }

    #[test]
    fn monitor_bounds_span_multiple_outputs() {
        let monitors = json!([
            { "x": 0, "y": 0, "width": 1920, "height": 1080 },
            { "x": 1920, "y": 0, "width": 2560, "height": 1440 }
        ]);
        assert_eq!(monitor_bounds(&monitors).unwrap_or_default(), (4480, 1440));
    }

    #[test]
    fn hyprland_detection_requires_expected_sockets() {
        let temp =
            std::env::temp_dir().join(format!("hyprbridge-wayland-test-{}", std::process::id()));
        let hypr_dir = temp.join("hypr").join("instance-a");
        fs::create_dir_all(&hypr_dir).expect("temp dir should be created");
        fs::write(hypr_dir.join(".socket.sock"), []).expect("command socket placeholder");
        fs::write(hypr_dir.join(".socket2.sock"), []).expect("event socket placeholder");

        unsafe {
            env::set_var("XDG_RUNTIME_DIR", &temp);
            env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "instance-a");
        }
        assert!(super::HyprlandBackend::detect().is_some());
        unsafe {
            env::remove_var("XDG_RUNTIME_DIR");
            env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        }
        std::fs::remove_dir_all(temp).expect("temp dir cleanup");
    }
}
