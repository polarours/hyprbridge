//! evdev -> uinput remapping engine for HyprBridge.
//!
//! Reads a configured input device with an exclusive evdev grab and emits
//! remapped key/button actions through a virtual uinput device. Unbound events
//! are passed through unchanged, so pointer motion and ordinary clicks keep
//! working while only the configured buttons change meaning.
//!
//! Only one process may grab a device. Disable makima, input-remapper and
//! keyd for the same device before starting this engine.

pub mod engine;
pub mod events;
pub mod ipc;
pub mod output;

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use evdev::{Device, EventType};
use hyprbridge_peripherals::config::{default_config_path, load};
use hyprbridge_peripherals::hidpp::{HidppConnection, HidrawTransport};
use hyprbridge_peripherals::model::Action;

use crate::engine::BindingEngine;
use crate::events::{event_name, parse_event_code};
use crate::output::VirtualOutput;

/// What to emit when a captured button is released.
#[derive(Debug, Clone)]
enum Held {
    /// Emit the original release event.
    Raw,
    /// Emit releases for these injected key codes.
    Keys(Vec<u16>),
    /// Swallow the release.
    None,
}

/// A running remapper bound to one physical device.
pub struct Remapper {
    engine: BindingEngine,
    device_name: String,
}

impl Remapper {
    /// Build a remapper from the named configured device.
    pub fn new(device_name: &str, config_path: Option<&Path>) -> Result<Self> {
        let path = config_path
            .map(Path::to_path_buf)
            .unwrap_or_else(default_config_path);
        let config = load(&path).with_context(|| format!("config: {}", path.display()))?;
        let device = config
            .devices
            .into_iter()
            .find(|device| {
                device.name.eq_ignore_ascii_case(device_name)
                    || device
                        .name
                        .to_lowercase()
                        .contains(&device_name.to_lowercase())
            })
            .ok_or_else(|| anyhow!("no configured device matching {device_name:?}"))?;
        Ok(Self {
            engine: BindingEngine::new(device),
            device_name: device_name.to_string(),
        })
    }

    /// The configured device name.
    #[must_use]
    pub fn device_name(&self) -> &str {
        self.engine.device_name()
    }

    /// One-line summary for dry runs and diagnostics.
    #[must_use]
    pub fn summary(&self) -> String {
        let profile = self.engine.profile();
        format!(
            "{}: active profile {}, {} binding(s)",
            self.device_name(),
            profile.name,
            profile.bindings.len()
        )
    }

    /// Grab the device and run until the process is stopped.
    pub fn run(&mut self) -> Result<()> {
        let mut source = find_device(&self.device_name)?;
        let source_name = source.name().unwrap_or("unknown").to_string();
        if source_name == VirtualOutput::name() {
            return Err(anyhow!("refusing to grab our own virtual device"));
        }
        source.grab().with_context(|| {
            format!(
                "cannot grab {source_name}; another remapper may own it, or evdev permissions are missing"
            )
        })?;
        println!("grabbed {source_name}; press Ctrl-C to stop");

        let mut output = VirtualOutput::new()?;
        let mut held: HashMap<u16, Held> = HashMap::new();

        loop {
            {
                let events = source
                    .fetch_events()
                    .context("failed to read evdev events")?;
                for event in events {
                    if event.event_type() != EventType::KEY {
                        output.emit_raw(&[event])?;
                        continue;
                    }
                    let code = event.code();
                    match event.value() {
                        1 => {
                            self.engine.refresh_window();
                            let action = self.engine.action_for(code).cloned();
                            match action {
                                Some(action) => {
                                    let next = self.apply_press(&action, code, &mut output)?;
                                    held.insert(code, next);
                                }
                                None => {
                                    output.emit_raw(&[event])?;
                                    held.insert(code, Held::Raw);
                                }
                            }
                        }
                        0 => match held.remove(&code) {
                            Some(Held::Keys(codes)) => {
                                for key in codes {
                                    output.emit_key(key, 0)?;
                                }
                            }
                            Some(Held::Raw) => output.emit_raw(&[event])?,
                            Some(Held::None) | None => {}
                        },
                        _ => output.emit_raw(&[event])?,
                    }
                }
            }
        }
    }

    fn apply_press(
        &mut self,
        action: &Action,
        raw_code: u16,
        output: &mut VirtualOutput,
    ) -> Result<Held> {
        match action {
            Action::Key { keys } => {
                let codes = resolve_keys(keys)?;
                for code in &codes {
                    output.emit_key(*code, 1)?;
                }
                Ok(Held::Keys(codes))
            }
            Action::Macro { keys } => {
                let codes = resolve_keys(keys)?;
                for code in &codes {
                    output.emit_key(*code, 1)?;
                    output.emit_key(*code, 0)?;
                    thread::sleep(Duration::from_millis(20));
                }
                Ok(Held::None)
            }
            Action::Exec { argv } => {
                if let Some((program, args)) = argv.split_first() {
                    if let Err(error) = Command::new(program).args(args).spawn() {
                        eprintln!("remap exec failed for {program}: {error}");
                    }
                }
                Ok(Held::None)
            }
            Action::Dpi { value } => {
                if let Err(error) = write_dpi(*value) {
                    eprintln!("remap DPI write failed: {error}");
                }
                Ok(Held::None)
            }
            Action::Profile { value } => {
                if !self.engine.select_profile(value) {
                    eprintln!("remap: unknown profile {value:?}");
                }
                Ok(Held::None)
            }
            Action::Pass => Ok(Held::Raw),
            Action::Disable => Ok(Held::None),
        }
    }
}

fn resolve_keys(keys: &[String]) -> Result<Vec<u16>> {
    keys.iter()
        .map(|name| {
            parse_event_code(name).ok_or_else(|| anyhow!("unknown key name {name:?} in binding"))
        })
        .collect()
}

fn find_device(name: &str) -> Result<Device> {
    let needle = name.to_lowercase();
    for (_, device) in evdev::enumerate() {
        if device
            .name()
            .is_some_and(|found| found.to_lowercase().contains(&needle))
        {
            return Ok(device);
        }
    }
    Err(anyhow!("no evdev device matching {name:?}"))
}

fn write_dpi(dpi: u32) -> Result<()> {
    let value = u16::try_from(dpi).context("DPI must fit in 16 bits")?;
    let nodes = hyprbridge_peripherals::discovery::receiver_hidraw_nodes();
    for node in nodes {
        let Ok(transport) = HidrawTransport::open(&node) else {
            continue;
        };
        let mut connection = HidppConnection::new(transport, 1);
        if connection.request(0, 0x00, &[0x22, 0x01]).is_ok() {
            return connection.set_dpi(value).map_err(Into::into);
        }
    }
    Err(anyhow!("no responding Lightspeed receiver for DPI write"))
}

/// Print raw events from a device without grabbing it (for discovering codes).
pub fn watch_events(name: &str) -> Result<()> {
    let mut device = find_device(name)?;
    println!(
        "watching {}; press buttons to see their codes",
        device.name().unwrap_or(name)
    );
    loop {
        let events = device
            .fetch_events()
            .context("failed to read evdev events")?;
        for event in events {
            if event.event_type() == EventType::KEY && (event.value() == 0 || event.value() == 1) {
                println!(
                    "type={:?} code={:<4} name={:<16} value={}",
                    event.event_type(),
                    event.code(),
                    event_name(event.code()),
                    event.value()
                );
            }
        }
    }
}

/// Prepare diagnostics text for the `remap devices` command.
#[must_use]
pub fn describe_device(name: &str) -> String {
    format!("configured device: {name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_key_names() {
        assert_eq!(resolve_keys(&["KEY_LEFTMETA".into()]).unwrap(), vec![125]);
        assert_eq!(resolve_keys(&["BTN_SIDE".into()]).unwrap(), vec![0x113]);
        assert!(resolve_keys(&["NOPE".into()]).is_err());
    }
}
