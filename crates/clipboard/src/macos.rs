//! macOS clipboard access via pbcopy/pbpaste.

use std::process::Command;

use hyprbridge_protocol::ClipboardContentType;

use crate::{ClipboardContent, ClipboardPlatform, detect_content_type};

pub struct MacClipboard;

impl ClipboardPlatform for MacClipboard {
    fn read(&self) -> anyhow::Result<ClipboardContent> {
        let output = Command::new("pbpaste").output()?;
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
        let mut cmd = Command::new("pbcopy");
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
