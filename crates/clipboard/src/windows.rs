//! Windows clipboard access via Win32 API.

use crate::{ClipboardContent, ClipboardPlatform};

pub struct WinClipboard;

impl ClipboardPlatform for WinClipboard {
    fn read(&self) -> anyhow::Result<ClipboardContent> {
        anyhow::bail!("Windows clipboard not yet implemented")
    }

    fn write(&self, _content: &ClipboardContent) -> anyhow::Result<()> {
        anyhow::bail!("Windows clipboard not yet implemented")
    }
}
