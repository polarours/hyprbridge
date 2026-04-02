# Clipboard History Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add configurable clipboard history with pinning, CLI access, and Web UI panel.

**Architecture:** `ClipboardHistory` in clipboard crate stores recent entries with configurable size and optional persistence. Extends existing `ClipboardManager`. CLI + REST API + Web UI for access.

**Tech Stack:** Rust (clipboard, daemon, cli), React (UI)

---

## Task 1: ClipboardHistory Structure

**Covers:** [S3]

**Files:**
- Create: `crates/clipboard/src/history.rs`
- Modify: `crates/clipboard/src/lib.rs`

- [ ] **Step 1: Create history.rs**

Create `crates/clipboard/src/history.rs`:

```rust
//! Clipboard history management.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use hyprbridge_protocol::{ClipboardContentType, DeviceId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ClipboardContent;

/// A single clipboard history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardHistoryEntry {
    /// Unique entry identifier.
    pub id: String,
    /// Content type.
    pub content_type: ClipboardContentType,
    /// Content preview (first 200 chars for text, filename for files).
    pub preview: String,
    /// Full content hash for dedup.
    pub content_hash: String,
    /// When this entry was created.
    pub timestamp: DateTime<Utc>,
    /// Source device ID (None for local clipboard).
    pub source_device: Option<DeviceId>,
    /// Whether this entry is pinned (protected from eviction).
    pub pinned: bool,
}

/// Configuration for clipboard history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardHistoryConfig {
    /// Maximum number of entries to keep.
    pub max_size: usize,
    /// Whether to persist history to disk.
    pub persist: bool,
    /// Path to persist history file.
    pub persist_path: Option<String>,
}

impl Default for ClipboardHistoryConfig {
    fn default() -> Self {
        Self {
            max_size: 50,
            persist: false,
            persist_path: None,
        }
    }
}

/// Manages clipboard history.
pub struct ClipboardHistory {
    entries: VecDeque<ClipboardHistoryEntry>,
    config: ClipboardHistoryConfig,
}

impl ClipboardHistory {
    /// Create a new clipboard history manager.
    pub fn new(config: ClipboardHistoryConfig) -> Self {
        let mut history = Self {
            entries: VecDeque::new(),
            config,
        };

        if history.config.persist {
            if let Some(path) = &history.config.persist_path {
                let _ = history.load_from_disk(Path::new(path));
            }
        }

        history
    }

    /// Add a new entry to the history.
    pub fn add(&mut self, content: &ClipboardContent, source_device: Option<DeviceId>) -> String {
        let hash = content.hash();

        // Skip if same as most recent entry
        if let Some(latest) = self.entries.front() {
            if latest.content_hash == hash {
                return latest.id.clone();
            }
        }

        let preview = match content.content_type {
            ClipboardContentType::Text => {
                let text = String::from_utf8_lossy(&content.data);
                if text.len() > 200 {
                    format!("{}...", &text[..200])
                } else {
                    text.to_string()
                }
            }
            ClipboardContentType::Image => "[Image]".into(),
            ClipboardContentType::FileList => {
                String::from_utf8_lossy(&content.data)
                    .lines()
                    .next()
                    .unwrap_or("[Files]")
                    .to_string()
            }
        };

        let entry = ClipboardHistoryEntry {
            id: Uuid::new_v4().to_string(),
            content_type: content.content_type.clone(),
            preview,
            content_hash: hash,
            timestamp: Utc::now(),
            source_device,
            pinned: false,
        };

        let id = entry.id.clone();
        self.entries.push_front(entry);

        // Evict unpinned entries if over limit
        while self.entries.len() > self.config.max_size {
            if let Some(last) = self.entries.back() {
                if last.pinned {
                    break;
                }
                self.entries.pop_back();
            }
        }

        self.save_if_persistent();
        id
    }

    /// Get all entries.
    pub fn entries(&self) -> Vec<ClipboardHistoryEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Get entry by ID.
    pub fn get(&self, id: &str) -> Option<ClipboardHistoryEntry> {
        self.entries.iter().find(|e| e.id == id).cloned()
    }

    /// Pin an entry.
    pub fn pin(&mut self, id: &str) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.pinned = true;
            self.save_if_persistent();
            true
        } else {
            false
        }
    }

    /// Unpin an entry.
    pub fn unpin(&mut self, id: &str) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.pinned = false;
            self.save_if_persistent();
            true
        } else {
            false
        }
    }

    /// Delete an entry.
    pub fn delete(&mut self, id: &str) -> bool {
        let len = self.entries.len();
        self.entries.retain(|e| e.id != id);
        if self.entries.len() < len {
            self.save_if_persistent();
            true
        } else {
            false
        }
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.save_if_persistent();
    }

    /// Get entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn save_if_persistent(&self) {
        if self.config.persist {
            if let Some(path) = &self.config.persist_path {
                let _ = self.save_to_disk(Path::new(path));
            }
        }
    }

    fn save_to_disk(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(path, json)
    }

    fn load_from_disk(&mut self, path: &Path) -> std::io::Result<()> {
        if path.exists() {
            let json = std::fs::read_to_string(path)?;
            let entries: VecDeque<ClipboardHistoryEntry> = serde_json::from_str(&json)?;
            self.entries = entries;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_content(text: &str) -> ClipboardContent {
        ClipboardContent {
            content_type: ClipboardContentType::Text,
            data: text.as_bytes().to_vec(),
            file_names: None,
        }
    }

    #[test]
    fn add_entry_to_history() {
        let mut history = ClipboardHistory::new(ClipboardHistoryConfig::default());
        let id = history.add(&text_content("hello"), None);
        assert_eq!(history.len(), 1);
        assert!(history.get(&id).is_some());
    }

    #[test]
    fn duplicate_content_not_added() {
        let mut history = ClipboardHistory::new(ClipboardHistoryConfig::default());
        history.add(&text_content("hello"), None);
        history.add(&text_content("hello"), None);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn evicts_oldest_unpinned() {
        let config = ClipboardHistoryConfig { max_size: 2, ..Default::default() };
        let mut history = ClipboardHistory::new(config);
        history.add(&text_content("a"), None);
        history.add(&text_content("b"), None);
        history.add(&text_content("c"), None);
        assert_eq!(history.len(), 2);
        assert!(history.entries()[0].preview == "c");
        assert!(history.entries()[1].preview == "b");
    }

    #[test]
    fn pinned_entries_not_evicted() {
        let config = ClipboardHistoryConfig { max_size: 2, ..Default::default() };
        let mut history = ClipboardHistory::new(config);
        let id = history.add(&text_content("a"), None);
        history.pin(&id);
        history.add(&text_content("b"), None);
        history.add(&text_content("c"), None);
        assert_eq!(history.len(), 2);
        assert!(history.entries()[0].preview == "c");
        assert!(history.entries()[1].preview == "a");
    }

    #[test]
    fn delete_entry() {
        let mut history = ClipboardHistory::new(ClipboardHistoryConfig::default());
        let id = history.add(&text_content("hello"), None);
        assert!(history.delete(&id));
        assert!(history.is_empty());
    }

    #[test]
    fn clear_all() {
        let mut history = ClipboardHistory::new(ClipboardHistoryConfig::default());
        history.add(&text_content("a"), None);
        history.add(&text_content("b"), None);
        history.clear();
        assert!(history.is_empty());
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

In `crates/clipboard/src/lib.rs`, add:

```rust
pub mod history;
pub use history::{ClipboardHistory, ClipboardHistoryConfig, ClipboardHistoryEntry};
```

- [ ] **Step 3: Run check**

Run: `cargo check -p hyprbridge-clipboard`
Expected: PASS

- [ ] **Step 4: Run tests**

Run: `cargo test -p hyprbridge-clipboard`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/clipboard/
git commit -m "feat(clipboard): add ClipboardHistory with pinning and eviction"
```

---

## Task 2: Config Extension

**Covers:** [S3]

**Files:**
- Modify: `crates/config/src/lib.rs`

- [ ] **Step 1: Add clipboard config**

In `crates/config/src/lib.rs`, add to `AppConfig`:

```rust
/// Clipboard configuration.
#[serde(default)]
pub clipboard: ClipboardConfig,

/// Clipboard settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardConfig {
    /// Maximum history entries.
    pub history_size: usize,
    /// Whether to persist history.
    pub persist_history: bool,
    /// History file path.
    pub history_path: Option<String>,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            history_size: 50,
            persist_history: false,
            history_path: None,
        }
    }
}
```

- [ ] **Step 2: Run check**

Run: `cargo check -p hyprbridge-config`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/config/
git commit -m "feat(config): add clipboard history configuration"
```

---

## Task 3: Daemon Integration

**Covers:** [S5]

**Files:**
- Modify: `crates/daemon/src/lib.rs`

- [ ] **Step 1: Add history state to DaemonState**

In `crates/daemon/src/lib.rs`:

```rust
use hyprbridge_clipboard::{ClipboardHistory, ClipboardHistoryConfig};

pub struct DaemonState {
    // ... existing fields ...
    clipboard_history: Arc<RwLock<ClipboardHistory>>,
}
```

Initialize in `DaemonState::new`:

```rust
let history_config = ClipboardHistoryConfig {
    max_size: config.clipboard.history_size,
    persist: config.clipboard.persist_history,
    persist_path: config.clipboard.history_path.clone(),
};
let clipboard_history = Arc::new(RwLock::new(ClipboardHistory::new(history_config)));
```

- [ ] **Step 2: Add history REST endpoints**

Add routes:

```rust
.route("/v1/clipboard/history", get(get_clipboard_history))
.route("/v1/clipboard/history/pin", post(post_clipboard_pin))
.route("/v1/clipboard/history/unpin", post(post_clipboard_unpin))
.route("/v1/clipboard/history/clear", post(post_clipboard_clear))
```

Handlers:

```rust
async fn get_clipboard_history(
    State(state): State<Arc<DaemonState>>,
) -> Json<serde_json::Value> {
    let history = state.clipboard_history.read().await;
    let entries: Vec<_> = history.entries().into_iter().map(|e| {
        serde_json::json!({
            "id": e.id,
            "preview": e.preview,
            "content_type": e.content_type,
            "timestamp": e.timestamp,
            "source_device": e.source_device,
            "pinned": e.pinned,
        })
    }).collect();
    Json(serde_json::json!({ "entries": entries }))
}

async fn post_clipboard_pin(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let id = request["id"].as_str().unwrap_or("");
    let mut history = state.clipboard_history.write().await;
    let pinned = history.pin(id);
    Json(serde_json::json!({ "pinned": pinned }))
}

async fn post_clipboard_unpin(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let id = request["id"].as_str().unwrap_or("");
    let mut history = state.clipboard_history.write().await;
    let unpinned = history.unpin(id);
    Json(serde_json::json!({ "unpinned": unpinned }))
}

async fn post_clipboard_clear(
    State(state): State<Arc<DaemonState>>,
) -> Json<serde_json::Value> {
    let mut history = state.clipboard_history.write().await;
    history.clear();
    Json(serde_json::json!({ "cleared": true }))
}
```

- [ ] **Step 3: Update poll loop to record history**

In the clipboard poll loop, add history recording:

```rust
if let Some(_event) = clipboard_clone.poll_once().await {
    // Record to history
    // (Need to read content from event)
}
```

- [ ] **Step 4: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/daemon/
git commit -m "feat(daemon): integrate clipboard history with REST API"
```

---

## Task 4: CLI Commands

**Covers:** [S6]

**Files:**
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: Add history subcommands**

In `crates/cli/src/main.rs`, update `ClipboardCommand`:

```rust
/// Clipboard management commands.
#[derive(Subcommand)]
enum ClipboardCommand {
    /// Show clipboard sync status.
    Status,
    /// Manually push current clipboard to peers.
    Push,
    /// Show clipboard history.
    History,
    /// Pin a history entry.
    Pin {
        /// Entry ID.
        id: String,
    },
    /// Unpin a history entry.
    Unpin {
        /// Entry ID.
        id: String,
    },
    /// Delete a history entry.
    Delete {
        /// Entry ID.
        id: String,
    },
    /// Clear all history.
    Clear,
}
```

- [ ] **Step 2: Update handler**

```rust
async fn handle_clipboard(
    command: ClipboardCommand,
    client: &Client,
    api: &str,
    json: bool,
) -> Result<()> {
    match command {
        ClipboardCommand::Status => { /* existing */ }
        ClipboardCommand::Push => { /* existing */ }
        ClipboardCommand::History => {
            let history: serde_json::Value = get_json(client, api, "/v1/clipboard/history").await?;
            print_value(&history, json);
        }
        ClipboardCommand::Pin { id } => {
            let _: serde_json::Value = post_json(client, api, "/v1/clipboard/history/pin", &serde_json::json!({ "id": id })).await?;
            println!("Pinned entry {id}");
        }
        ClipboardCommand::Unpin { id } => {
            let _: serde_json::Value = post_json(client, api, "/v1/clipboard/history/unpin", &serde_json::json!({ "id": id })).await?;
            println!("Unpinned entry {id}");
        }
        ClipboardCommand::Delete { id } => {
            let _: serde_json::Value = post_empty(client, api, &format!("/v1/clipboard/history/{id}")).await?;
            println!("Deleted entry {id}");
        }
        ClipboardCommand::Clear => {
            let _: serde_json::Value = post_empty(client, api, "/v1/clipboard/history/clear").await?;
            println!("History cleared");
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Run check**

Run: `cargo check -p hyprbridge-cli`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/cli/
git commit -m "feat(cli): add clipboard history commands"
```

---

## Task 5: Web UI Panel

**Covers:** [S7]

**Files:**
- Create: `packages/ui/src/clipboard-history-panel.tsx`
- Modify: `packages/ui/src/management-console.tsx`
- Modify: `packages/ui/src/styles.css`
- Modify: `packages/ui/src/index.ts`

- [ ] **Step 1: Create ClipboardHistoryPanel**

Create `packages/ui/src/clipboard-history-panel.tsx`:

```tsx
import { Button, Panel } from "./primitives";

export interface ClipboardHistoryEntry {
  id: string;
  preview: string;
  content_type: string;
  timestamp: string;
  pinned: boolean;
}

export interface ClipboardHistoryPanelProps {
  entries: ClipboardHistoryEntry[];
  onPin: (id: string) => void;
  onUnpin: (id: string) => void;
  onDelete: (id: string) => void;
  onClear: () => void;
}

export function ClipboardHistoryPanel(props: ClipboardHistoryPanelProps) {
  return (
    <Panel title="Clipboard History" eyebrow="Clipboard">
      <div className="ib-clipboard-history">
        {props.entries.length > 0 ? (
          <>
            <div className="ib-clipboard-history__actions">
              <Button tone="neutral" onClick={props.onClear}>Clear All</Button>
            </div>
            <div className="ib-clipboard-history__list">
              {props.entries.map((entry) => (
                <div key={entry.id} className={`ib-clipboard-history__entry ${entry.pinned ? "ib-clipboard-history__entry--pinned" : ""}`}>
                  <div className="ib-clipboard-history__entry-content">
                    <span className="ib-clipboard-history__preview">{entry.preview}</span>
                    <span className="ib-clipboard-history__meta">
                      {entry.content_type} · {new Date(entry.timestamp).toLocaleTimeString()}
                    </span>
                  </div>
                  <div className="ib-clipboard-history__entry-actions">
                    <Button tone="neutral" onClick={() => entry.pinned ? props.onUnpin(entry.id) : props.onPin(entry.id)}>
                      {entry.pinned ? "Unpin" : "Pin"}
                    </Button>
                    <Button tone="neutral" onClick={() => props.onDelete(entry.id)}>×</Button>
                  </div>
                </div>
              ))}
            </div>
          </>
        ) : (
          <p className="ib-copy">No clipboard history yet.</p>
        )}
      </div>
    </Panel>
  );
}
```

- [ ] **Step 2: Add styles**

Append to `packages/ui/src/styles.css`:

```css
.ib-clipboard-history__actions { margin-bottom: 12px; }
.ib-clipboard-history__list { display: flex; flex-direction: column; gap: 4px; }
.ib-clipboard-history__entry { display: flex; justify-content: space-between; align-items: center; padding: 8px 12px; border: 1px solid var(--ib-border); border-radius: 8px; transition: border-color 120ms ease; }
.ib-clipboard-history__entry--pinned { border-color: var(--ib-accent-warm); }
.ib-clipboard-history__entry-content { flex: 1; min-width: 0; }
.ib-clipboard-history__preview { display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ib-clipboard-history__meta { font-size: 0.75rem; color: var(--ib-text-muted); }
.ib-clipboard-history__entry-actions { display: flex; gap: 4px; }
```

- [ ] **Step 3: Integrate into ManagementConsole**

Import and add state/handlers in `management-console.tsx`.

- [ ] **Step 4: Export from index.ts**

- [ ] **Step 5: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/ui check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add packages/ui/
git commit -m "feat(ui): add clipboard history panel"
```

---

## Task 6: Final Verification

**Covers:** [S8]

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

Run: `cargo run -p hyprbridge-cli -- clipboard --help`
Expected: CLI help with history subcommands

- [ ] **Step 5: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for clipboard history"
```
