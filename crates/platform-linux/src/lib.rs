//! Linux session detection and capability routing.

use std::{
    io::ErrorKind,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
    time::{Duration, Instant},
};

use evdev::{Device, EventSummary, KeyCode, RelativeAxisCode, enumerate};
use hyprbridge_input::{
    BoundaryHandoffBackend, CapabilityProbe, ControllerInputBackend, ControllerSample,
    TargetInjectionBackend,
};
use hyprbridge_platform_wayland::{
    HYPRBRIDGE_VIRTUAL_DEVICE_NAME, WaylandProbe, hyprland_edge_candidate,
    hyprland_inject_keyboard_key, hyprland_inject_pointer_button, hyprland_inject_pointer_relative,
    hyprland_inject_pointer_scroll, hyprland_place_pointer_on_entry, hyprland_pointer_sample,
    hyprland_recenter_pointer, inspect_hyprland_backend,
};
use hyprbridge_platform_x11::{X11Backend, X11Probe};
use hyprbridge_protocol::{
    CapabilityReport, ControllerCaptureSnapshot, InputBackendSnapshot, LinuxSessionKind,
    MouseButton, ScreenEdge, SupportLevel,
};

/// Detect the current Linux session kind.
#[must_use]
pub fn detect_session() -> LinuxSessionKind {
    match std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "x11" => LinuxSessionKind::X11,
        "wayland" => LinuxSessionKind::Wayland,
        _ if std::env::var("WAYLAND_DISPLAY").is_ok() => LinuxSessionKind::Wayland,
        _ if std::env::var("DISPLAY").is_ok() => LinuxSessionKind::X11,
        _ => LinuxSessionKind::Unknown,
    }
}

/// Returns `true` when the current Linux session is X11.
#[must_use]
pub fn is_x11_session() -> bool {
    detect_session() == LinuxSessionKind::X11
}

/// Probe Linux capabilities based on the detected session.
#[must_use]
pub fn probe_local() -> CapabilityReport {
    match detect_session() {
        LinuxSessionKind::X11 => X11Probe.probe(),
        LinuxSessionKind::Wayland => probe_wayland(),
        LinuxSessionKind::Unknown => CapabilityReport::planned("linux"),
    }
}

/// Inspect the active native input backend for the current Linux session.
pub async fn inspect_input_backend(dead_zone_px: u16) -> Option<InputBackendSnapshot> {
    match detect_session() {
        LinuxSessionKind::X11 => {
            let backend = X11Backend::new().ok()?;
            backend.snapshot(dead_zone_px).await.ok()
        }
        LinuxSessionKind::Wayland => inspect_wayland_backend(dead_zone_px),
        LinuxSessionKind::Unknown => None,
    }
}

/// Read the current native edge candidate for Linux when available.
pub async fn current_edge_candidate(dead_zone_px: u16) -> Option<ScreenEdge> {
    match detect_session() {
        LinuxSessionKind::X11 => {
            let backend = X11Backend::new().ok()?;
            backend.edge_candidate(dead_zone_px).await.ok().flatten()
        }
        LinuxSessionKind::Wayland => hyprland_edge_candidate(dead_zone_px),
        LinuxSessionKind::Unknown => None,
    }
}

/// Capture a richer controller-side input sample for remote relay.
pub async fn controller_sample() -> Option<ControllerSample> {
    match detect_session() {
        LinuxSessionKind::X11 => {
            let backend = X11Backend::new().ok()?;
            backend.sample().await.ok()
        }
        LinuxSessionKind::Wayland => hyprland_controller_sample(),
        LinuxSessionKind::Unknown => None,
    }
}

/// Ask the active Linux backend to perform a local pointer handoff.
pub async fn perform_local_handoff(edge: ScreenEdge) -> bool {
    match detect_session() {
        LinuxSessionKind::X11 => match X11Backend::new() {
            Ok(backend) => backend.handoff(edge).await.is_ok(),
            Err(_) => false,
        },
        LinuxSessionKind::Wayland => hyprland_place_pointer_on_entry(edge),
        LinuxSessionKind::Unknown => false,
    }
}

/// Recenter the local controller pointer to keep collecting relative motion.
pub async fn recenter_controller_pointer() -> bool {
    match detect_session() {
        LinuxSessionKind::X11 => match X11Backend::new() {
            Ok(backend) => backend.center_pointer().is_ok(),
            Err(_) => false,
        },
        LinuxSessionKind::Wayland => hyprland_recenter_pointer(),
        LinuxSessionKind::Unknown => false,
    }
}

/// Ask the active Linux target backend to place the pointer on an entry edge.
pub async fn place_pointer_on_entry(edge: ScreenEdge) -> bool {
    match detect_session() {
        LinuxSessionKind::X11 => match X11Backend::new() {
            Ok(backend) => backend.enter_from_edge(edge).is_ok(),
            Err(_) => false,
        },
        LinuxSessionKind::Wayland => hyprland_place_pointer_on_entry(edge),
        LinuxSessionKind::Unknown => false,
    }
}

/// Inject relative pointer motion on the current Linux target backend.
pub async fn inject_pointer_relative(delta_x: i16, delta_y: i16) -> bool {
    match detect_session() {
        LinuxSessionKind::X11 => match X11Backend::new() {
            Ok(backend) => backend.move_pointer_relative(delta_x, delta_y).is_ok(),
            Err(_) => false,
        },
        LinuxSessionKind::Wayland => hyprland_inject_pointer_relative(delta_x, delta_y),
        LinuxSessionKind::Unknown => false,
    }
}

/// Inject a pointer button state on the current Linux target backend.
pub async fn inject_pointer_button(button: MouseButton, pressed: bool) -> bool {
    match detect_session() {
        LinuxSessionKind::X11 => match X11Backend::new() {
            Ok(backend) => backend.set_pointer_button(button, pressed).await.is_ok(),
            Err(_) => false,
        },
        LinuxSessionKind::Wayland => hyprland_inject_pointer_button(button, pressed),
        LinuxSessionKind::Unknown => false,
    }
}

/// Inject wheel scrolling on the current Linux target backend.
pub async fn inject_pointer_scroll(horizontal_steps: i16, vertical_steps: i16) -> bool {
    match detect_session() {
        LinuxSessionKind::X11 => match X11Backend::new() {
            Ok(backend) => backend
                .scroll_pointer(horizontal_steps, vertical_steps)
                .await
                .is_ok(),
            Err(_) => false,
        },
        LinuxSessionKind::Wayland => {
            hyprland_inject_pointer_scroll(horizontal_steps, vertical_steps)
        }
        LinuxSessionKind::Unknown => false,
    }
}

/// Inject a keyboard key state on the current Linux target backend.
pub async fn inject_keyboard_key(keycode: u8, pressed: bool) -> bool {
    match detect_session() {
        LinuxSessionKind::X11 => match X11Backend::new() {
            Ok(backend) => backend.set_key(keycode, pressed).await.is_ok(),
            Err(_) => false,
        },
        LinuxSessionKind::Wayland => hyprland_inject_keyboard_key(keycode, pressed),
        LinuxSessionKind::Unknown => false,
    }
}

/// Ask the active Linux target backend to restore local focus semantics.
pub async fn focus_local_target() -> bool {
    match detect_session() {
        LinuxSessionKind::X11 => match X11Backend::new() {
            Ok(backend) => backend.focus_local().await.is_ok(),
            Err(_) => false,
        },
        LinuxSessionKind::Wayland => hyprland_recenter_pointer(),
        LinuxSessionKind::Unknown => false,
    }
}

/// Summary of Linux controller capture activation attempts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CaptureGrabReport {
    /// Devices considered for exclusive grab state.
    pub attempted_devices: usize,
    /// Devices that are now in the requested grab state.
    pub affected_devices: usize,
}

/// Attempt to grab or ungrab local controller devices for remote ownership.
pub async fn set_remote_capture_active(active: bool) -> Option<CaptureGrabReport> {
    if detect_session() != LinuxSessionKind::Wayland {
        return None;
    }

    let mut cache = lock_evdev_device_cache();
    if cache.devices.is_empty() {
        cache.rediscover_devices();
    }
    Some(cache.set_grabbed(active))
}

/// Grab the X11 pointer so local applications do not receive events during
/// a remote relay session. Returns `true` on success or if not on X11.
pub fn grab_pointer_for_remote() -> bool {
    if detect_session() != LinuxSessionKind::X11 {
        return true;
    }
    hyprbridge_platform_x11::x11_grab_pointer()
}

/// Release a previously grabbed X11 pointer.
pub fn ungrab_pointer_for_remote() {
    if detect_session() == LinuxSessionKind::X11 {
        hyprbridge_platform_x11::x11_ungrab_pointer();
    }
}

fn probe_wayland() -> CapabilityReport {
    let mut report = WaylandProbe.probe();
    let evdev = sample_evdev_controller_state();

    if evdev.readable_devices > 0 {
        report.input_capture = SupportLevel::Supported;
        report.notes.push(format!(
            "Controller-side key/button capture is enabled through {} readable /dev/input event device(s).",
            evdev.readable_devices
        ));
    } else {
        report.notes.push(
            "Full controller-side keyboard and mouse-button capture on Wayland requires readable /dev/input/event* devices. Pointer capture still uses Hyprland IPC."
                .into(),
        );
    }

    report
}

fn inspect_wayland_backend(dead_zone_px: u16) -> Option<InputBackendSnapshot> {
    let mut snapshot = inspect_hyprland_backend(dead_zone_px)?;
    let evdev = sample_evdev_controller_state();
    snapshot.controller_capture = Some(ControllerCaptureSnapshot {
        readable_devices: evdev.readable_devices,
        grabbed_devices: evdev.grabbed_devices,
        exclusive_capture_active: evdev.exclusive_capture_active,
    });
    snapshot.detail = format!(
        "{} Controller evdev devices: readable={}, grabbed={}, exclusive_capture_active={}. Hyprland target-side wheel injection is {}.",
        snapshot.detail,
        evdev.readable_devices,
        evdev.grabbed_devices,
        evdev.exclusive_capture_active,
        if snapshot.target_wheel_injection == SupportLevel::Supported {
            "available"
        } else {
            "pending"
        }
    );
    Some(snapshot)
}

fn hyprland_controller_sample() -> Option<ControllerSample> {
    let mut sample = hyprland_pointer_sample()?;
    let evdev = sample_evdev_controller_state();
    sample.buttons_mask = evdev.buttons_mask;
    sample.keymap = evdev.keymap;
    sample.scroll_horizontal_steps = evdev.scroll_horizontal_steps;
    sample.scroll_vertical_steps = evdev.scroll_vertical_steps;
    Some(sample)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct EvdevControllerState {
    buttons_mask: u16,
    keymap: [u8; 32],
    scroll_horizontal_steps: i16,
    scroll_vertical_steps: i16,
    readable_devices: usize,
    grabbed_devices: usize,
    exclusive_capture_active: bool,
}

fn sample_evdev_controller_state() -> EvdevControllerState {
    let mut cache = lock_evdev_device_cache();
    cache.refresh_if_needed();

    let mut state = cache.sample();
    if state.readable_devices == 0 && !cache.devices.is_empty() {
        cache.needs_refresh = true;
        cache.refresh_if_needed();
        state = cache.sample();
    } else if cache.needs_refresh {
        cache.rediscover_devices();
        state = cache.sample();
    }
    state
}

fn is_controller_candidate(device: &Device) -> bool {
    if is_hyprbridge_virtual_device(device.name()) {
        return false;
    }

    is_keyboard_device(device) || is_pointer_device(device)
}

fn is_hyprbridge_virtual_device(name: Option<&str>) -> bool {
    matches!(
        name,
        Some(device_name)
            if device_name == HYPRBRIDGE_VIRTUAL_DEVICE_NAME
                || device_name.starts_with("HyprBridge Virtual")
    )
}

fn is_keyboard_device(device: &Device) -> bool {
    let Some(keys) = device.supported_keys() else {
        return false;
    };

    keys.contains(KeyCode::KEY_A)
        || keys.contains(KeyCode::KEY_SPACE)
        || keys.contains(KeyCode::KEY_ENTER)
        || keys.contains(KeyCode::KEY_LEFTCTRL)
}

fn is_pointer_device(device: &Device) -> bool {
    let has_pointer_axes = device.supported_relative_axes().is_some_and(|axes| {
        axes.contains(RelativeAxisCode::REL_X) || axes.contains(RelativeAxisCode::REL_Y)
    });
    let has_pointer_buttons = device.supported_keys().is_some_and(|keys| {
        keys.contains(KeyCode::BTN_LEFT)
            || keys.contains(KeyCode::BTN_MIDDLE)
            || keys.contains(KeyCode::BTN_RIGHT)
    });

    has_pointer_axes || has_pointer_buttons
}

fn merge_pressed_keys(
    state: &mut EvdevControllerState,
    pressed_keys: impl IntoIterator<Item = KeyCode>,
) {
    for key in pressed_keys {
        match key {
            KeyCode::BTN_LEFT => state.buttons_mask |= 1 << 8,
            KeyCode::BTN_MIDDLE => state.buttons_mask |= 1 << 9,
            KeyCode::BTN_RIGHT => state.buttons_mask |= 1 << 10,
            _ => set_x11_keymap_bit(&mut state.keymap, key),
        }
    }
}

fn collect_scroll_events(state: &mut EvdevControllerState, device: &mut Device) {
    let events = match device.fetch_events() {
        Ok(events) => events,
        Err(error) if error.kind() == ErrorKind::WouldBlock => return,
        Err(_) => return,
    };

    for event in events {
        match event.destructure() {
            EventSummary::RelativeAxis(_, RelativeAxisCode::REL_WHEEL, value) => {
                state.scroll_vertical_steps = state
                    .scroll_vertical_steps
                    .saturating_add(clamp_i32_to_i16(value));
            }
            EventSummary::RelativeAxis(_, RelativeAxisCode::REL_HWHEEL, value) => {
                state.scroll_horizontal_steps = state
                    .scroll_horizontal_steps
                    .saturating_add(clamp_i32_to_i16(value));
            }
            _ => {}
        }
    }
}

fn set_x11_keymap_bit(keymap: &mut [u8; 32], key: KeyCode) {
    let Some(x11_keycode) = evdev_keycode_to_x11(key) else {
        return;
    };

    let byte_index = usize::from(x11_keycode / 8);
    let bit_index = x11_keycode % 8;
    keymap[byte_index] |= 1 << bit_index;
}

fn evdev_keycode_to_x11(key: KeyCode) -> Option<u8> {
    let raw = key.code();
    let translated = raw.checked_add(8)?;
    u8::try_from(translated).ok().filter(|code| *code >= 8)
}

fn clamp_i32_to_i16(value: i32) -> i16 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn evdev_device_cache() -> &'static Mutex<EvdevDeviceCache> {
    static CACHE: OnceLock<Mutex<EvdevDeviceCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(EvdevDeviceCache::default()))
}

fn lock_evdev_device_cache() -> MutexGuard<'static, EvdevDeviceCache> {
    match evdev_device_cache().lock() {
        Ok(cache) => cache,
        Err(poisoned) => {
            let mut cache = poisoned.into_inner();
            // Recover from a poisoned cache by forcing a clean rediscovery on the next use.
            cache.paths.clear();
            cache.devices.clear();
            cache.needs_refresh = true;
            cache.last_refresh_at = None;
            cache
        }
    }
}

#[derive(Debug, Default)]
struct EvdevDeviceCache {
    paths: Vec<PathBuf>,
    devices: Vec<Device>,
    grab_active: bool,
    needs_refresh: bool,
    last_refresh_at: Option<Instant>,
}

impl EvdevDeviceCache {
    fn refresh_if_needed(&mut self) {
        const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

        let stale = self
            .last_refresh_at
            .is_none_or(|last| last.elapsed() >= DEVICE_REFRESH_INTERVAL);
        if self.devices.is_empty() || self.needs_refresh || stale {
            self.rediscover_devices();
        }
    }

    fn rediscover_devices(&mut self) {
        self.paths.clear();
        self.devices.clear();

        for (path, mut device) in enumerate() {
            if !is_controller_candidate(&device) {
                continue;
            }
            if device.set_nonblocking(true).is_err() {
                continue;
            }
            if self.grab_active {
                let _ = device.grab();
            }
            self.paths.push(path);
            self.devices.push(device);
        }
        self.needs_refresh = false;
        self.last_refresh_at = Some(Instant::now());
    }

    fn sample(&mut self) -> EvdevControllerState {
        let mut state = EvdevControllerState {
            exclusive_capture_active: self.grab_active,
            ..Default::default()
        };

        for device in &mut self.devices {
            let Ok(pressed_keys) = device.get_key_state() else {
                self.needs_refresh = true;
                continue;
            };

            state.readable_devices += 1;
            if device.is_grabbed() {
                state.grabbed_devices += 1;
            }
            merge_pressed_keys(&mut state, pressed_keys.iter());
            collect_scroll_events(&mut state, device);
        }

        state
    }

    fn set_grabbed(&mut self, active: bool) -> CaptureGrabReport {
        let mut report = CaptureGrabReport::default();

        for device in &mut self.devices {
            report.attempted_devices += 1;
            if active {
                if device.is_grabbed() || device.grab().is_ok() {
                    report.affected_devices += 1;
                }
            } else if !device.is_grabbed() || device.ungrab().is_ok() {
                report.affected_devices += 1;
            }
        }

        self.grab_active = active;
        report
    }
}

#[cfg(test)]
mod tests {
    use evdev::KeyCode;
    use hyprbridge_protocol::LinuxSessionKind;

    use super::{
        EvdevControllerState, clamp_i32_to_i16, detect_session, is_hyprbridge_virtual_device,
        merge_pressed_keys,
    };

    #[test]
    fn session_detection_uses_explicit_env() {
        unsafe {
            std::env::set_var("XDG_SESSION_TYPE", "x11");
        }
        assert_eq!(detect_session(), LinuxSessionKind::X11);
        unsafe {
            std::env::remove_var("XDG_SESSION_TYPE");
        }
    }

    #[test]
    fn evdev_keyboard_codes_are_packed_as_x11_keymap_bits() {
        let mut state = EvdevControllerState::default();
        merge_pressed_keys(
            &mut state,
            [
                KeyCode::KEY_A,
                KeyCode::KEY_SPACE,
                KeyCode::KEY_LEFTCTRL,
                KeyCode::KEY_RESERVED,
            ],
        );

        assert_ne!(state.keymap, [0; 32]);
        assert!(state.keymap[4] & (1 << 6) != 0);
        assert!(state.keymap[8] & (1 << 1) != 0);
        assert!(state.keymap[4] & (1 << 5) != 0);
    }

    #[test]
    fn evdev_mouse_buttons_map_to_x11_pointer_mask_bits() {
        let mut state = EvdevControllerState::default();
        merge_pressed_keys(
            &mut state,
            [KeyCode::BTN_LEFT, KeyCode::BTN_MIDDLE, KeyCode::BTN_RIGHT],
        );

        assert_eq!(state.buttons_mask, (1 << 8) | (1 << 9) | (1 << 10));
        assert_eq!(state.keymap, [0; 32]);
    }

    #[test]
    fn wheel_delta_clamps_to_i16_range() {
        assert_eq!(clamp_i32_to_i16(i32::MAX), i16::MAX);
        assert_eq!(clamp_i32_to_i16(i32::MIN), i16::MIN);
    }

    #[test]
    fn hyprbridge_virtual_devices_are_excluded_from_controller_capture() {
        assert!(is_hyprbridge_virtual_device(Some(
            "HyprBridge Virtual KVM"
        )));
        assert!(is_hyprbridge_virtual_device(Some(
            "HyprBridge Virtual Auxiliary Device"
        )));
        assert!(!is_hyprbridge_virtual_device(Some(
            "AT Translated Set 2 keyboard"
        )));
        assert!(!is_hyprbridge_virtual_device(None));
    }
}
