# Batch Transfer Design

## [S1] Problem

HyprBridge file transfer only supports single files. Users need to transfer folders and multiple files at once, preserving directory structure. This is a common use case for cross-device workflows.

## [S2] Solution Overview

Extend FileManager with folder and batch transfer support:
- `send_folder()` for directory transfer with configurable structure preservation
- `send_batch()` for multi-file transfer
- `BatchTransferState` for progress tracking (file count, bytes, current file)
- CLI + REST API + WebSocket progress events

## [S3] FileManager Extension

Extend `crates/filetransfer`:

```rust
pub struct BatchTransferRequest {
    pub id: String,
    pub target_device: DeviceId,
    pub items: Vec<TransferItem>,
    pub preserve_structure: bool,
}

pub enum TransferItem {
    File(PathBuf),
    Folder(PathBuf),
}

pub struct BatchTransferState {
    pub id: String,
    pub total_files: usize,
    pub completed_files: usize,
    pub current_file: Option<String>,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: TransferStatus,
}
```

**Files:** `crates/filetransfer/src/lib.rs`

## [S4] Protocol Extensions

```rust
pub struct BatchTransferHeader {
    pub batch_id: String,
    pub total_files: usize,
    pub total_bytes: u64,
    pub preserve_structure: bool,
    pub root_name: String,
}

pub struct BatchFileHeader {
    pub relative_path: String,
    pub file_size: u64,
    pub content_hash: String,
}
```

**Files:** `crates/protocol/src/lib.rs`

## [S5] Transfer Flow

1. Send `BatchTransferHeader`
2. For each file: `BatchFileHeader` → data chunks → `End`
3. Send `BatchTransferEnd`
4. Receiver validates all file hashes

**Files:** `crates/network/src/lib.rs`

## [S6] CLI Commands

```bash
hyprbridge-cli transfer send --folder <path> --device <id>
hyprbridge-cli transfer send --files <file1> <file2> --device <id>
hyprbridge-cli transfer batch-status <batch_id>
```

**Files:** `crates/cli/src/main.rs`

## [S7] REST API

```bash
POST /v1/transfer/send-folder
POST /v1/transfer/send-batch
GET  /v1/transfer/batch/:id
```

**Files:** `crates/daemon/src/lib.rs`

## [S8] Progress Tracking

- `BatchTransferState` updated after each file
- WebSocket events push progress to frontend
- Frontend shows: total files, completed, current file, progress bar

**Files:** `crates/filetransfer/src/lib.rs`, `packages/ui/src/`

## [S9] Testing Strategy

- Unit tests for directory traversal and structure preservation
- Integration test: send folder between two daemons
- Manual test: CLI folder transfer
- Visual test: progress display in Web UI

## [S10] Non-Goals

- Symbolic link handling (v2)
- File permission preservation (v2)
- Delta/incremental transfer (v2)
- Compression (v2)
- Encryption at rest (already encrypted in transit)
