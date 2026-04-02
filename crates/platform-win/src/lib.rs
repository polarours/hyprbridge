//! Windows platform backend for input capture and injection.

use hyprbridge_input::CapabilityProbe;
use hyprbridge_protocol::{CapabilityReport, PermissionState};

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
