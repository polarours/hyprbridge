//! Linux clipboard access via xclip/xsel (X11) or wl-clipboard (Wayland).

use std::process::Command;

use hyprbridge_protocol::ClipboardContentType;

use crate::{ClipboardContent, ClipboardPlatform, detect_content_type};

pub struct LinuxClipboard {
    is_wayland: bool,
}

impl Default for LinuxClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxClipboard {
    pub fn new() -> Self {
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        Self { is_wayland }
    }

    fn read_cmd(&self) -> &str {
        if self.is_wayland { "wl-paste" } else { "xclip" }
    }

    fn write_cmd(&self) -> &str {
        if self.is_wayland { "wl-copy" } else { "xclip" }
    }

    fn read_args(&self) -> Vec<&str> {
        if self.is_wayland {
            vec!["--type", "none"]
        } else {
            vec!["-selection", "clipboard", "-o"]
        }
    }

    fn write_args(&self, content_type: &ClipboardContentType) -> Vec<String> {
        if self.is_wayland {
            let mime = match content_type {
                ClipboardContentType::Text => "text/plain",
                ClipboardContentType::Image => "image/png",
                ClipboardContentType::FileList => "text/uri-list",
            };
            vec!["--type".into(), mime.into()]
        } else {
            let target = match content_type {
                ClipboardContentType::Text => "UTF8_STRING",
                ClipboardContentType::Image => "image/png",
                ClipboardContentType::FileList => "text/uri-list",
            };
            vec!["-selection".into(), "clipboard".into(), "-t".into(), target.into()]
        }
    }
}

impl ClipboardPlatform for LinuxClipboard {
    fn read(&self) -> anyhow::Result<ClipboardContent> {
        let output = Command::new(self.read_cmd())
            .args(self.read_args())
            .output()?;
        if !output.status.success() {
            anyhow::bail!("clipboard read failed");
        }
        let data = output.stdout;
        let content_type = detect_content_type(&data);
        Ok(ClipboardContent {
            content_type,
            data,
            file_names: None,
        })
    }

    fn write(&self, content: &ClipboardContent) -> anyhow::Result<()> {
        use std::io::Write;
        let args = self.write_args(&content.content_type);
        let mut cmd = Command::new(self.write_cmd());
        cmd.args(&args);
        cmd.stdin(std::process::Stdio::piped());
        let mut child = cmd.spawn()?;
        if let Some(ref mut stdin) = child.stdin {
            stdin.write_all(&content.data)?;
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("clipboard write failed");
        }
        Ok(())
    }
}
