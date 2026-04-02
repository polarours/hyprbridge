# File Transfer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement streaming file transfer between paired devices via dedicated QUIC streams with progress tracking and integrity verification.

**Architecture:** New `FileManager` crate orchestrates transfers. Dedicated QUIC streams per file (same pattern as clipboard). Metadata-first protocol: header → chunks → end → ack. SHA-256 integrity verification. CLI + REST API for v1.

**Tech Stack:** Rust, SHA-256, existing QUIC transport, tokio, serde

---

## Task 1: Protocol Extensions

**Covers:** [S3]

**Files:**
- Modify: `crates/protocol/src/lib.rs`

- [ ] **Step 1: Add FileTransferHeader**

In `crates/protocol/src/lib.rs`, add:

```rust
/// File transfer metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferHeader {
    /// Original file name.
    pub file_name: String,
    /// Total file size in bytes.
    pub file_size: u64,
    /// SHA-256 hash of the full file.
    pub content_hash: String,
    /// Device that initiated the transfer.
    pub source_device: DeviceId,
    /// When the transfer was initiated.
    pub timestamp: DateTime<Utc>,
    /// MIME type of the file.
    pub mime_type: String,
}
```

- [ ] **Step 2: Add FileTransferCommand**

```rust
/// Command sent over a dedicated file transfer QUIC stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileTransferCommand {
    /// Begin a file transfer stream.
    Begin(FileTransferHeader),
    /// Data chunk (up to 64KB).
    Chunk(Vec<u8>),
    /// End of file data.
    End,
    /// Acknowledgement from receiver.
    Ack { accepted: bool, detail: String },
}
```

- [ ] **Step 3: Add FileSendRequest and FileSendResponse**

```rust
/// Request to send a file to a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSendRequest {
    /// Target device ID.
    pub device_id: DeviceId,
    /// Local file path to send.
    pub file_path: String,
}

/// Response after initiating a file send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSendResponse {
    /// Unique transfer identifier.
    pub transfer_id: String,
    /// Transfer status.
    pub status: String,
}
```

- [ ] **Step 4: Add FileTransferNotify to ControlFrame**

In the `ControlFrame` enum, add:

```rust
/// Notify peer of incoming file transfer.
FileTransferNotify { header: FileTransferHeader },
```

- [ ] **Step 5: Run check**

Run: `cargo check -p hyprbridge-protocol`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/protocol/
git commit -m "feat(protocol): add file transfer types and ControlFrame variant"
```

---

## Task 2: FileManager Crate Scaffolding

**Covers:** [S4]

**Files:**
- Create: `crates/filetransfer/Cargo.toml`
- Create: `crates/filetransfer/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Add filetransfer to workspace members**

In root `Cargo.toml`, add `"crates/filetransfer"` to the `members` list.

- [ ] **Step 2: Create Cargo.toml**

Create `crates/filetransfer/Cargo.toml`:

```toml
[package]
name = "hyprbridge-filetransfer"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
anyhow.workspace = true
chrono.workspace = true
hex = "0.4.3"
hyprbridge-protocol = { path = "../protocol" }
sha2.workspace = true
tokio = { workspace = true, features = ["fs", "io-util"] }
tracing.workspace = true
uuid.workspace = true
```

- [ ] **Step 3: Create lib.rs**

Create `crates/filetransfer/src/lib.rs`:

```rust
//! File transfer between paired devices.

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use hyprbridge_protocol::{DeviceId, FileTransferHeader};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Status of a file transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferStatus {
    /// Transfer is queued but not started.
    Pending,
    /// Transfer is actively in progress.
    InProgress,
    /// Transfer completed successfully.
    Completed,
    /// Transfer failed with an error.
    Failed(String),
}

/// State of a single file transfer.
#[derive(Debug, Clone)]
pub struct TransferState {
    /// Unique transfer identifier.
    pub id: String,
    /// Transfer metadata.
    pub header: Option<FileTransferHeader>,
    /// Target device (for outbound) or source device (for inbound).
    pub peer_device: DeviceId,
    /// Bytes transferred so far.
    pub bytes_transferred: u64,
    /// Total file size.
    pub total_bytes: u64,
    /// Current status.
    pub status: TransferStatus,
    /// Local file path (source for outbound, destination for inbound).
    pub file_path: Option<PathBuf>,
}

/// A pending outbound file transfer request.
#[derive(Debug, Clone)]
pub struct TransferRequest {
    /// Unique transfer identifier.
    pub id: String,
    /// Target device.
    pub target_device: DeviceId,
    /// Local file path to send.
    pub file_path: PathBuf,
}

/// Manages file transfers between paired devices.
pub struct FileManager {
    /// Pending outbound transfers.
    queue: RwLock<Vec<TransferRequest>>,
    /// Active transfers (both inbound and outbound).
    active: RwLock<HashMap<String, TransferState>>,
    /// Directory for received files.
    download_dir: PathBuf,
}

impl FileManager {
    /// Create a new file manager.
    pub fn new(download_dir: PathBuf) -> Self {
        Self {
            queue: RwLock::new(Vec::new()),
            active: RwLock::new(HashMap::new()),
            download_dir,
        }
    }

    /// Enqueue a file for sending to a peer.
    pub async fn send_file(
        &self,
        target: DeviceId,
        path: PathBuf,
    ) -> anyhow::Result<String> {
        if !path.exists() {
            anyhow::bail!("file not found: {}", path.display());
        }

        let id = Uuid::new_v4().to_string();
        let request = TransferRequest {
            id: id.clone(),
            target_device: target,
            file_path: path,
        };
        self.queue.write().await.push(request);

        let state = TransferState {
            id: id.clone(),
            header: None,
            peer_device: target,
            bytes_transferred: 0,
            total_bytes: 0,
            status: TransferStatus::Pending,
            file_path: None,
        };
        self.active.write().await.insert(id.clone(), state);

        Ok(id)
    }

    /// Build a transfer header for a file.
    pub async fn build_header(&self, path: &Path, source_device: &DeviceId) -> anyhow::Result<FileTransferHeader> {
        let data = fs::read(path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = hex::encode(hasher.finalize());

        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        Ok(FileTransferHeader {
            file_name,
            file_size: data.len() as u64,
            content_hash: hash,
            source_device: source_device.clone(),
            timestamp: Utc::now(),
            mime_type: mime,
        })
    }

    /// Receive a file from a peer.
    pub async fn receive_file(
        &self,
        header: FileTransferHeader,
        data: Vec<u8>,
    ) -> anyhow::Result<PathBuf> {
        // Verify hash
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = hex::encode(hasher.finalize());
        if hash != header.content_hash {
            anyhow::bail!("hash mismatch: expected {}, got {}", header.content_hash, hash);
        }

        // Create download directory
        fs::create_dir_all(&self.download_dir).await?;

        // Write file
        let file_path = self.download_dir.join(&header.file_name);
        fs::write(&file_path, &data).await?;

        // Track transfer
        let id = Uuid::new_v4().to_string();
        let state = TransferState {
            id: id.clone(),
            header: Some(header.clone()),
            peer_device: header.source_device,
            bytes_transferred: data.len() as u64,
            total_bytes: data.len() as u64,
            status: TransferStatus::Completed,
            file_path: Some(file_path.clone()),
        };
        self.active.write().await.insert(id, state);

        Ok(file_path)
    }

    /// Get the status of a transfer.
    pub async fn progress(&self, transfer_id: &str) -> Option<TransferState> {
        self.active.read().await.get(transfer_id).cloned()
    }

    /// List all transfers.
    pub async fn list_transfers(&self) -> Vec<TransferState> {
        self.active.read().await.values().cloned().collect()
    }

    /// Get the download directory.
    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }
}

/// Compute SHA-256 hash of data.
pub fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Guess MIME type from file path.
pub fn guess_mime(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        let data = b"hello world";
        let h1 = compute_hash(data);
        let h2 = compute_hash(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_data_different_hash() {
        let h1 = compute_hash(b"hello");
        let h2 = compute_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn guess_mime_txt() {
        assert_eq!(guess_mime(Path::new("test.txt")), "text/plain");
    }

    #[test]
    fn guess_mime_png() {
        assert_eq!(guess_mime(Path::new("image.png")), "image/png");
    }
}
```

- [ ] **Step 4: Add mime_guess dependency**

In `crates/filetransfer/Cargo.toml`, add:

```toml
mime_guess = "2.0"
```

- [ ] **Step 5: Run check**

Run: `cargo check -p hyprbridge-filetransfer`
Expected: PASS

- [ ] **Step 6: Run tests**

Run: `cargo test -p hyprbridge-filetransfer`
Expected: 4 tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/filetransfer/ Cargo.toml
git commit -m "feat(filetransfer): add FileManager crate with hash and MIME detection"
```

---

## Task 3: QUIC File Transfer Stream Integration

**Covers:** [S5]

**Files:**
- Modify: `crates/network/src/lib.rs`

- [ ] **Step 1: Add file transfer stream methods to QuicTransport**

In `crates/network/src/lib.rs`, add:

```rust
use hyprbridge_protocol::FileTransferCommand;

impl QuicTransport {
    /// Send a file over a dedicated QUIC stream.
    pub async fn send_file_content(
        &self,
        device_id: &DeviceId,
        header: hyprbridge_protocol::FileTransferHeader,
        data: &[u8],
    ) -> anyhow::Result<()> {
        let (mut send, _recv) = self.open_clipboard_stream(device_id).await?;

        // Send begin
        let begin = FileTransferCommand::Begin(header);
        send.write_all(&serde_json::to_vec(&begin)?).await?;

        // Send chunks (64KB each)
        const CHUNK_SIZE: usize = 64 * 1024;
        for chunk in data.chunks(CHUNK_SIZE) {
            let cmd = FileTransferCommand::Chunk(chunk.to_vec());
            send.write_all(&serde_json::to_vec(&cmd)?).await?;
        }

        // Send end
        let end = FileTransferCommand::End;
        send.write_all(&serde_json::to_vec(&end)?).await?;
        send.finish().context("failed to finish file transfer stream")?;

        Ok(())
    }
}
```

- [ ] **Step 2: Add file transfer stream accept handler**

In the `handle_incoming_connection` function, add detection for `FileTransferCommand::Begin` alongside the existing `ClipboardStreamCommand::Begin` detection.

- [ ] **Step 3: Run check**

Run: `cargo check -p hyprbridge-network`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/network/
git commit -m "feat(network): add QUIC file transfer stream support"
```

---

## Task 4: Daemon Integration

**Covers:** [S7]

**Files:**
- Modify: `crates/daemon/src/lib.rs`
- Modify: `crates/daemon/Cargo.toml`
- Modify: `crates/cli/src/main.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add filetransfer dependency to daemon**

In `crates/daemon/Cargo.toml`, add:

```toml
hyprbridge-filetransfer = { path = "../filetransfer" }
```

- [ ] **Step 2: Add FileManager to DaemonState**

In `crates/daemon/src/lib.rs`:

```rust
use hyprbridge_filetransfer::FileManager;

pub struct DaemonState {
    // ... existing fields ...
    file_manager: Arc<FileManager>,
}
```

Initialize in `DaemonState::new`:

```rust
let download_dir = dirs::download_dir()
    .unwrap_or_else(|| std::env::temp_dir())
    .join("HyprBridge");
let file_manager = Arc::new(FileManager::new(download_dir));
```

- [ ] **Step 3: Handle FileTransferNotify in DaemonControlHandler**

Add to the `handle` method:

```rust
ControlFrame::FileTransferNotify { header } => {
    tracing::info!(
        "incoming file transfer from {}: {} ({} bytes)",
        peer.device_id.0,
        header.file_name,
        header.file_size
    );
    ControlAck {
        accepted: true,
        detail: format!("file transfer notification received for {}", header.file_name),
    }
}
```

- [ ] **Step 4: Add CLI transfer commands**

In `crates/cli/src/main.rs`, add:

```rust
/// File transfer commands.
#[derive(Subcommand)]
enum TransferCommand {
    /// Send a file to a peer.
    Send {
        /// File path to send.
        #[arg(short, long)]
        file: String,
        /// Target device ID.
        #[arg(short, long)]
        device: String,
    },
    /// Show transfer status.
    Status,
}
```

Add to main `Commands` enum and handler.

- [ ] **Step 5: Add REST endpoints**

```rust
.route("/v1/transfer/send", post(post_transfer_send))
.route("/v1/transfer/status", get(get_transfer_status))
```

- [ ] **Step 6: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/daemon/ crates/cli/ Cargo.toml
git commit -m "feat(daemon): integrate FileManager with REST API and CLI"
```

---

## Task 5: Final Verification

**Covers:** [S8]

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
Expected: CLI help with transfer subcommand

- [ ] **Step 5: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for file transfer"
```
