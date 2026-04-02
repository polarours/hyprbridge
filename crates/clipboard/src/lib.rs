//! Clipboard synchronization between paired devices.

use std::sync::Arc;

use chrono::Utc;
use hyprbridge_protocol::{
    ClipboardContentType, ClipboardStreamHeader, DeviceId,
};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

pub mod history;
pub use history::{ClipboardHistory, ClipboardHistoryConfig, ClipboardHistoryEntry};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxClipboard;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacClipboard;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WinClipboard;

/// Content read from or written to the local clipboard.
#[derive(Debug, Clone)]
pub struct ClipboardContent {
    /// Type of content.
    pub content_type: ClipboardContentType,
    /// Raw content bytes.
    pub data: Vec<u8>,
    /// File names (for FileList type).
    pub file_names: Option<Vec<String>>,
}

impl ClipboardContent {
    /// Compute SHA-256 hash of the content.
    #[must_use]
    pub fn hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.data);
        hex::encode(hasher.finalize())
    }

    /// Total size in bytes.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.data.len() as u64
    }
}

/// Event emitted when clipboard content changes.
#[derive(Debug, Clone)]
pub enum ClipboardEvent {
    /// Local clipboard changed, ready to sync to peers.
    LocalChanged(ClipboardContent),
    /// Remote clipboard received, ready to apply locally.
    RemoteReceived(ClipboardContent, DeviceId),
}

/// Platform-specific clipboard operations.
pub trait ClipboardPlatform: Send + Sync {
    /// Read current clipboard content.
    fn read(&self) -> anyhow::Result<ClipboardContent>;
    /// Write content to clipboard.
    fn write(&self, content: &ClipboardContent) -> anyhow::Result<()>;
}

/// Clipboard manager that polls for changes and syncs across devices.
pub struct ClipboardManager {
    last_hash: RwLock<Option<String>>,
    local_device_id: DeviceId,
    platform: Arc<dyn ClipboardPlatform>,
}

impl ClipboardManager {
    /// Create a new clipboard manager.
    pub fn new(local_device_id: DeviceId, platform: Arc<dyn ClipboardPlatform>) -> Self {
        Self {
            last_hash: RwLock::new(None),
            local_device_id,
            platform,
        }
    }

    /// Poll the local clipboard for changes.
    pub async fn poll_once(&self) -> Option<ClipboardEvent> {
        let content = match self.platform.read() {
            Ok(c) => c,
            Err(_) => return None,
        };
        let hash = content.hash();
        let mut last = self.last_hash.write().await;
        if *last == Some(hash.clone()) {
            return None;
        }
        *last = Some(hash);
        Some(ClipboardEvent::LocalChanged(content))
    }

    /// Apply remote clipboard content to the local clipboard.
    pub async fn apply_remote(
        &self,
        content: ClipboardContent,
        _source: DeviceId,
    ) -> anyhow::Result<bool> {
        let hash = content.hash();
        let mut last = self.last_hash.write().await;
        if *last == Some(hash.clone()) {
            return Ok(false);
        }
        self.platform.write(&content)?;
        *last = Some(hash);
        Ok(true)
    }

    /// Return the last observed clipboard content hash.
    pub async fn last_hash(&self) -> Option<String> {
        self.last_hash.read().await.clone()
    }

    /// Build a stream header for this content.
    pub fn build_header(&self, content: &ClipboardContent) -> ClipboardStreamHeader {
        ClipboardStreamHeader {
            content_type: content.content_type.clone(),
            content_hash: content.hash(),
            size_bytes: content.size(),
            source_device: self.local_device_id.clone(),
            timestamp: Utc::now(),
            file_names: content.file_names.clone(),
        }
    }
}

/// Detect content type from raw data.
#[must_use]
pub fn detect_content_type(data: &[u8]) -> ClipboardContentType {
    if data.len() >= 8 && data[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return ClipboardContentType::Image;
    }
    if data.len() >= 2 && data[0..2] == [0xFF, 0xD8] {
        return ClipboardContentType::Image;
    }
    ClipboardContentType::Text
}

/// Create the appropriate platform clipboard backend.
pub fn create_platform_clipboard() -> Arc<dyn ClipboardPlatform> {
    #[cfg(target_os = "linux")]
    { Arc::new(LinuxClipboard::new()) }
    #[cfg(target_os = "macos")]
    { Arc::new(MacClipboard) }
    #[cfg(target_os = "windows")]
    { Arc::new(WinClipboard) }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    { panic!("unsupported platform") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_deterministic() {
        let content = ClipboardContent {
            content_type: ClipboardContentType::Text,
            data: b"hello world".to_vec(),
            file_names: None,
        };
        let hash1 = content.hash();
        let hash2 = content.hash();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn different_content_different_hash() {
        let a = ClipboardContent {
            content_type: ClipboardContentType::Text,
            data: b"hello".to_vec(),
            file_names: None,
        };
        let b = ClipboardContent {
            content_type: ClipboardContentType::Text,
            data: b"world".to_vec(),
            file_names: None,
        };
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn detect_png_image() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert_eq!(detect_content_type(&data), ClipboardContentType::Image);
    }

    #[test]
    fn detect_text() {
        let data = b"plain text content";
        assert_eq!(detect_content_type(data), ClipboardContentType::Text);
    }
}
