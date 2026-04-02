# Clipboard Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement bidirectional clipboard sync between paired devices — text via ControlFrame, images/files via dedicated QUIC streams.

**Architecture:** Hybrid approach: text clipboard via new `ClipboardText` ControlFrame variant, images/files via dedicated QUIC streams with chunked transfer. Polling-based change detection (500ms), SHA-256 content hashing for dedup, last-writer-wins conflict resolution.

**Tech Stack:** Rust, SHA-256, existing QUIC transport, platform clipboard APIs (xclip/wl-copy, pbcopy/pbpaste, Win32)

---

## Task 1: Protocol Extensions

**Covers:** [S3]

**Files:**
- Modify: `crates/protocol/src/lib.rs`

- [ ] **Step 1: Add ClipboardContentType enum**

In `crates/protocol/src/lib.rs`, add:

```rust
/// Clipboard content type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardContentType {
    /// Plain text.
    Text,
    /// Image data (PNG/JPEG).
    Image,
    /// List of file paths.
    FileList,
}
```

- [ ] **Step 2: Add ClipboardStreamHeader**

```rust
/// Clipboard update header sent over dedicated stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardStreamHeader {
    /// Type of clipboard content.
    pub content_type: ClipboardContentType,
    /// SHA-256 hash of content for deduplication.
    pub content_hash: String,
    /// Total size in bytes.
    pub size_bytes: u64,
    /// Device that copied this content.
    pub source_device: DeviceId,
    /// When the content was copied.
    pub timestamp: DateTime<Utc>,
    /// File names for FileList type.
    pub file_names: Option<Vec<String>>,
}
```

- [ ] **Step 3: Add ClipboardStreamCommand**

```rust
/// Command sent over a dedicated clipboard QUIC stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardStreamCommand {
    /// Begin a clipboard update stream.
    Begin(ClipboardStreamHeader),
    /// Data chunk (up to 64KB).
    Chunk(Vec<u8>),
    /// End of stream.
    End,
}
```

- [ ] **Step 4: Add ClipboardText variant to ControlFrame**

In the `ControlFrame` enum, add:

```rust
/// Clipboard text update (small payload, fits in control frame).
ClipboardText { text: String },
```

- [ ] **Step 5: Add clipboard types to Cargo.toml**

Ensure `sha2` is in workspace dependencies (already present). Add `chrono` if not already present (already present).

- [ ] **Step 6: Run check**

Run: `cargo check -p hyprbridge-protocol`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/protocol/
git commit -m "feat(protocol): add clipboard sync types and ControlFrame variant"
```

---

## Task 2: Clipboard Crate Scaffolding

**Covers:** [S4]

**Files:**
- Create: `crates/clipboard/Cargo.toml`
- Create: `crates/clipboard/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Add clipboard to workspace members**

In root `Cargo.toml`, add `"crates/clipboard"` to the `members` list.

- [ ] **Step 2: Create Cargo.toml**

Create `crates/clipboard/Cargo.toml`:

```toml
[package]
name = "hyprbridge-clipboard"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
anyhow.workspace = true
chrono.workspace = true
hyprbridge-protocol = { path = "../protocol" }
sha2.workspace = true
tokio.workspace = true
tracing.workspace = true
```

- [ ] **Step 3: Create lib.rs scaffolding**

Create `crates/clipboard/src/lib.rs`:

```rust
//! Clipboard synchronization between paired devices.

use std::sync::Arc;

use chrono::Utc;
use hyprbridge_protocol::{
    ClipboardContentType, ClipboardStreamHeader, DeviceId,
};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

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
    pub fn hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.data);
        hex::encode(hasher.finalize())
    }

    /// Total size in bytes.
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
    /// Detect content type from clipboard data.
    fn detect_type(data: &[u8]) -> ClipboardContentType;
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
        source: DeviceId,
    ) -> anyhow::Result<bool> {
        let hash = content.hash();
        let mut last = self.last_hash.write().await;
        if *last == Some(hash) {
            return Ok(false); // Already have this content
        }
        self.platform.write(&content)?;
        *last = Some(hash);
        Ok(true)
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
pub fn detect_content_type(data: &[u8]) -> ClipboardContentType {
    // PNG magic bytes
    if data.len() >= 8 && data[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return ClipboardContentType::Image;
    }
    // JPEG magic bytes
    if data.len() >= 2 && data[0..2] == [0xFF, 0xD8] {
        return ClipboardContentType::Image;
    }
    // Default to text
    ClipboardContentType::Text
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
```

- [ ] **Step 4: Run check**

Run: `cargo check -p hyprbridge-clipboard`
Expected: PASS

- [ ] **Step 5: Run tests**

Run: `cargo test -p hyprbridge-clipboard`
Expected: 4 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/clipboard/ Cargo.toml
git commit -m "feat(clipboard): add clipboard crate scaffolding with hash and type detection"
```

---

## Task 3: Platform Clipboard Backends

**Covers:** [S4]

**Files:**
- Create: `crates/clipboard/src/linux.rs`
- Create: `crates/clipboard/src/macos.rs`
- Create: `crates/clipboard/src/windows.rs`
- Modify: `crates/clipboard/src/lib.rs`
- Modify: `crates/clipboard/Cargo.toml`

- [ ] **Step 1: Add platform dependencies**

In `crates/clipboard/Cargo.toml`, add:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_System_Memory",
    "Win32_UI_WindowsAndMessaging",
] }
```

- [ ] **Step 2: Create Linux backend**

Create `crates/clipboard/src/linux.rs`:

```rust
//! Linux clipboard access via xclip/xsel (X11) or wl-clipboard (Wayland).

use std::process::Command;

use hyprbridge_protocol::ClipboardContentType;

use crate::{ClipboardContent, ClipboardPlatform, detect_content_type};

pub struct LinuxClipboard {
    is_wayland: bool,
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
        let args = self.write_args(&content.content_type);
        let mut cmd = Command::new(self.write_cmd());
        cmd.args(&args);
        cmd.stdin(std::io::Cursor::new(&content.data));

        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("clipboard write failed");
        }
        Ok(())
    }

    fn detect_type(_data: &[u8]) -> ClipboardContentType {
        ClipboardContentType::Text
    }
}
```

- [ ] **Step 3: Create macOS backend**

Create `crates/clipboard/src/macos.rs`:

```rust
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
        let mut cmd = Command::new("pbcopy");
        cmd.stdin(std::io::Cursor::new(&content.data));
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("clipboard write failed");
        }
        Ok(())
    }

    fn detect_type(_data: &[u8]) -> ClipboardContentType {
        ClipboardContentType::Text
    }
}
```

- [ ] **Step 4: Create Windows backend**

Create `crates/clipboard/src/windows.rs`:

```rust
//! Windows clipboard access via Win32 API.

use hyprbridge_protocol::ClipboardContentType;

use crate::{ClipboardContent, ClipboardPlatform, detect_content_type};

pub struct WinClipboard;

impl ClipboardPlatform for WinClipboard {
    fn read(&self) -> anyhow::Result<ClipboardContent> {
        // Placeholder — real implementation uses Win32 OpenClipboard/GetClipboardData
        anyhow::bail!("Windows clipboard not yet implemented")
    }

    fn write(&self, _content: &ClipboardContent) -> anyhow::Result<()> {
        // Placeholder — real implementation uses Win32 SetClipboardData
        anyhow::bail!("Windows clipboard not yet implemented")
    }

    fn detect_type(_data: &[u8]) -> ClipboardContentType {
        ClipboardContentType::Text
    }
}
```

- [ ] **Step 5: Update lib.rs to expose platform backends**

In `crates/clipboard/src/lib.rs`, add platform modules:

```rust
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
```

- [ ] **Step 6: Run check**

Run: `cargo check -p hyprbridge-clipboard`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/clipboard/
git commit -m "feat(clipboard): add platform clipboard backends (Linux, macOS, Windows)"
```

---

## Task 4: QUIC Clipboard Stream Integration

**Covers:** [S5]

**Files:**
- Modify: `crates/network/src/lib.rs`

- [ ] **Step 1: Add clipboard stream methods to QuicTransport**

In `crates/network/src/lib.rs`, add to `QuicTransport`:

```rust
use hyprbridge_protocol::ClipboardStreamCommand;

impl QuicTransport {
    /// Open a dedicated clipboard stream to a peer.
    pub async fn open_clipboard_stream(
        &self,
        device_id: &DeviceId,
    ) -> Result<quinn::OwnedBiDirStream> {
        let connection = self
            .connections
            .lock()
            .await
            .get(&device_id.0)
            .cloned()
            .ok_or_else(|| anyhow!("device {} has no active session", device_id.0))?;

        let (send, recv) = connection
            .open_bi()
            .await
            .context("failed to open clipboard stream")?;
        Ok((send, recv))
    }

    /// Send clipboard content over a dedicated stream.
    pub async fn send_clipboard_content(
        &self,
        device_id: &DeviceId,
        header: hyprbridge_protocol::ClipboardStreamHeader,
        data: &[u8],
    ) -> Result<()> {
        let (mut send, _recv) = self.open_clipboard_stream(device_id).await?;

        // Send begin
        let begin = ClipboardStreamCommand::Begin(header);
        send.write_all(&serde_json::to_vec(&begin)?).await?;
        send.finish().context("failed to finish begin")?;

        // Reopen for chunks (QUIC streams are one-shot per finish)
        let (mut send, _recv) = self.open_clipboard_stream(device_id).await?;

        // Send chunks (64KB each)
        const CHUNK_SIZE: usize = 64 * 1024;
        for chunk in data.chunks(CHUNK_SIZE) {
            let cmd = ClipboardStreamCommand::Chunk(chunk.to_vec());
            send.write_all(&serde_json::to_vec(&cmd)?).await?;
        }

        // Send end
        let end = ClipboardStreamCommand::End;
        send.write_all(&serde_json::to_vec(&end)?).await?;
        send.finish().context("failed to finish end")?;

        Ok(())
    }
}
```

- [ ] **Step 2: Add clipboard stream accept handler**

In the `spawn_accept_loop` or `handle_incoming_connection` function, add handling for clipboard streams. The existing accept loop handles bi-directional streams for control frames. Clipboard streams use a different first-message format (`ClipboardStreamCommand::Begin`).

Add a check: if the first deserialized message is a `ClipboardStreamCommand`, route to clipboard handler instead of control handler.

- [ ] **Step 3: Run check**

Run: `cargo check -p hyprbridge-network`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/network/
git commit -m "feat(network): add QUIC clipboard stream support"
```

---

## Task 5: Daemon Integration

**Covers:** [S6]

**Files:**
- Modify: `crates/daemon/src/lib.rs`
- Modify: `crates/daemon/Cargo.toml`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Add clipboard dependency to daemon**

In `crates/daemon/Cargo.toml`, add:

```toml
hyprbridge-clipboard = { path = "../clipboard" }
```

- [ ] **Step 2: Add ClipboardManager to DaemonState**

In `crates/daemon/src/lib.rs`:

```rust
use hyprbridge_clipboard::{ClipboardManager, ClipboardEvent, create_platform_clipboard};

pub struct DaemonState {
    // ... existing fields ...
    clipboard: Arc<ClipboardManager>,
}
```

Initialize in `DaemonState::new`:

```rust
let platform = create_platform_clipboard();
let clipboard = Arc::new(ClipboardManager::new(
    identity.public_identity().device_id.clone(),
    platform,
));
```

- [ ] **Step 3: Spawn clipboard poll loop**

In `serve()`, after starting the daemon:

```rust
let clipboard_clone = Arc::clone(&state.clipboard);
tokio::spawn(async move {
    loop {
        sleep(Duration::from_millis(500)).await;
        if let Some(event) = clipboard_clone.poll_once().await {
            match event {
                ClipboardEvent::LocalChanged(content) => {
                    // Send to all connected peers
                    // For text, use ClipboardText ControlFrame
                    // For images/files, use dedicated stream
                }
                _ => {}
            }
        }
    }
});
```

- [ ] **Step 4: Handle incoming ClipboardText in DaemonControlHandler**

Add to the `handle` method:

```rust
ControlFrame::ClipboardText { text } => {
    let content = ClipboardContent {
        content_type: ClipboardContentType::Text,
        data: text.into_bytes(),
        file_names: None,
    };
    match state.clipboard.apply_remote(content, peer.device_id.clone()).await {
        Ok(true) => ControlAck {
            accepted: true,
            detail: "clipboard text applied".into(),
        },
        Ok(false) => ControlAck {
            accepted: true,
            detail: "clipboard text already up to date".into(),
        },
        Err(e) => ControlAck {
            accepted: false,
            detail: format!("failed to apply clipboard: {e}"),
        },
    }
}
```

- [ ] **Step 5: Add CLI clipboard commands**

In `crates/cli/src/main.rs`, add subcommands:

```rust
/// Clipboard management commands.
#[derive(Subcommand)]
enum ClipboardCommand {
    /// Show clipboard sync status.
    Status,
    /// Manually push current clipboard to peers.
    Push,
    /// Manually pull clipboard from a peer.
    Pull {
        /// Device ID to pull from.
        device_id: String,
    },
}
```

- [ ] **Step 6: Add REST endpoints**

Add routes in `serve()`:

```rust
.route("/v1/clipboard", get(get_clipboard))
.route("/v1/clipboard/push", post(post_clipboard_push))
.route("/v1/clipboard/pull", post(post_clipboard_pull))
```

- [ ] **Step 7: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add crates/daemon/ crates/cli/ Cargo.toml
git commit -m "feat(daemon): integrate clipboard manager with poll loop and API"
```

---

## Task 6: Final Verification

**Covers:** [S7]

**Files:** None

- [ ] **Step 1: Run all workspace tests**

Run: `cargo test --workspace`
Expected: All tests PASS

- [ ] **Step 2: Run workspace check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace`
Expected: No new warnings

- [ ] **Step 4: Verify CLI compiles**

Run: `cargo run -p hyprbridge-cli -- --help`
Expected: CLI help output with clipboard subcommand

- [ ] **Step 5: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for clipboard sync"
```
