//! Stable platform abstraction layer for input handling.

use async_trait::async_trait;
use hyprbridge_protocol::{
    CapabilityReport, DeviceId, InputBackendSnapshot, MouseButton, PointerSnapshot, ScreenEdge,
    SupportLevel,
};
use thiserror::Error;

/// Errors surfaced by platform backends.
#[derive(Debug, Error)]
pub enum BackendError {
    /// The operation is not implemented on this platform/backend yet.
    #[error("backend operation is not implemented")]
    NotImplemented,
    /// A required permission is missing.
    #[error("required platform permission is missing: {0}")]
    MissingPermission(String),
    /// An unexpected platform error occurred.
    #[error("platform backend error: {0}")]
    Other(String),
}

/// Probe the current machine and return a capability report.
pub trait CapabilityProbe: Send + Sync {
    /// Inspect the current environment.
    fn probe(&self) -> CapabilityReport;
}

/// Capture local controller-side input.
#[async_trait]
pub trait ControllerInputBackend: Send + Sync {
    /// Start the backend.
    async fn start(&self) -> Result<(), BackendError>;
    /// Stop the backend.
    async fn stop(&self) -> Result<(), BackendError>;
    /// Read the current pointer position and root screen extent.
    async fn pointer_snapshot(&self) -> Result<PointerSnapshot, BackendError>;
    /// Read a richer controller-side sample for remote relay loops.
    async fn sample(&self) -> Result<ControllerSample, BackendError> {
        Ok(ControllerSample {
            pointer: self.pointer_snapshot().await?,
            buttons_mask: 0,
            keymap: [0; 32],
            scroll_horizontal_steps: 0,
            scroll_vertical_steps: 0,
        })
    }
    /// Inspect whether the pointer is currently crossing a configured dead zone.
    async fn edge_candidate(&self, threshold_px: u16) -> Result<Option<ScreenEdge>, BackendError>;
    /// Summarize runtime backend state for diagnostics.
    async fn snapshot(&self, threshold_px: u16) -> Result<InputBackendSnapshot, BackendError> {
        let pointer = self.pointer_snapshot().await?;
        let edge_candidate = self.edge_candidate(threshold_px).await?;
        Ok(InputBackendSnapshot {
            backend: "unknown".into(),
            available: true,
            pointer: Some(pointer),
            edge_candidate,
            controller_capture: None,
            target_injection: None,
            target_wheel_injection: SupportLevel::Planned,
            detail: "input backend snapshot available".into(),
        })
    }
}

/// Inject events on a target machine.
#[async_trait]
pub trait TargetInjectionBackend: Send + Sync {
    /// Move the pointer into a remote target.
    async fn focus_remote(&self, device: &DeviceId) -> Result<(), BackendError>;
    /// Return focus to the local machine.
    async fn focus_local(&self) -> Result<(), BackendError>;
    /// Move the pointer relative to its current position.
    async fn move_pointer_relative(
        &self,
        _delta_x: i16,
        _delta_y: i16,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }
    /// Set a remote pointer button state.
    async fn set_pointer_button(
        &self,
        _button: MouseButton,
        _pressed: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }
    /// Scroll the remote pointer by wheel steps.
    async fn scroll_pointer(
        &self,
        _horizontal_steps: i16,
        _vertical_steps: i16,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }
    /// Set a remote keyboard key state by native keycode.
    async fn set_key(&self, _keycode: u8, _pressed: bool) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }
}

/// Handle screen-boundary transitions.
#[async_trait]
pub trait BoundaryHandoffBackend: Send + Sync {
    /// Prepare for a boundary crossing.
    async fn handoff(&self, edge: ScreenEdge) -> Result<(), BackendError>;
}

/// Generate user-facing permission help for the current platform.
pub trait PermissionGuide: Send + Sync {
    /// Human-readable guidance lines.
    fn guidance(&self) -> Vec<String>;
}

/// Controller-side input sample used by transport relay loops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerSample {
    /// Current pointer snapshot.
    pub pointer: PointerSnapshot,
    /// Current X11-style pointer button mask bits when available.
    pub buttons_mask: u16,
    /// Current raw keymap bitset when available.
    pub keymap: [u8; 32],
    /// Pending horizontal wheel steps since the previous sample.
    pub scroll_horizontal_steps: i16,
    /// Pending vertical wheel steps since the previous sample.
    pub scroll_vertical_steps: i16,
}
