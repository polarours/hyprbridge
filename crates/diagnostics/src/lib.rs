//! Diagnostics and doctor reporting.

use hyprbridge_protocol::{
    CapabilityReport, InputBackendSnapshot, LinuxSessionKind, PermissionState, SupportLevel,
    WaylandSupportLevel,
};
use serde::{Deserialize, Serialize};

/// Doctor check status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// System looks healthy.
    Ok,
    /// Something needs user attention.
    Warning,
    /// The feature is not available.
    Error,
}

/// A single doctor check row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    /// Stable check id.
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Status.
    pub status: CheckStatus,
    /// Check details.
    pub detail: String,
}

/// Aggregate doctor report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Generated checks.
    pub checks: Vec<DoctorCheck>,
}

/// Build a doctor report from a capability snapshot.
#[must_use]
pub fn build_doctor_report(
    capability: &CapabilityReport,
    input_backend: Option<&InputBackendSnapshot>,
) -> DoctorReport {
    let session_check = DoctorCheck {
        id: "linux-session".into(),
        title: "Linux session".into(),
        status: match capability.linux_session {
            LinuxSessionKind::Unknown => CheckStatus::Warning,
            LinuxSessionKind::X11 | LinuxSessionKind::Wayland => CheckStatus::Ok,
        },
        detail: format!("Detected session: {:?}", capability.linux_session),
    };

    let permission_check =
        DoctorCheck {
            id: "permissions".into(),
            title: "Platform permissions".into(),
            status: match capability.permission_state {
                PermissionState::Granted => CheckStatus::Ok,
                PermissionState::Missing => CheckStatus::Error,
                PermissionState::Unknown => CheckStatus::Warning,
            },
            detail: capability.notes.first().cloned().unwrap_or_else(|| {
                "Permission status was inferred from platform heuristics.".into()
            }),
        };

    let input_check = DoctorCheck {
        id: "input-pipeline".into(),
        title: "Input capture/injection".into(),
        status: match (capability.input_capture, capability.input_injection) {
            (SupportLevel::Supported, SupportLevel::Supported) => CheckStatus::Ok,
            (SupportLevel::Unsupported, _) | (_, SupportLevel::Unsupported) => CheckStatus::Error,
            _ => CheckStatus::Warning,
        },
        detail: format!(
            "capture={:?}, injection={:?}, cursor_handoff={:?}",
            capability.input_capture, capability.input_injection, capability.cursor_handoff
        ),
    };

    let compositor_check = DoctorCheck {
        id: "wayland-backend".into(),
        title: "Wayland compositor backend".into(),
        status: match (capability.linux_session, capability.wayland_support) {
            (LinuxSessionKind::Wayland, WaylandSupportLevel::Supported) => CheckStatus::Ok,
            (LinuxSessionKind::Wayland, WaylandSupportLevel::Partial) => CheckStatus::Warning,
            (LinuxSessionKind::Wayland, WaylandSupportLevel::Unsupported) => CheckStatus::Error,
            _ => CheckStatus::Ok,
        },
        detail: if capability.linux_session == LinuxSessionKind::Wayland {
            capability
                .notes
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            "Not running under Wayland.".into()
        },
    };

    let controller_capture_check = DoctorCheck {
        id: "controller-capture".into(),
        title: "Controller capture runtime".into(),
        status: match input_backend.and_then(|backend| backend.controller_capture.as_ref()) {
            Some(capture) if capture.readable_devices == 0 => CheckStatus::Error,
            Some(capture) if capture.exclusive_capture_active && capture.grabbed_devices == 0 => {
                CheckStatus::Warning
            }
            Some(_) => CheckStatus::Ok,
            None if capability.linux_session == LinuxSessionKind::Wayland => CheckStatus::Warning,
            None => CheckStatus::Ok,
        },
        detail: match input_backend.and_then(|backend| backend.controller_capture.as_ref()) {
            Some(capture) => format!(
                "readable_evdev_devices={}, grabbed_devices={}, exclusive_capture_active={}",
                capture.readable_devices, capture.grabbed_devices, capture.exclusive_capture_active
            ),
            None if capability.linux_session == LinuxSessionKind::Wayland => {
                "No live Wayland controller capture snapshot is available.".into()
            }
            None => "Controller capture runtime reporting is not required for this backend.".into(),
        },
    };

    let target_injection_check = DoctorCheck {
        id: "target-injection-runtime".into(),
        title: "Target injection runtime".into(),
        status: match input_backend.and_then(|backend| backend.target_injection.as_ref()) {
            Some(injection) if injection.uinput_writable => CheckStatus::Ok,
            Some(injection) if injection.last_error.is_some() => CheckStatus::Warning,
            Some(_) if capability.linux_session == LinuxSessionKind::Wayland => {
                CheckStatus::Warning
            }
            Some(_) => CheckStatus::Ok,
            None if capability.linux_session == LinuxSessionKind::Wayland => CheckStatus::Warning,
            None => CheckStatus::Ok,
        },
        detail: match input_backend.and_then(|backend| backend.target_injection.as_ref()) {
            Some(injection) => format!(
                "primary_backend={}, fallback_backend={}, uinput_writable={}, last_error={}",
                injection.primary_backend,
                injection.fallback_backend.as_deref().unwrap_or("none"),
                injection.uinput_writable,
                injection.last_error.as_deref().unwrap_or("none")
            ),
            None if capability.linux_session == LinuxSessionKind::Wayland => {
                "No live target injection snapshot is available.".into()
            }
            None => "Target injection runtime reporting is not required for this backend.".into(),
        },
    };

    let wheel_check = DoctorCheck {
        id: "wheel-injection".into(),
        title: "Target wheel injection".into(),
        status: match input_backend.map(|backend| backend.target_wheel_injection) {
            Some(SupportLevel::Supported) => CheckStatus::Ok,
            Some(SupportLevel::Unsupported) => CheckStatus::Error,
            Some(SupportLevel::Planned) => CheckStatus::Warning,
            None => CheckStatus::Warning,
        },
        detail: match input_backend {
            Some(backend) => format!(
                "backend={}, target_wheel_injection={:?}",
                backend.backend, backend.target_wheel_injection
            ),
            None => "No live input backend snapshot is available.".into(),
        },
    };

    let snapshot = hyprbridge_workspace::inspect_local();
    let desktop = hyprbridge_workspace::assess_desktop(&snapshot);
    let desktop_check = DoctorCheck {
        id: "desktop-mode".into(),
        title: "Desktop mode (ported from DevBridge)".into(),
        status: match desktop.state.as_str() {
            "supported" => CheckStatus::Ok,
            "partial" => CheckStatus::Warning,
            _ => CheckStatus::Error,
        },
        detail: format!("backend={}, {}", desktop.backend, desktop.summary),
    };

    DoctorReport {
        checks: vec![
            session_check,
            permission_check,
            input_check,
            compositor_check,
            controller_capture_check,
            target_injection_check,
            wheel_check,
            desktop_check,
        ],
    }
}

#[cfg(test)]
mod tests {
    use hyprbridge_protocol::{
        CapabilityReport, InputBackendSnapshot, LinuxSessionKind, PermissionState, SupportLevel,
        WaylandSupportLevel,
    };

    use super::{CheckStatus, build_doctor_report};

    #[test]
    fn missing_permissions_surface_as_error() {
        let report = CapabilityReport {
            platform: "linux".into(),
            linux_session: LinuxSessionKind::Wayland,
            wayland_support: WaylandSupportLevel::Partial,
            input_capture: SupportLevel::Planned,
            input_injection: SupportLevel::Unsupported,
            cursor_handoff: SupportLevel::Planned,
            permission_state: PermissionState::Missing,
            notes: vec!["Portal access was not available.".into()],
        };

        let doctor = build_doctor_report(&report, None);
        assert_eq!(doctor.checks[1].status, CheckStatus::Error);
        assert_eq!(doctor.checks[2].status, CheckStatus::Error);
        assert_eq!(doctor.checks[3].status, CheckStatus::Warning);
        assert_eq!(doctor.checks[4].status, CheckStatus::Warning);
        assert_eq!(doctor.checks[5].status, CheckStatus::Warning);
        assert_eq!(doctor.checks[6].status, CheckStatus::Warning);
    }

    #[test]
    fn wayland_controller_capture_and_wheel_surface_runtime_state() {
        let report = CapabilityReport {
            platform: "linux".into(),
            linux_session: LinuxSessionKind::Wayland,
            wayland_support: WaylandSupportLevel::Partial,
            input_capture: SupportLevel::Supported,
            input_injection: SupportLevel::Supported,
            cursor_handoff: SupportLevel::Supported,
            permission_state: PermissionState::Granted,
            notes: vec!["Hyprland runtime detected.".into()],
        };
        let backend = InputBackendSnapshot {
            backend: "wayland-hyprland".into(),
            available: true,
            pointer: None,
            edge_candidate: None,
            controller_capture: Some(hyprbridge_protocol::ControllerCaptureSnapshot {
                readable_devices: 3,
                grabbed_devices: 2,
                exclusive_capture_active: true,
            }),
            target_injection: Some(hyprbridge_protocol::TargetInjectionSnapshot {
                primary_backend: "uinput-ready".into(),
                fallback_backend: Some("hyprland-ipc-dispatcher".into()),
                uinput_writable: true,
                last_error: None,
            }),
            target_wheel_injection: SupportLevel::Planned,
            detail: "runtime".into(),
        };

        let doctor = build_doctor_report(&report, Some(&backend));
        assert_eq!(doctor.checks[4].status, CheckStatus::Ok);
        assert_eq!(doctor.checks[5].status, CheckStatus::Ok);
        assert_eq!(doctor.checks[6].status, CheckStatus::Warning);
    }
}
