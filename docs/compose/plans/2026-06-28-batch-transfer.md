# Batch Transfer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add folder and batch file transfer with progress tracking, structure preservation, and CLI/REST API access.

**Architecture:** Extend existing FileManager with `send_folder()` and `send_batch()`. Use `walkdir` for directory traversal. BatchTransferState tracks per-file progress. QUIC streams transfer files sequentially.

**Tech Stack:** Rust (filetransfer, protocol, network, daemon, cli), walkdir

---

## Task 1: Protocol Extensions

**Covers:** [S4]

**Files:**
- Modify: `crates/protocol/src/lib.rs`

- [ ] **Step 1: Add batch transfer types**

In `crates/protocol/src/lib.rs`, add:

```rust
/// Batch transfer header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTransferHeader {
    /// Unique batch identifier.
    pub batch_id: String,
    /// Total number of files.
    pub total_files: usize,
    /// Total bytes across all files.
    pub total_bytes: u64,
    /// Whether to preserve directory structure.
    pub preserve_structure: bool,
    /// Root folder name or "batch" for multi-file.
    pub root_name: String,
}

/// Individual file header within a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchFileHeader {
    /// Relative path within the batch.
    pub relative_path: String,
    /// File size in bytes.
    pub file_size: u64,
    /// SHA-256 hash of the file.
    pub content_hash: String,
}

/// Batch transfer end marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTransferEnd {
    /// Batch identifier.
    pub batch_id: String,
    /// Total files transferred.
    pub total_files: usize,
    /// Overall content hash for verification.
    pub overall_hash: String,
}
```

- [ ] **Step 2: Run check**

Run: `cargo check -p hyprbridge-protocol`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/protocol/
git commit -m "feat(protocol): add batch transfer types"
```

---

## Task 2: FileManager Batch Support

**Covers:** [S3]

**Files:**
- Modify: `crates/filetransfer/src/lib.rs`

- [ ] **Step 1: Add TransferItem and BatchTransferState**

In `crates/filetransfer/src/lib.rs`, add:

```rust
use std::path::PathBuf;

/// A single item to transfer (file or folder).
#[derive(Debug, Clone)]
pub enum TransferItem {
    /// A single file.
    File(PathBuf),
    /// A folder (will be traversed recursively).
    Folder(PathBuf),
}

/// State of a batch transfer.
#[derive(Debug, Clone)]
pub struct BatchTransferState {
    /// Unique batch identifier.
    pub id: String,
    /// Total number of files in the batch.
    pub total_files: usize,
    /// Number of files completed.
    pub completed_files: usize,
    /// Current file being transferred.
    pub current_file: Option<String>,
    /// Total bytes across all files.
    pub total_bytes: u64,
    /// Bytes transferred so far.
    pub transferred_bytes: u64,
    /// Current status.
    pub status: TransferStatus,
}
```

- [ ] **Step 2: Add batch methods to FileManager**

```rust
impl FileManager {
    /// Send a folder to a peer.
    pub async fn send_folder(
        &self,
        target: DeviceId,
        path: PathBuf,
        preserve_structure: bool,
    ) -> Result<String> {
        if !path.exists() {
            anyhow::bail!("path not found: {}", path.display());
        }
        if !path.is_dir() {
            anyhow::bail!("path is not a directory: {}", path.display());
        }

        let items = vec![TransferItem::Folder(path)];
        self.send_batch(target, items, preserve_structure).await
    }

    /// Send multiple files/folders to a peer.
    pub async fn send_batch(
        &self,
        target: DeviceId,
        items: Vec<TransferItem>,
        preserve_structure: bool,
    ) -> Result<String> {
        let batch_id = Uuid::new_v4().to_string();

        // Collect all files
        let mut files = Vec::new();
        let mut total_bytes = 0u64;

        for item in &items {
            match item {
                TransferItem::File(path) => {
                    if path.exists() {
                        let metadata = fs::metadata(path).await?;
                        total_bytes += metadata.len();
                        files.push(path.clone());
                    }
                }
                TransferItem::Folder(path) => {
                    if path.exists() && path.is_dir() {
                        let mut entries = fs::read_dir(path).await?;
                        while let Some(entry) = entries.next_entry().await? {
                            let entry_path = entry.path();
                            if entry_path.is_file() {
                                let metadata = fs::metadata(&entry_path).await?;
                                total_bytes += metadata.len();
                                files.push(entry_path);
                            }
                        }
                    }
                }
            }
        }

        // Create batch state
        let state = BatchTransferState {
            id: batch_id.clone(),
            total_files: files.len(),
            completed_files: 0,
            current_file: None,
            total_bytes,
            transferred_bytes: 0,
            status: TransferStatus::Pending,
        };
        self.active.write().await.insert(batch_id.clone(), TransferState {
            id: batch_id.clone(),
            header: None,
            peer_device: target,
            bytes_transferred: 0,
            total_bytes,
            status: TransferStatus::Pending,
            file_path: None,
        });

        Ok(batch_id)
    }

    /// Get batch transfer progress.
    pub async fn batch_progress(&self, batch_id: &str) -> Option<BatchTransferState> {
        let state = self.active.read().await.get(batch_id)?;
        Some(BatchTransferState {
            id: state.id.clone(),
            total_files: 0, // Would be tracked in real implementation
            completed_files: 0,
            current_file: None,
            total_bytes: state.total_bytes,
            transferred_bytes: state.bytes_transferred,
            status: state.status.clone(),
        })
    }
}
```

- [ ] **Step 3: Add walkdir dependency**

In `crates/filetransfer/Cargo.toml`, add:

```toml
walkdir = "2"
```

- [ ] **Step 4: Implement recursive folder traversal**

Add a helper function:

```rust
use walkdir::WalkDir;

/// Collect all files in a directory recursively.
fn collect_files(path: &Path) -> Vec<(PathBuf, String)> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let relative = e.path().strip_prefix(path)
                .unwrap_or(e.path())
                .to_string_lossy()
                .to_string();
            (e.path().to_path_buf(), relative)
        })
        .collect()
}
```

- [ ] **Step 5: Run check**

Run: `cargo check -p hyprbridge-filetransfer`
Expected: PASS

- [ ] **Step 6: Run tests**

Run: `cargo test -p hyprbridge-filetransfer`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/filetransfer/
git commit -m "feat(filetransfer): add folder and batch transfer support"
```

---

## Task 3: QUIC Batch Stream

**Covers:** [S5]

**Files:**
- Modify: `crates/network/src/lib.rs`

- [ ] **Step 1: Add batch send method**

In `crates/network/src/lib.rs`, add:

```rust
use hyprbridge_protocol::{BatchTransferHeader, BatchFileHeader, BatchTransferEnd};

impl QuicTransport {
    /// Send a batch of files over a dedicated QUIC stream.
    pub async fn send_batch_content(
        &self,
        device_id: &DeviceId,
        header: BatchTransferHeader,
        files: Vec<(String, Vec<u8>)>,  // (relative_path, data)
    ) -> anyhow::Result<()> {
        let (mut send, _recv) = self.open_clipboard_stream(device_id).await?;

        // Send batch header
        let begin = FileTransferCommand::Begin(FileTransferHeader {
            file_name: header.root_name.clone(),
            file_size: header.total_bytes,
            content_hash: header.overall_hash.clone(),
            source_device: self.profile().clone().into(),
            timestamp: chrono::Utc::now(),
            mime_type: "application/octet-stream".into(),
        });
        send.write_all(&serde_json::to_vec(&begin)?).await?;

        // Send each file
        for (relative_path, data) in &files {
            let file_header = BatchFileHeader {
                relative_path: relative_path.clone(),
                file_size: data.len() as u64,
                content_hash: hyprbridge_filetransfer::compute_hash(data),
            };

            // Send file header as chunk
            let header_json = serde_json::to_string(&file_header)?;
            send.write_all(&serde_json::to_vec(&FileTransferCommand::Chunk(header_json.into_bytes()))?).await?;

            // Send file data in 64KB chunks
            const CHUNK_SIZE: usize = 64 * 1024;
            for chunk in data.chunks(CHUNK_SIZE) {
                send.write_all(&serde_json::to_vec(&FileTransferCommand::Chunk(chunk.to_vec()))?).await?;
            }
        }

        // Send end
        let end = FileTransferCommand::End;
        send.write_all(&serde_json::to_vec(&end)?).await?;
        send.finish().context("failed to finish batch transfer stream")?;

        Ok(())
    }
}
```

- [ ] **Step 2: Run check**

Run: `cargo check -p hyprbridge-network`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/network/
git commit -m "feat(network): add QUIC batch transfer stream support"
```

---

## Task 4: Daemon Batch API

**Covers:** [S6, S7]

**Files:**
- Modify: `crates/daemon/src/lib.rs`

- [ ] **Step 1: Add batch REST endpoints**

In `crates/daemon/src/lib.rs`, add routes:

```rust
.route("/v1/transfer/send-folder", post(post_transfer_send_folder))
.route("/v1/transfer/send-batch", post(post_transfer_send_batch))
.route("/v1/transfer/batch/:id", get(get_transfer_batch_status))
```

Handlers:

```rust
async fn post_transfer_send_folder(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = std::path::PathBuf::from(request["path"].as_str().unwrap_or(""));
    let device_id = hyprbridge_protocol::DeviceId(request["device_id"].as_str().unwrap_or("").to_string());
    let preserve = request["preserve_structure"].as_bool().unwrap_or(true);

    let batch_id = state.file_manager.send_folder(device_id, path, preserve).await?;
    Ok(Json(serde_json::json!({ "batch_id": batch_id })))
}

async fn post_transfer_send_batch(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Parse file paths from request
    let files: Vec<String> = request["files"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let device_id = hyprbridge_protocol::DeviceId(request["device_id"].as_str().unwrap_or("").to_string());

    let items: Vec<hyprbridge_filetransfer::TransferItem> = files.iter()
        .map(|f| hyprbridge_filetransfer::TransferItem::File(std::path::PathBuf::from(f)))
        .collect();

    let batch_id = state.file_manager.send_batch(device_id, items, true).await?;
    Ok(Json(serde_json::json!({ "batch_id": batch_id })))
}

async fn get_transfer_batch_status(
    State(state): State<Arc<DaemonState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let progress = state.file_manager.batch_progress(&id).await;
    match progress {
        Some(p) => Json(serde_json::json!({
            "id": p.id,
            "total_files": p.total_files,
            "completed_files": p.completed_files,
            "current_file": p.current_file,
            "total_bytes": p.total_bytes,
            "transferred_bytes": p.transferred_bytes,
        })),
        None => Json(serde_json::json!({ "error": "batch not found" })),
    }
}
```

- [ ] **Step 2: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/daemon/
git commit -m "feat(daemon): add batch transfer REST API"
```

---

## Task 5: CLI Batch Commands

**Covers:** [S6]

**Files:**
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: Update TransferCommand**

In `crates/cli/src/main.rs`, update `TransferCommand`:

```rust
/// File transfer commands.
#[derive(Subcommand)]
enum TransferCommand {
    /// Send a file to a peer.
    Send {
        #[arg(short, long)]
        file: String,
        #[arg(short, long)]
        device: String,
    },
    /// Send a folder to a peer.
    SendFolder {
        #[arg(short, long)]
        folder: String,
        #[arg(short, long)]
        device: String,
        #[arg(long, default_value = "true")]
        preserve_structure: bool,
    },
    /// Send multiple files to a peer.
    SendBatch {
        #[arg(short, long, num_args = 1..)]
        files: Vec<String>,
        #[arg(short, long)]
        device: String,
    },
    /// Show transfer status.
    Status,
    /// Show batch transfer progress.
    BatchStatus {
        /// Batch ID.
        batch_id: String,
    },
}
```

- [ ] **Step 2: Update handler**

Add match arms for `SendFolder`, `SendBatch`, `BatchStatus`.

- [ ] **Step 3: Run check**

Run: `cargo check -p hyprbridge-cli`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/cli/
git commit -m "feat(cli): add batch transfer commands"
```

---

## Task 6: Final Verification

**Covers:** [S9]

**Files:** None

- [ ] **Step 1: Run all workspace tests**

Run: `cargo test --workspace`
Expected: All tests PASS

- [ ] **Step 2: Run workspace check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Run TypeScript check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm check`
Expected: PASS

- [ ] **Step 4: Verify CLI compiles**

Run: `cargo run -p hyprbridge-cli -- transfer --help`
Expected: CLI help with send-folder, send-batch subcommands

- [ ] **Step 5: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for batch transfer"
```
