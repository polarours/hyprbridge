//! Peripheral-specific diagnostics.

use std::fs::OpenOptions;
use std::path::Path;

use crate::config;
use crate::discovery;
use crate::model::PeripheralConfig;

/// One diagnostic row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PeripheralCheck {
    /// Stable identifier.
    pub id: String,
    /// `true` = healthy, `false` = problem, `null` = informational.
    pub ok: Option<bool>,
    /// Human-readable detail.
    pub detail: String,
}

fn check(id: &str, ok: Option<bool>, detail: impl Into<String>) -> PeripheralCheck {
    PeripheralCheck {
        id: id.into(),
        ok,
        detail: detail.into(),
    }
}

/// Build peripheral diagnostics for the current machine.
pub fn checks(config: Option<&PeripheralConfig>) -> Vec<PeripheralCheck> {
    let mut result = Vec::new();

    let uinput = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/uinput");
    result.push(check(
        "uinput",
        Some(uinput.is_ok()),
        match uinput {
            Ok(_) => "/dev/uinput is writable".into(),
            Err(error) => format!("/dev/uinput unavailable: {error}"),
        },
    ));

    let receivers = discovery::logitech_receivers();
    result.push(check(
        "logitech-receiver",
        None,
        if receivers.is_empty() {
            "no Lightspeed receiver detected".into()
        } else {
            receivers
                .iter()
                .map(|(v, p)| format!("{v}:{p}"))
                .collect::<Vec<_>>()
                .join(", ")
        },
    ));

    let nodes = discovery::receiver_hidraw_nodes();
    let hidraw_writable = nodes
        .first()
        .map(|node| OpenOptions::new().read(true).write(true).open(node).is_ok())
        .unwrap_or(false);
    result.push(check(
        "receiver-hidraw",
        Some(hidraw_writable),
        if nodes.is_empty() {
            "no receiver hidraw node detected".into()
        } else {
            format!(
                "{} (writable={hidraw_writable})",
                nodes
                    .iter()
                    .map(|node| node.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
    ));

    let working = discovery::receiver_hidraw_nodes()
        .into_iter()
        .find_map(|node| {
            let transport = crate::hidpp::HidrawTransport::open(&node).ok()?;
            let mut connection = crate::hidpp::HidppConnection::new(transport, 1);
            connection.request(0, 0x00, &[0x00, 0x01]).ok()?;
            Some(node)
        });
    result.push(check(
        "hidpp-probe",
        Some(working.is_some()),
        match working {
            Some(node) => format!("{} answers HID++ feature calls", node.display()),
            None => "no receiver hidraw interface answered a HID++ probe".into(),
        },
    ));

    let devices = discovery::list_devices().unwrap_or_default();
    let g304: Vec<&str> = devices
        .iter()
        .filter(|device| {
            device.name.to_lowercase().contains("g304")
                || device.name.to_lowercase().contains("g305")
        })
        .map(|device| device.name.as_str())
        .collect();
    result.push(check(
        "g304-device",
        None,
        if g304.is_empty() {
            "not connected".into()
        } else {
            g304.join(", ")
        },
    ));

    let path = config::default_config_path();
    result.push(check(
        "config-path",
        Some(path.exists()),
        path.display().to_string(),
    ));

    if let Some(config) = config {
        result.push(check(
            "config-validation",
            Some(true),
            format!("{} device(s) configured", config.devices.len()),
        ));
        for device in &config.devices {
            let matched = crate::discovery::find_devices(&device.match_rule, &devices);
            result.push(check(
                &format!("device:{}", device.name),
                Some(!matched.is_empty()),
                if matched.is_empty() {
                    "no live device matches this rule".into()
                } else {
                    matched
                        .iter()
                        .map(|d| d.event_nodes().join(","))
                        .collect::<Vec<_>>()
                        .join(" | ")
                },
            ));
        }
    }

    let kernel =
        std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_else(|_| "unknown".into());
    result.push(check("kernel", None, kernel.trim().to_string()));

    let session = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unset".into());
    let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
    result.push(check(
        "session",
        None,
        format!("{session}, wayland={wayland}"),
    ));

    result
}

/// True when `/dev/uinput` exists and can be opened read/write.
#[must_use]
pub fn uinput_writable() -> bool {
    Path::new("/dev/uinput").exists()
        && OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/uinput")
            .is_ok()
}
