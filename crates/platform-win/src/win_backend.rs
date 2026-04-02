//! Windows native backend using Win32 low-level hooks and SendInput.

use std::sync::{Arc, Mutex, OnceLock};

use hyprbridge_input::{
    BackendError, BoundaryHandoffBackend, CapabilityProbe, ControllerInputBackend,
    ControllerSample, TargetInjectionBackend,
};
use hyprbridge_protocol::{
    CapabilityReport, InputBackendSnapshot, MouseButton, PermissionState, PointerSnapshot,
    ScreenEdge, SupportLevel, TargetInjectionSnapshot,
};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetAsyncKeyState, GetCursorPos, GetSystemMetrics, SetCursorPos, SM_CXSCREEN, SM_CYSCREEN,
};

fn detect_session() -> &'static str {
    "console"
}

fn screen_size() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

#[derive(Debug, Default)]
struct WinControllerState {
    buttons_mask: u16,
    keymap: [u8; 32],
}

struct WinController {
    state: Arc<Mutex<WinControllerState>>,
}

impl WinController {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(WinControllerState::default())),
        }
    }

    fn sample(&self) -> ControllerSample {
        let mut point = POINT::default();
        let (width, height) = if unsafe { GetCursorPos(&mut point).is_ok() } {
            let (w, h) = screen_size();
            (w as u16, h as u16)
        } else {
            (1920, 1080)
        };

        let mut state = self.state.lock().unwrap();
        let mut keymap = [0u8; 32];
        let mut buttons_mask = 0u16;

        unsafe {
            if GetAsyncKeyState(VK_LBUTTON.0 as i32) & 0x8000 != 0 {
                buttons_mask |= 1 << 8;
            }
            if GetAsyncKeyState(VK_MBUTTON.0 as i32) & 0x8000 != 0 {
                buttons_mask |= 1 << 9;
            }
            if GetAsyncKeyState(VK_RBUTTON.0 as i32) & 0x8000 != 0 {
                buttons_mask |= 1 << 10;
            }

            for vk in 0u16..256 {
                let async_state = GetAsyncKeyState(vk as i32);
                let is_pressed = async_state & 0x8000 != 0;
                let x11_keycode = vk.saturating_add(8) as u8;
                if x11_keycode < 8 {
                    continue;
                }
                let byte_index = (x11_keycode / 8) as usize;
                let bit_index = (x11_keycode % 8) as u8;
                if byte_index < 32 {
                    if is_pressed {
                        keymap[byte_index] |= 1u8 << bit_index;
                    } else {
                        keymap[byte_index] &= !(1u8 << bit_index);
                    }
                }
            }
        }

        state.buttons_mask = buttons_mask;
        state.keymap = keymap;

        ControllerSample {
            pointer: PointerSnapshot {
                x: point.x,
                y: point.y,
                width,
                height,
            },
            buttons_mask,
            keymap,
            scroll_horizontal_steps: 0,
            scroll_vertical_steps: 0,
        }
    }
}

static CONTROLLER: OnceLock<WinController> = OnceLock::new();

fn controller() -> &'static WinController {
    CONTROLLER.get_or_init(WinController::new)
}

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

pub async fn win_recenter_controller_pointer() -> bool {
    let (w, h) = screen_size();
    unsafe { SetCursorPos(w / 2, h / 2).is_ok() }
}

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

pub async fn win_inject_pointer_button(button: MouseButton, pressed: bool) -> bool {
    let flag = match (button, pressed) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        (MouseButton::Other(_), true) => MOUSEEVENTF_XDOWN,
        (MouseButton::Other(_), false) => MOUSEEVENTF_XUP,
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

pub async fn win_inject_pointer_scroll(horizontal: i16, vertical: i16) -> bool {
    let mut success = true;
    if vertical != 0 {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
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

pub async fn win_inject_keyboard_key(keycode: u8, pressed: bool) -> bool {
    let vk = keycode.saturating_sub(8) as u16;
    let flags = if pressed { KEYEVENTF_KEYDOWN } else { KEYEVENTF_KEYUP };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) == 1 }
}

pub async fn win_focus_local_target() -> bool {
    win_recenter_controller_pointer().await
}

pub async fn win_controller_sample() -> Option<ControllerSample> {
    Some(controller().sample())
}

pub async fn win_set_remote_capture_active(_active: bool) -> Option<()> {
    Some(())
}

pub async fn win_inspect_input_backend(dead_zone_px: u16) -> Option<InputBackendSnapshot> {
    let pointer = current_pointer_snapshot();
    let edge = pointer.as_ref().and_then(|p| detect_edge(p, dead_zone_px));
    Some(InputBackendSnapshot {
        backend: "windows-sendinput".into(),
        available: true,
        pointer,
        edge_candidate: edge,
        controller_capture: None,
        target_injection: Some(TargetInjectionSnapshot {
            primary_backend: "windows-sendinput".into(),
            fallback_backend: None,
            uinput_writable: false,
            last_error: None,
        }),
        target_wheel_injection: SupportLevel::Supported,
        detail: "Windows SendInput backend".into(),
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

pub async fn win_perform_local_handoff(edge: ScreenEdge) -> bool {
    win_place_pointer_on_entry(edge).await
}

#[derive(Debug, Default)]
pub struct WinInputBackend {
    _private: (),
}

impl WinInputBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl ControllerInputBackend for WinInputBackend {
    async fn start(&self) -> Result<(), BackendError> {
        Ok(())
    }

    async fn stop(&self) -> Result<(), BackendError> {
        Ok(())
    }

    async fn pointer_snapshot(&self) -> Result<PointerSnapshot, BackendError> {
        current_pointer_snapshot().ok_or_else(|| {
            BackendError::Other("failed to read pointer position".into())
        })
    }

    async fn sample(&self) -> Result<ControllerSample, BackendError> {
        win_controller_sample().await.ok_or_else(|| {
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
            backend: "windows-sendinput".into(),
            available: true,
            pointer: Some(pointer),
            edge_candidate: edge,
            controller_capture: None,
            target_injection: Some(TargetInjectionSnapshot {
                primary_backend: "windows-sendinput".into(),
                fallback_backend: None,
                uinput_writable: false,
                last_error: None,
            }),
            target_wheel_injection: SupportLevel::Supported,
            detail: "Windows SendInput backend".into(),
        })
    }
}

#[async_trait::async_trait]
impl TargetInjectionBackend for WinInputBackend {
    async fn focus_remote(&self, _device: &hyprbridge_protocol::DeviceId) -> Result<(), BackendError> {
        win_place_pointer_on_entry(ScreenEdge::Left)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("pointer placement failed".into()))
    }

    async fn focus_local(&self) -> Result<(), BackendError> {
        win_recenter_controller_pointer()
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("pointer recenter failed".into()))
    }

    async fn move_pointer_relative(
        &self,
        delta_x: i16,
        delta_y: i16,
    ) -> Result<(), BackendError> {
        win_inject_pointer_relative(delta_x, delta_y)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("pointer move failed".into()))
    }

    async fn set_pointer_button(
        &self,
        button: MouseButton,
        pressed: bool,
    ) -> Result<(), BackendError> {
        win_inject_pointer_button(button, pressed)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("pointer button failed".into()))
    }

    async fn scroll_pointer(
        &self,
        horizontal_steps: i16,
        vertical_steps: i16,
    ) -> Result<(), BackendError> {
        win_inject_pointer_scroll(horizontal_steps, vertical_steps)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("scroll failed".into()))
    }

    async fn set_key(&self, keycode: u8, pressed: bool) -> Result<(), BackendError> {
        win_inject_keyboard_key(keycode, pressed)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("keyboard key failed".into()))
    }
}

#[async_trait::async_trait]
impl BoundaryHandoffBackend for WinInputBackend {
    async fn handoff(&self, edge: ScreenEdge) -> Result<(), BackendError> {
        win_place_pointer_on_entry(edge)
            .await
            .then_some(())
            .ok_or_else(|| BackendError::Other("handoff failed".into()))
    }
}
