//! X11 capability model and native backend helpers.

use std::sync::Mutex;

use async_trait::async_trait;
use hyprbridge_input::{
    BackendError, BoundaryHandoffBackend, CapabilityProbe, ControllerInputBackend,
    ControllerSample, PermissionGuide, TargetInjectionBackend,
};
use hyprbridge_protocol::{
    CapabilityReport, DeviceId, InputBackendSnapshot, LinuxSessionKind, MouseButton,
    PermissionState, PointerSnapshot, ScreenEdge, SupportLevel, WaylandSupportLevel,
};
use x11rb::{
    NONE,
    connection::Connection,
    protocol::{
        xproto::{
            BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ConnectionExt, EventMask, GrabMode,
            KEY_PRESS_EVENT, KEY_RELEASE_EVENT, KeyButMask, MOTION_NOTIFY_EVENT, Screen,
        },
        xtest::ConnectionExt as XTestConnectionExt,
    },
    rust_connection::RustConnection,
};

/// X11 platform probe.
#[derive(Debug, Default)]
pub struct X11Probe;

impl CapabilityProbe for X11Probe {
    fn probe(&self) -> CapabilityReport {
        match X11Backend::new() {
            Ok(backend) => match backend.pointer_snapshot_blocking() {
                Ok(pointer) => CapabilityReport {
                    platform: "linux".into(),
                    linux_session: LinuxSessionKind::X11,
                    wayland_support: WaylandSupportLevel::Unsupported,
                    input_capture: SupportLevel::Supported,
                    input_injection: SupportLevel::Supported,
                    cursor_handoff: SupportLevel::Supported,
                    permission_state: PermissionState::Granted,
                    notes: vec![format!(
                        "Connected to X11 display `{}` with root extent {}x{}.",
                        backend.display_name(),
                        pointer.width,
                        pointer.height
                    )],
                },
                Err(error) => degraded_x11_report(error),
            },
            Err(error) => degraded_x11_report(error),
        }
    }
}

/// Concrete X11 backend used for controller polling and target pointer placement.
#[derive(Debug, Clone)]
pub struct X11Backend {
    display_name: String,
}

impl X11Backend {
    /// Construct the backend from the local X11 environment.
    pub fn new() -> Result<Self, BackendError> {
        let display_name = std::env::var("DISPLAY").map_err(|_| {
            BackendError::MissingPermission("DISPLAY is not set for an X11 session".into())
        })?;
        Ok(Self { display_name })
    }

    /// Inspect the backend without going through async traits.
    pub fn inspect(&self, threshold_px: u16) -> Result<InputBackendSnapshot, BackendError> {
        let pointer = self.controller_sample_blocking()?.pointer;
        let edge_candidate = edge_candidate_for(&pointer, threshold_px);
        Ok(InputBackendSnapshot {
            backend: "x11".into(),
            available: true,
            pointer: Some(pointer),
            edge_candidate,
            controller_capture: None,
            target_injection: Some(hyprbridge_protocol::TargetInjectionSnapshot {
                primary_backend: "x11-xtest".into(),
                fallback_backend: None,
                uinput_writable: false,
                last_error: None,
            }),
            target_wheel_injection: SupportLevel::Supported,
            detail: format!("Connected to X11 display `{}`.", self.display_name),
        })
    }

    /// Return the display name used by this backend.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    fn pointer_snapshot_blocking(&self) -> Result<PointerSnapshot, BackendError> {
        Ok(self.controller_sample_blocking()?.pointer)
    }

    /// Capture pointer, button, and keymap state in one roundtrip set.
    pub fn controller_sample_blocking(&self) -> Result<ControllerSample, BackendError> {
        let (connection, screen) = self.connect()?;
        let root = screen.root;
        let pointer = connection
            .query_pointer(root)
            .map_err(x11_error)?
            .reply()
            .map_err(x11_error)?;
        let keymap = connection
            .query_keymap()
            .map_err(x11_error)?
            .reply()
            .map_err(x11_error)?;

        Ok(ControllerSample {
            pointer: PointerSnapshot {
                x: i32::from(pointer.root_x),
                y: i32::from(pointer.root_y),
                width: screen.width_in_pixels,
                height: screen.height_in_pixels,
            },
            buttons_mask: u16::from(pointer.mask),
            keymap: keymap.keys,
            scroll_horizontal_steps: 0,
            scroll_vertical_steps: 0,
        })
    }

    fn warp_pointer(&self, x: i16, y: i16) -> Result<(), BackendError> {
        let (connection, screen) = self.connect()?;
        connection
            .warp_pointer(NONE, screen.root, 0, 0, 0, 0, x, y)
            .map_err(x11_error)?
            .check()
            .map_err(x11_error)?;
        connection.flush().map_err(x11_error)?;
        Ok(())
    }

    fn xtest_pointer_absolute(&self, x: i16, y: i16) -> Result<(), BackendError> {
        let (connection, screen) = self.connect()?;
        ensure_xtest(&connection)?;
        connection
            .xtest_fake_input(MOTION_NOTIFY_EVENT, 0, 0, screen.root, x, y, 0)
            .map_err(x11_error)?
            .check()
            .map_err(x11_error)?;
        connection.flush().map_err(x11_error)?;
        Ok(())
    }

    fn xtest_button(&self, button: MouseButton, pressed: bool) -> Result<(), BackendError> {
        let detail = encode_mouse_button(button)?;
        let (connection, screen) = self.connect()?;
        ensure_xtest(&connection)?;
        connection
            .xtest_fake_input(
                if pressed {
                    BUTTON_PRESS_EVENT
                } else {
                    BUTTON_RELEASE_EVENT
                },
                detail,
                0,
                screen.root,
                0,
                0,
                0,
            )
            .map_err(x11_error)?
            .check()
            .map_err(x11_error)?;
        connection.flush().map_err(x11_error)?;
        Ok(())
    }

    fn xtest_scroll_steps(
        &self,
        horizontal_steps: i16,
        vertical_steps: i16,
    ) -> Result<(), BackendError> {
        relay_scroll_axis(self, vertical_steps, 4, 5)?;
        relay_scroll_axis(self, horizontal_steps, 7, 6)?;
        Ok(())
    }

    fn xtest_key(&self, keycode: u8, pressed: bool) -> Result<(), BackendError> {
        let (connection, screen) = self.connect()?;
        ensure_xtest(&connection)?;
        connection
            .xtest_fake_input(
                if pressed {
                    KEY_PRESS_EVENT
                } else {
                    KEY_RELEASE_EVENT
                },
                keycode,
                0,
                screen.root,
                0,
                0,
                0,
            )
            .map_err(x11_error)?
            .check()
            .map_err(x11_error)?;
        connection.flush().map_err(x11_error)?;
        Ok(())
    }

    /// Place the pointer just inside a remote entry edge.
    pub fn enter_from_edge(&self, edge: ScreenEdge) -> Result<(), BackendError> {
        let pointer = self.pointer_snapshot_blocking()?;
        let center_x = center_coordinate(pointer.width);
        let center_y = center_coordinate(pointer.height);
        let target = match edge {
            ScreenEdge::Left => (1_i16, center_y),
            ScreenEdge::Right => (
                saturating_dim_to_i16(pointer.width.saturating_sub(2)),
                center_y,
            ),
            ScreenEdge::Top => (center_x, 1_i16),
            ScreenEdge::Bottom => (
                center_x,
                saturating_dim_to_i16(pointer.height.saturating_sub(2)),
            ),
        };
        self.warp_pointer(target.0, target.1)
    }

    /// Center the local pointer so relative relay can keep using local motion deltas.
    pub fn center_pointer(&self) -> Result<(), BackendError> {
        let pointer = self.pointer_snapshot_blocking()?;
        self.warp_pointer(
            center_coordinate(pointer.width),
            center_coordinate(pointer.height),
        )
    }

    /// Move the local pointer relatively via XTEST motion injection.
    pub fn move_pointer_relative(&self, delta_x: i16, delta_y: i16) -> Result<(), BackendError> {
        let pointer = self.pointer_snapshot_blocking()?;
        let target_x = clamp_pointer_axis(pointer.x + i32::from(delta_x), pointer.width);
        let target_y = clamp_pointer_axis(pointer.y + i32::from(delta_y), pointer.height);
        self.xtest_pointer_absolute(target_x, target_y)
    }

    fn connect(&self) -> Result<(RustConnection, Screen), BackendError> {
        let (connection, screen_num) =
            RustConnection::connect(Some(self.display_name.as_str())).map_err(x11_error)?;
        let screen = connection
            .setup()
            .roots
            .get(screen_num)
            .cloned()
            .ok_or_else(|| BackendError::Other("X11 root screen was not available".into()))?;
        Ok((connection, screen))
    }
}

#[async_trait]
impl ControllerInputBackend for X11Backend {
    async fn start(&self) -> Result<(), BackendError> {
        self.connect().map(|_| ())
    }

    async fn stop(&self) -> Result<(), BackendError> {
        Ok(())
    }

    async fn pointer_snapshot(&self) -> Result<PointerSnapshot, BackendError> {
        self.pointer_snapshot_blocking()
    }

    async fn sample(&self) -> Result<ControllerSample, BackendError> {
        self.controller_sample_blocking()
    }

    async fn edge_candidate(&self, threshold_px: u16) -> Result<Option<ScreenEdge>, BackendError> {
        Ok(edge_candidate_for(
            &self.pointer_snapshot_blocking()?,
            threshold_px,
        ))
    }

    async fn snapshot(&self, threshold_px: u16) -> Result<InputBackendSnapshot, BackendError> {
        self.inspect(threshold_px)
    }
}

#[async_trait]
impl TargetInjectionBackend for X11Backend {
    async fn focus_remote(&self, _device: &DeviceId) -> Result<(), BackendError> {
        self.enter_from_edge(ScreenEdge::Left)
    }

    async fn focus_local(&self) -> Result<(), BackendError> {
        self.center_pointer()
    }

    async fn move_pointer_relative(&self, delta_x: i16, delta_y: i16) -> Result<(), BackendError> {
        self.move_pointer_relative(delta_x, delta_y)
    }

    async fn set_pointer_button(
        &self,
        button: MouseButton,
        pressed: bool,
    ) -> Result<(), BackendError> {
        self.xtest_button(button, pressed)
    }

    async fn scroll_pointer(
        &self,
        horizontal_steps: i16,
        vertical_steps: i16,
    ) -> Result<(), BackendError> {
        self.xtest_scroll_steps(horizontal_steps, vertical_steps)
    }

    async fn set_key(&self, keycode: u8, pressed: bool) -> Result<(), BackendError> {
        self.xtest_key(keycode, pressed)
    }
}

#[async_trait]
impl BoundaryHandoffBackend for X11Backend {
    async fn handoff(&self, edge: ScreenEdge) -> Result<(), BackendError> {
        let pointer = self.pointer_snapshot_blocking()?;
        let target = match edge {
            ScreenEdge::Left => (1_i16, clamp_i16(pointer.y)),
            ScreenEdge::Right => (
                saturating_dim_to_i16(pointer.width.saturating_sub(2)),
                clamp_i16(pointer.y),
            ),
            ScreenEdge::Top => (clamp_i16(pointer.x), 1_i16),
            ScreenEdge::Bottom => (
                clamp_i16(pointer.x),
                saturating_dim_to_i16(pointer.height.saturating_sub(2)),
            ),
        };
        self.warp_pointer(target.0, target.1)
    }
}

/// User-facing permission guidance for X11.
#[derive(Debug, Default)]
pub struct X11PermissionGuide;

impl PermissionGuide for X11PermissionGuide {
    fn guidance(&self) -> Vec<String> {
        vec![
            "Ensure the process can connect to the target X11 display.".into(),
            "If running under sudo or another user, forward or authorize DISPLAY/XAUTHORITY explicitly."
                .into(),
        ]
    }
}

fn degraded_x11_report(error: BackendError) -> CapabilityReport {
    CapabilityReport {
        platform: "linux".into(),
        linux_session: LinuxSessionKind::X11,
        wayland_support: WaylandSupportLevel::Unsupported,
        input_capture: SupportLevel::Unsupported,
        input_injection: SupportLevel::Unsupported,
        cursor_handoff: SupportLevel::Unsupported,
        permission_state: match error {
            BackendError::MissingPermission(_) => PermissionState::Missing,
            _ => PermissionState::Unknown,
        },
        notes: vec![error.to_string()],
    }
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

fn center_coordinate(dimension: u16) -> i16 {
    saturating_dim_to_i16(dimension / 2)
}

fn clamp_pointer_axis(value: i32, dimension: u16) -> i16 {
    let max = i32::from(dimension.saturating_sub(2));
    value.clamp(1, max.max(1)) as i16
}

fn saturating_dim_to_i16(value: u16) -> i16 {
    i16::try_from(value).unwrap_or(i16::MAX)
}

fn clamp_i16(value: i32) -> i16 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn x11_error(error: impl std::fmt::Display) -> BackendError {
    BackendError::Other(error.to_string())
}

fn ensure_xtest(connection: &RustConnection) -> Result<(), BackendError> {
    connection
        .xtest_get_version(2, 2)
        .map_err(x11_error)?
        .reply()
        .map_err(x11_error)?;
    Ok(())
}

fn encode_mouse_button(button: MouseButton) -> Result<u8, BackendError> {
    match button {
        MouseButton::Left => Ok(1),
        MouseButton::Middle => Ok(2),
        MouseButton::Right => Ok(3),
        MouseButton::Other(code) if code > 0 => Ok(code),
        MouseButton::Other(_) => Err(BackendError::Other(
            "mouse button code 0 is invalid for X11 injection".into(),
        )),
    }
}

fn relay_scroll_axis(
    backend: &X11Backend,
    steps: i16,
    positive_button: u8,
    negative_button: u8,
) -> Result<(), BackendError> {
    if steps == 0 {
        return Ok(());
    }

    let button = if steps > 0 {
        positive_button
    } else {
        negative_button
    };
    for _ in 0..usize::from(steps.unsigned_abs()) {
        backend.xtest_button(MouseButton::Other(button), true)?;
        backend.xtest_button(MouseButton::Other(button), false)?;
    }
    Ok(())
}

/// Read the currently pressed controller-side pointer buttons from X11.
pub fn controller_buttons(mask: u16) -> [bool; 5] {
    let state = KeyButMask::from(mask);
    [
        contains_mask(state, KeyButMask::BUTTON1),
        contains_mask(state, KeyButMask::BUTTON2),
        contains_mask(state, KeyButMask::BUTTON3),
        contains_mask(state, KeyButMask::BUTTON4),
        contains_mask(state, KeyButMask::BUTTON5),
    ]
}

fn contains_mask(mask: KeyButMask, bit: KeyButMask) -> bool {
    u16::from(mask) & u16::from(bit) != 0
}

/// State for the X11 pointer grab used during remote relay.
static POINTER_GRAB: Mutex<Option<X11Backend>> = Mutex::new(None);

/// Grab the pointer on the X11 root window so local applications do not
/// receive motion/button events while a remote session is active.
pub fn x11_grab_pointer() -> bool {
    let backend = match X11Backend::new() {
        Ok(b) => b,
        Err(_) => return false,
    };
    let (connection, screen) = match backend.connect() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let root = screen.root;
    // owner_events=false: local client does not receive events during grab.
    // pointer_mode=AsyncPointer: pointer continues to move freely.
    // keyboard_mode=AsyncKeyboard: keyboard continues to work.
    let result = connection
        .grab_pointer(
            false,
            root,
            EventMask::POINTER_MOTION | EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            NONE,
            NONE,
            x11rb::CURRENT_TIME,
        )
        .map(|c| c.reply())
        .map_err(|_| ());
    match result {
        Ok(_) => {
            let _ = POINTER_GRAB.lock().map(|mut g| *g = Some(backend));
            true
        }
        Err(_) => false,
    }
}

/// Release a previously grabbed X11 pointer.
pub fn x11_ungrab_pointer() -> bool {
    let result = (|| {
        let backend = POINTER_GRAB.lock().ok()?.clone()?;
        let (connection, _) = backend.connect().ok()?;
        connection
            .ungrab_pointer(x11rb::CURRENT_TIME)
            .ok()?
            .check()
            .ok()?;
        Some(())
    })();
    if result.is_some() {
        let _ = POINTER_GRAB.lock().map(|mut g| *g = None);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use hyprbridge_protocol::{PointerSnapshot, ScreenEdge};

    use super::{center_coordinate, edge_candidate_for, saturating_dim_to_i16};

    #[test]
    fn edge_detection_prefers_left_and_top_before_far_edges() {
        let pointer = PointerSnapshot {
            x: 3,
            y: 40,
            width: 1920,
            height: 1080,
        };

        assert_eq!(edge_candidate_for(&pointer, 6), Some(ScreenEdge::Left));
    }

    #[test]
    fn edge_detection_finds_bottom_and_right() {
        let pointer = PointerSnapshot {
            x: 1918,
            y: 1077,
            width: 1920,
            height: 1080,
        };

        assert_eq!(edge_candidate_for(&pointer, 4), Some(ScreenEdge::Right));
    }

    #[test]
    fn center_coordinate_is_stable() {
        assert_eq!(center_coordinate(1920), 960);
        assert_eq!(saturating_dim_to_i16(u16::MAX), i16::MAX);
    }
}
