# Clipboard History Design

## [S1] Problem

HyprBridge syncs clipboard content but has no history. Users cannot access previously copied items, select from multiple clips, or pin important items. This limits productivity for power users.

## [S2] Solution Overview

Configurable clipboard history with pinning:
- `ClipboardHistory` structure stores recent clipboard entries
- Configurable size (default 50) and persistence (optional JSON file)
- Pin/unpin important items
- CLI and Web UI access
- REST API for programmatic access

## [S3] ClipboardHistory Structure

In `crates/clipboard`:

```rust
pub struct ClipboardHistoryEntry {
    pub id: String,
    pub content: ClipboardContent,
    pub timestamp: DateTime<Utc>,
    pub source_device: Option<DeviceId>,
    pub pinned: bool,
}

pub struct ClipboardHistory {
    entries: VecDeque<ClipboardHistoryEntry>,
    max_size: usize,
    persist_path: Option<PathBuf>,
}
```

**Config:**
```toml
[clipboard]
history_size = 50
persist_history = false
history_path = "~/.local/share/hyprbridge/clipboard-history.json"
```

**Files:** `crates/clipboard/src/history.rs`, `crates/config/src/lib.rs`

## [S4] ClipboardManager Integration

Extend `ClipboardManager`:
- `history()` → return history entries
- `pin(id)` → pin entry (not overwritten by new entries)
- `unpin(id)` → unpin entry
- `delete(id)` → delete entry
- `clear()` → clear all history

**Files:** `crates/clipboard/src/lib.rs`

## [S5] REST API

```bash
GET    /v1/clipboard/history
POST   /v1/clipboard/history/pin
POST   /v1/clipboard/history/unpin
DELETE /v1/clipboard/history/:id
POST   /v1/clipboard/history/clear
```

**Files:** `crates/daemon/src/lib.rs`

## [S6] CLI Commands

```bash
hyprbridge-cli clipboard history
hyprbridge-cli clipboard pin <id>
hyprbridge-cli clipboard unpin <id>
hyprbridge-cli clipboard delete <id>
hyprbridge-cli clipboard clear
```

**Files:** `crates/cli/src/main.rs`

## [S7] Web UI

New `ClipboardHistoryPanel` component:
- Entry list with content preview, timestamp, source device
- Pin/unpin/delete buttons per entry
- Click to copy to current clipboard
- Real-time updates via WebSocket

**Files:** `packages/ui/src/clipboard-history-panel.tsx`

## [S8] Testing Strategy

- Unit tests for ClipboardHistory (add, pin, eviction, persistence)
- Integration test: poll clipboard → history grows → pin → clear
- Manual test: CLI history operations
- Visual test: Web UI panel renders correctly

## [S9] Non-Goals

- Rich text preview (v2)
- Image thumbnail preview (v2)
- Search/filter history (v2)
- Cross-device history sync (v2)
- History export/import (v2)
