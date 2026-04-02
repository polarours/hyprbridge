//! Virtual uinput device used to inject remapped events on Wayland.

use anyhow::{Context, Result};
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AttributeSet, InputEvent, KeyCode, KeyEvent, RelativeAxisCode};

const VIRTUAL_DEVICE_NAME: &str = "HyprBridge Remap Virtual Device";

/// A virtual keyboard/mouse that emits events into the compositor.
pub struct VirtualOutput {
    device: VirtualDevice,
}

impl VirtualOutput {
    /// Create the virtual device. Requires write access to `/dev/uinput`.
    pub fn new() -> Result<Self> {
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
        let axes: AttributeSet<RelativeAxisCode> = [
            RelativeAxisCode::REL_X,
            RelativeAxisCode::REL_Y,
            RelativeAxisCode::REL_WHEEL,
            RelativeAxisCode::REL_HWHEEL,
        ]
        .into_iter()
        .collect();

        let device = VirtualDevice::builder()
            .context("failed to open /dev/uinput")?
            .name(VIRTUAL_DEVICE_NAME)
            .with_keys(&keys)
            .context("failed to configure virtual keys")?
            .with_relative_axes(&axes)
            .context("failed to configure virtual axes")?
            .build()
            .context("failed to build virtual input device")?;
        Ok(Self { device })
    }

    /// Name reported to the compositor; used to avoid grabbing our own device.
    #[must_use]
    pub fn name() -> &'static str {
        VIRTUAL_DEVICE_NAME
    }

    /// Emit a key or button state change.
    pub fn emit_key(&mut self, code: u16, value: i32) -> Result<()> {
        self.emit(InputEvent::from(KeyEvent::new(KeyCode::new(code), value)))
    }

    /// Emit previously captured raw events unchanged.
    pub fn emit_raw(&mut self, events: &[InputEvent]) -> Result<()> {
        self.emit_slice(events)
    }

    fn emit(&mut self, event: InputEvent) -> Result<()> {
        self.emit_slice(&[event])
    }

    fn emit_slice(&mut self, events: &[InputEvent]) -> Result<()> {
        self.device
            .emit(events)
            .context("failed to emit virtual input events")
    }
}
