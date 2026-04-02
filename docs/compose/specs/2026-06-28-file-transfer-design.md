# File Transfer Design

## [S1] Problem

HyprBridge has no file transfer capability between paired devices. Users need to move files across machines on the same network. This is a core KVM feature that complements clipboard sync.

## [S2] Solution Overview

Dedicated QUIC stream-based file transfer:
- New `FileManager` crate handles file I/O, progress tracking, transfer queue
- Dedicated QUIC streams per file (streaming, no size limit)
- Metadata-first protocol: header → chunks → end → ack
- SHA-256 integrity verification
- Auto-accept from trusted peers, save to `~/Downloads/HyprBridge/`
- CLI + REST API for v1, drag-to-edge deferred

## [S3] Protocol Extensions

Add to `crates/protocol/src/lib.rs`:

```rust
pub struct FileTransferHeader {
    pub file_name: String,
    pub file_size: u64,
    pub content_hash: String,
    pub source_device: DeviceId,
    pub timestamp: DateTime<Utc>,
    pub mime_type: String,
}

pub enum FileTransferCommand {
    Begin(FileTransferHeader),
    Chunk(Vec<u8>),
    End,
    Ack { accepted: bool, detail: String },
}

pub struct FileSendRequest {
    pub device_id: DeviceId,
    pub file_path: String,
}

pub struct FileSendResponse {
    pub transfer_id: String,
    pub status: String,
}
```

**New `ControlFrame` variant:**
```rust
FileTransferNotify { header: FileTransferHeader },
```

**Files:** `crates/protocol/src/lib.rs`

## [S4] FileManager Crate

New crate `crates/filetransfer`:

**Key types:**
- `FileManager` — orchestrates transfers, manages queue and active transfers
- `TransferState` — progress tracking per transfer
- `TransferRequest` — outbound file request
- `TransferStatus` — Pending/InProgress/Completed/Failed

**Key API:**
```rust
impl FileManager {
    pub fn new(download_dir: PathBuf) -> Self;
    pub async fn send_file(&self, target: DeviceId, path: PathBuf) -> Result<String>;
    pub async fn receive_file(&self, header: FileTransferHeader, data: Vec<u8>) -> Result<PathBuf>;
    pub async fn progress(&self, transfer_id: &str) -> Option<TransferState>;
    pub async fn list_transfers(&self) -> Vec<TransferState>;
}
```

**File reception:** Auto-accept from trusted peers, save to `~/Downloads/HyprBridge/`, verify SHA-256.

**Files:** `crates/filetransfer/Cargo.toml`, `crates/filetransfer/src/lib.rs`

## [S5] QUIC Stream Integration

Same pattern as clipboard sync:

**Outbound:** Open stream → Begin(header) → Chunk × N → End → wait for Ack
**Inbound:** Detect Begin → create file → receive chunks → write → validate hash → send Ack

Progress: Update `bytes_sent` after each chunk, expose via REST/CLI.

**Files:** `crates/network/src/lib.rs`

## [S6] Drag-to-Edge (Deferred)

Native DnD protocol integration deferred to v2. V1 uses:
- CLI: `hyprbridge-cli transfer send --file <path> --device <id>`
- REST: `POST /v1/transfer/send`
- Future: X11 DnD, Wayland data_device, macOS NSFilenamesPboard, Windows WM_DROPFILES

## [S7] Daemon Integration

1. Add `FileManager` to `DaemonState`
2. REST endpoints: `POST /v1/transfer/send`, `GET /v1/transfer/status`, `GET /v1/transfer/{id}`
3. CLI: `transfer send|status|receive`
4. QUIC stream handler for file transfers

**Files:** `crates/daemon/src/lib.rs`, `crates/cli/src/main.rs`

## [S8] Testing Strategy

- Unit tests for FileManager (hash, progress, queue)
- Integration test: send file between two daemons
- Manual test: CLI send/receive on same machine
- Cross-compilation check

## [S9] Non-Goals

- Drag-to-edge file drops (v2)
- Folder/directory transfer (v2)
- Resumable transfers (v2)
- File encryption at rest (already encrypted in transit via QUIC)
- Transfer history (v2)
