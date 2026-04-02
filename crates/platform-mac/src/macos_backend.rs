//! macOS native backend using Core Graphics event APIs.

use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventField, CGEventSource, CGEventSourceStateID, CGEventType, CGEventTap,
    CGEventTapCallBackFn, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
};
use core_graphics::event::{CGMouseButton, ScrollEventUnit};
use core_graphics::geometry::CGPoint;
use hyprbridge_input::{
    BackendError, BoundaryHandoffBackend, CapabilityProbe, ControllerInputBackend,
    ControllerSample, TargetInjectionBackend,
};
use hyprbridge_protocol::{
    CapabilityReport, InputBackendSnapshot, InputBackendSnapshot as _, MouseButton,
    PermissionState, PointerSnapshot, ScreenEdge, SupportLevel, TargetInjectionSnapshot,
};

fn is_accessibility_trusted() -> bool {
    objc2_app_kit::NSAccessibility::isProcessTrusted()
}

#[derive(Debug, Default)]
struct MacControllerState {
    pointer: PointerSnapshot,
    buttons_mask: u16,
    keymap: [u8; 32],
    scroll_horizontal_steps: i16,
    scroll_vertical_steps: i16,
}

struct MacController {
    state: Arc<Mutex<MacControllerState>>,
    tap: Mutex<Option<CGEventTap<'static>>>,
    stop_requested: Arc<Mutex<bool>>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
}

impl MacController {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MacControllerState::default())),
            tap: Mutex::new(None),
            stop_requested: Arc::new(Mutex::new(false)),
            thread: Mutex::new(None),
        }
    }

    fn start(&self) -> Result<(), BackendError> {
        if self.tap.lock().unwrap().is_some() {
            return Ok(());
        }
        let state = self.state.clone();
        let stop_requested = self.stop_requested.clone();

        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseUp,
                CGEventType::RightMouseDown,
                CGEventType::RightMouseUp,
                CGEventType::OtherMouseDown,
                CGEventType::OtherMouseUp,
                CGEventType::MouseMoved,
                CGEventType::ScrollWheel,
            ],
            move |_proxy, event_type, event| {
                let mut s = state.lock().unwrap();
                match event_type {
                    CGEventType::LeftMouseDown => {
                        s.buttons_mask |= 1 << 8;
                    }
                    CGEventType::LeftMouseUp => {
                        s.buttons_mask &= !(1 << 8);
                    }
                    CGEventType::RightMouseDown => {
                        s.buttons_mask |= 1 << 10;
                    }
                    CGEventType::RightMouseUp => {
                        s.buttons_mask &= !(1 << 10);
                    }
                    CGEventType::OtherMouseDown => {
                        s.buttons_mask |= 1 << 9;
                    }
                    CGEventType::OtherMouseUp => {
                        s.buttons_mask &= !(1 << 9);
                    }
                    CGEventType::MouseMoved => {
                        let loc = event.location();
                        let display = CGDisplay::main();
                        s.pointer = PointerSnapshot {
                            x: loc.x as i32,
                            y: loc.y as i32,
                            width: display.pixels_wide() as u16,
                            height: display.pixels_high() as u16,
                        };
                    }
                    CGEventType::ScrollWheel => {
                        let delta_y = event
                            .get_integer_value_field(CGEventField::SCROLL_WHEEL_EVENT_DELTA_AXIS_1)
                            as i16;
                        let delta_x = event
                            .get_integer_value_field(CGEventField::SCROLL_WHEEL_EVENT_DELTA_AXIS_2)
                            as i16;
                        s.scroll_vertical_steps = s
                            .scroll_vertical_steps
                            .saturating_add(delta_y);
                        s.scroll_horizontal_steps = s
                            .scroll_horizontal_steps
                            .saturating_add(delta_x);
                    }
                    CGEventType::FlagsChanged | CGEventType::KeyDown | CGEventType::KeyUp => {
                        let keycode =
                            event.get_integer_value_field(CGEventField::KEYBOARD_EVENT_KEYCODE)
                                as u16;
                        let is_pressed = matches!(
                            event_type,
                            CGEventType::KeyDown | CGEventType::FlagsChanged
                        );
                        update_keymap(&mut s.keymap, keycode, is_pressed);
                    }
                    _ => {}
                }
                None
            },
        )
        .map_err(|_| BackendError::Other("failed to create CGEventTap".into()))?;

        let loop_source = tap
            .mach_port
            .create_runloop_source(0)
            .map_err(|_| BackendError::Other("failed to create runloop source".into()))?;

        let stop_requested = self.stop_requested.clone();
        let handle = thread::spawn(move || {
            let runloop = CFRunLoop::get_current();
            runloop.add_source(&loop_source, kCFRunLoopCommonModes);
            tap.enable();
            while !*stop_requested.lock().unwrap() {
                CFRunLoop::run_in_mode(
                    kCFRunLoopCommonModes,
                    std::time::Duration::from_millis(100),
                    false,
                );
            }
        });

        *self.tap.lock().unwrap() = Some(tap);
        *self.thread.lock().unwrap() = Some(handle);
        Ok(())
    }

    fn stop(&self) {
        *self.stop_requested.lock().unwrap() = true;
        if let Some(tap) = self.tap.lock().unwrap().take() {
            unsafe {
                use core_foundation::base::CFRelease;
                CFRelease(tap.mach_port.as_CFTypeRef() as *const _ as *const core_foundation::base::CFTypeRef);
            }
        }
        if let Some(handle) = self.thread.lock().unwrap().take() {
            let _ = handle.join();
        }
    }
}

fn update_keymap(keymap: &mut [u8; 32], cg_keycode: u16, pressed: bool) {
    let x11_keycode = (cg_keycode as u8).saturating_add(8);
    if x11_keycode < 8 {
        return;
    }
    let byte_index = (x11_keycode / 8) as usize;
    let bit_index = (x11_keycode % 8) as u8;
    if byte_index >= 32 {
        return;
    }
    if pressed {
        keymap[byte_index] |= 1u8 << bit_index;
    } else {
        keymap[byte_index] &= !(1u8 << bit_index);
    }
}

static CONTROLLER: OnceLock<MacController> = OnceLock::new();

fn controller() -> &'static MacController {
    CONTROLLER.get_or_init(MacController::new)
}

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

pub fn current_pointer_snapshot() -> Option<PointerSnapshot> {
    let event = CGEvent::new(None).ok()?;
    let location = event.location();
    let display = CGDisplay::main();
    Some(PointerSnapshot {
        x: location.x as i32,
        y: location.y as i32,
        width: display.pixels_wide() as u16,
        height: display.pixels_high() as u16,
    })
}

pub async fn macos_place_pointer_on_entry(edge: ScreenEdge) -> bool {
    let snapshot = match current_pointer_snapshot() {
        Some(s) => s,
        None => return false,
    };
    let (x, y) = match edge {
        ScreenEdge::Left => (10.0, (snapshot.height / 2) as f64),
        ScreenEdge::Right => ((snapshot.width - 10) as f64, (snapshot.height / 2) as f64),
        ScreenEdge::Top => ((snapshot.width / 2) as f64, 10.0),
        ScreenEdge::Bottom => ((snapshot.width / 2) as f64, (snapshot.height - 10) as f64),
    };
    warp_pointer_to(x, y)
}

fn warp_pointer_to(x: f64, y: f64) -> bool {
    let point = CGPoint::new(x, y);
    if let Ok(event) = CGEvent::new_mouse_event(
        None,
        CGEventType::MouseMoved,
        point,
        CGMouseButton::Left,
    ) {
        event.post(CGEventTapLocation::HID);
        true
    } else {
        false
    }
}

pub async fn macos_recenter_controller_pointer() -> bool {
    let snapshot = match current_pointer_snapshot() {
        Some(s) => s,
        None => return false,
    };
    let cx = (snapshot.width / 2) as f64;
    let cy = (snapshot.height / 2) as f64;
    warp_pointer_to(cx, cy)
}

pub async fn macos_inject_pointer_relative(delta_x: i16, delta_y: i16) -> bool {
    let snapshot = match current_pointer_snapshot() {
        Some(s) => s,
        None => return false,
    };
    let new_x = (snapshot.x as f64) + (delta_x as f64);
    let new_y = (snapshot.y as f64) + (delta_y as f64);
    warp_pointer_to(new_x, new_y)
}

pub async fn macos_inject_pointer_button(button: MouseButton, pressed: bool) -> bool {
    let location = match current_pointer_snapshot() {
        Some(s) => CGPoint::new(s.x as f64, s.y as f64),
        None => return false,
    };
    let (event_type, cg_button) = match button {
        MouseButton::Left => (
            if pressed {
                CGEventType::LeftMouseDown
            } else {
                CGEventType::LeftMouseUp
            },
            CGMouseButton::Left,
        ),
        MouseButton::Right => (
            if pressed {
                CGEventType::RightMouseDown
            } else {
                CGEventType::RightMouseUp
            },
            CGMouseButton::Right,
        ),
        MouseButton::Middle => (
            if pressed {
                CGEventType::OtherMouseDown
            } else {
                CGEventType::OtherMouseUp
            },
            CGMouseButton::Center,
        ),
        MouseButton::Other(_) => (
            if pressed {
                CGEventType::OtherMouseDown
            } else {
                CGEventType::OtherMouseUp
            },
            CGMouseButton::Center,
        ),
    };
    if let Ok(event) = CGEvent::new_mouse_event(None, event_type, location, cg_button) {
        event.post(CGEventTapLocation::HID);
        true
    } else {
        false
    }
}

pub async fn macos_inject_pointer_scroll(horizontal: i16, vertical: i16) -> bool {
    if let Ok(event) = CGEvent::new_scroll_event(
        None,
        ScrollEventUnit::Line,
        2,
        vertical as i32,
        horizontal as i32,
        0,
        0,
    ) {
        event.post(CGEventTapLocation::HID);
        true
    } else {
        false
    }
}

pub async fn macos_inject_keyboard_key(keycode: u8, pressed: bool) -> bool {
    let cg_keycode = keycode.saturating_sub(8) as u16;
    if let Ok(event) = CGEvent::new_keyboard_event(None, cg_keycode, pressed) {
        event.post(CGEventTapLocation::HID);
        true
    } else {
        false
    }
}

pub async fn macos_focus_local_target() -> bool {
    macos_recenter_controller_pointer().await
}

pub async fn macos_controller_sample() -> Option<ControllerSample> {
    let ctrl = controller();
    let mut s = ctrl.state.lock().unwrap();
    let pointer = current_pointer_snapshot().unwrap_or_else(|| PointerSnapshot {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    });
    if pointer.width > 0 {
        s.pointer = pointer;
    }
    Some(ControllerSample {
        pointer: s.pointer,
        buttons_mask: s.buttons_mask,
        keymap: s.keymap,
        scroll_horizontal_steps: s.scroll_horizontal_steps,
        scroll_vertical_steps: s.scroll_vertical_steps,
    })
}

pub async fn macos_set_remote_capture_active(active: bool) -> Option<()> {
    let ctrl = controller();
    if active {
        ctrl.start().ok()?;
    } else {
        ctrl.stop();
    }
    Some(())
}

pub async fn macos_inspect_input_backend(dead_zone_px: u16) -> Option<InputBackendSnapshot> {
    let snapshot = current_pointer_snapshot();
    let edge = snapshot.as_ref().and_then(|p| detect_edge(p, dead_zone_px));
    let trusted = is_accessibility_trusted();
    Some(InputBackendSnapshot {
        backend: "macos-cgevent".into(),
        available: trusted,
        pointer: snapshot,
        edge_candidate: edge,
        controller_capture: None,
        target_injection: Some(TargetInjectionSnapshot {
            primary_backend: "macos-cgevent".into(),
            fallback_backend: None,
            uinput_writable: false,
            last_error: None,
        }),
        target_wheel_injection: SupportLevel::Supported,
        detail: "macOS Core Graphics event backend".into(),
    })
}

fn detect_edge(pointer: &PointerSnapshot, threshold: u16) -> Option<ScreenEdge> {
    let t = i32::from(threshold);
    if pointer.x <= t {
        return Some(ScreenEdge::Left);
    }
    if pointer.x >= i32::from(pointer.width) - t {
        return Some(ScreenEdge::Right);
    }
    if pointer.y <= t {
        return Some(ScreenEdge::Top);
    }
    if pointer.y >= i32::from(pointer.height) - t {
        return Some(ScreenEdge::Bottom);
    }
    None
}

pub async fn macos_perform_local_handoff(edge: ScreenEdge) -> bool {
    macos_place_pointer_on_entry(edge).await
}

#[derive(Debug, Default)]
pub struct MacInputBackend {
    controller: Arc<MacController>,
}

impl MacInputBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn controller(&self) -> &Arc<MacController> {
        &self.controller
    }
}

#[async_trait::async_trait]
impl ControllerInputBackend for MacInputBackend {
    async fn start(&self) -> Result<(), BackendError> {
        self.controller.start()
    }

    async fn stop(&self) -> Result<(), BackendError> {
        self.controller.stop();
        Ok(())
    }

    async fn pointer_snapshot(&self) -> Result<PointerSnapshot, BackendError> {
        current_pointer_snapshot().ok_or_else(|| {
            BackendError::Other("failed to read pointer position".into())
        })
    }

    async fn sample(&self) -> Result<ControllerSample, BackendError> {
        macos_controller_sample().await.ok_or_else(|| {
            BackendError::Other("controller sample unavailable".into())
        })
    }

    async fn edge_candidate(&self, threshold_px: u16) -> Result<Option<ScreenEdge>, BackendError> {
        let pointer = self.pointer_snapshot()?;
        Ok(detect_edge(&pointer, threshold_px))
    }

    async fn snapshot(
        &self,
        threshold_px: u16,
    ) -> Result<hyprbridge_protocol::InputBackendSnapshot, BackendError> {
        let pointer = self.pointer_snapshot()?;
        let edge = detect_edge(&pointer, threshold_px);
        Ok(InputBackendSnapshot {
            backend: "macos-cgevent".into(),
            available: true,
            pointer: Some(pointer),
            edge_candidate: edge,
            controller_capture: None,
            target_injection: Some(TargetInjectionSnapshot {
                primary_backend: "macos-cgevent".into(),
                fallback_backend: None,
                uinput_writable: false,
                last_error: None,
            }),
            target_wheel_injection: SupportLevel::Supported,
            detail: "macOS Core Graphics event backend".into(),
        })
    }
}

#[async_trait::async_trait]
impl TargetInjectionBackend for MacInputBackend {
    async fn focus_remote(&self, _device: &hyprbridge_protocol::DeviceId) -> Result<(), BackendError> {
        macos_place_pointer_on_entry(ScreenEdge::Left)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("pointer placement failed".into()))
    }

    async fn focus_local(&self) -> Result<(), BackendError> {
        macos_recenter_controller_pointer()
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("pointer recenter failed".into()))
    }

    async fn move_pointer_relative(
        &self,
        delta_x: i16,
        delta_y: i16,
    ) -> Result<(), BackendError> {
        macos_inject_pointer_relative(delta_x, delta_y)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("pointer move failed".into()))
    }

    async fn set_pointer_button(
        &self,
        button: MouseButton,
        pressed: bool,
    ) -> Result<(), BackendError> {
        macos_inject_pointer_button(button, pressed)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("pointer button failed".into()))
    }

    async fn scroll_pointer(
        &self,
        horizontal_steps: i16,
        vertical_steps: i16,
    ) -> Result<(), BackendError> {
        macos_inject_pointer_scroll(horizontal_steps, vertical_steps)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("scroll failed".into()))
    }

    async fn set_key(&self, keycode: u8, pressed: bool) -> Result<(), BackendError> {
        macos_inject_keyboard_key(keycode, pressed)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("keyboard key failed".into()))
    }
}

#[async_trait::async_trait]
impl BoundaryHandoffBackend for MacInputBackend {
    async fn handoff(&self, edge: ScreenEdge) -> Result<(), BackendError> {
        macos_place_pointer_on_entry(edge)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("handoff failed".into()))
    }
}
