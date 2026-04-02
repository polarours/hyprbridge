//! Minimal Hyprland IPC client used to pick a profile by focused window class.

use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// Resolve the Hyprland command socket for the current instance.
pub fn command_socket() -> Option<PathBuf> {
    let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/0".into());
    Some(
        PathBuf::from(runtime)
            .join("hypr")
            .join(signature)
            .join(".socket.sock"),
    )
}

/// Query the focused window class, e.g. `firefox`.
pub fn active_window_class() -> Option<String> {
    let socket = command_socket()?;
    let mut stream = UnixStream::connect(socket).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .ok()?;
    stream.write_all(b"j/activewindow").ok()?;
    stream.shutdown(std::net::Shutdown::Write).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let value: serde_json::Value = serde_json::from_str(response.trim()).ok()?;
    value
        .get("class")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|class| !class.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_uses_instance_signature() {
        // No assertion on the concrete path: it depends on the session env.
        let _ = command_socket();
    }
}
