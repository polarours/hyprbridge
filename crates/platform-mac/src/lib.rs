//! macOS platform backend for input capture and injection.

use hyprbridge_input::CapabilityProbe;
use hyprbridge_protocol::{CapabilityReport, PermissionState};

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
            report.notes
                .push("macOS backend requires building on macOS.".into());
            report
        }
    }
}
