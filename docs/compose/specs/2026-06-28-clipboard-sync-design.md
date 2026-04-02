# Clipboard Sync Design

## [S1] Problem

HyprBridge currently has no clipboard synchronization between paired devices. Users must manually transfer text, images, and files between machines. This is a high-frequency daily use case that significantly impacts the KVM experience.

## [S2] Solution Overview

Hybrid clipboard sync:
- Text clipboard via `ControlFrame::ClipboardText` (small, fits in existing control streams)
- Images and files via dedicated QUIC streams (large, needs chunked transfer)
- Bidirectional auto-sync with polling-based change detection (500ms interval)
- Content hash deduplication (SHA-256) to avoid re-transferring unchanged content
- Last-writer-wins conflict resolution

## [S3] Protocol Extensions

Add to `crates/protocol/src/lib.rs`:

**New `ControlFrame` variant:**
```rust
ClipboardText { text: String },
```

**New clipboard types:**
```rust
pub enum ClipboardContentType { Text, Image, FileList }

pub struct ClipboardStreamHeader {
    pub content_type: ClipboardContentType,
    pub content_hash: String,
    pub size_bytes: u64,
    pub source_device: DeviceId,
    pub timestamp: DateTime<Utc>,
    pub file_names: Option<Vec<String>>,
}

pub enum ClipboardStreamCommand {
    Begin(ClipboardStreamHeader),
    Chunk(Vec<u8>),
    End,
}
```

**Files:** `crates/protocol/src/lib.rs`

## [S4] ClipboardManager Crate

New crate `crates/clipboard` with platform-specific clipboard access:

**Architecture:**
- `ClipboardManager` struct with poll loop (500ms)
- Platform clipboard read/write via system commands or native APIs
- Content hash computation (SHA-256)
- Conflict resolution (last-writer-wins by timestamp)

**Key API:**
```rust
impl ClipboardManager {
    pub async fn start(transport: QuicTransport, local_device_id: DeviceId, peers: Vec<DeviceId>);
    pub async fn poll_once(&self) -> Option<ClipboardEvent>;
    pub async fn apply_remote(&self, content: ClipboardContent) -> bool;
}
```

**Platform clipboard access:**
- Linux: `xclip` / `xsel` / `wl-paste` / `wl-copy` (auto-detect Wayland vs X11)
- macOS: `pbpaste` / `pbcopy`
- Windows: Win32 `GetClipboardData` / `SetClipboardData` via `windows` crate

**Files:** `crates/clipboard/Cargo.toml`, `crates/clipboard/src/lib.rs`, `crates/clipboard/src/linux.rs`, `crates/clipboard/src/macos.rs`, `crates/clipboard/src/windows.rs`

## [S5] Dedicated QUIC Stream for Binary Data

For images and files, use a dedicated QUIC stream:

**Stream lifecycle:**
1. Sender opens bi-directional QUIC stream
2. Sends `ClipboardStreamCommand::Begin(header)`
3. Sends `ClipboardStreamCommand::Chunk(data)` × N (64KB chunks)
4. Sends `ClipboardStreamCommand::End`
5. Receiver validates hash, applies to local clipboard

**Integration with `QuicTransport`:**
- `open_clipboard_stream(device_id)` — opens a new stream for clipboard data
- Accept loop handles incoming clipboard streams alongside control streams

**Size limits:**
- Text: via `ClipboardText` ControlFrame (~100KB max)
- Images: via dedicated stream (chunked, no hard limit)
- Files: via dedicated stream (metadata in header, content in chunks)

**Files:** `crates/network/src/lib.rs`

## [S6] Daemon Integration

Wire clipboard manager into daemon:

1. Add `clipboard: Arc<ClipboardManager>` to `DaemonState`
2. Spawn poll loop in `serve()` (500ms interval)
3. Handle incoming clipboard streams in QUIC accept loop
4. Add CLI commands: `clipboard status`, `clipboard push`, `clipboard pull`
5. Add REST endpoints: `GET /v1/clipboard`, `POST /v1/clipboard/push`, `POST /v1/clipboard/pull`

**Files:** `crates/daemon/src/lib.rs`, `crates/cli/src/main.rs`

## [S7] Testing Strategy

- Unit tests for `ClipboardManager` (hash computation, conflict resolution)
- Unit tests for protocol types (serialization round-trip)
- Integration test: two daemons syncing text clipboard
- Manual test: copy on device A, paste on device B
- Cross-compilation check on all platforms

## [S8] Non-Goals

- Clipboard history (only current clipboard content)
- Selective sync (sync all or nothing)
- Clipboard encryption at rest (already encrypted in transit via QUIC)
- Rich text / RTF formatting preservation (plain text only for v1)
