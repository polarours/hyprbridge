//! Clipboard history management with pinning and eviction.

use std::collections::VecDeque;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use hyprbridge_protocol::{ClipboardContentType, DeviceId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A single entry in the clipboard history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardHistoryEntry {
    /// Unique identifier for this entry.
    pub id: String,
    /// Type of clipboard content.
    pub content_type: ClipboardContentType,
    /// Short preview of the content (e.g., first N chars of text).
    pub preview: String,
    /// SHA-256 hash of the full content for deduplication.
    pub content_hash: String,
    /// When this entry was created.
    pub timestamp: DateTime<Utc>,
    /// Device that copied this content.
    pub source_device: DeviceId,
    /// Whether this entry is pinned (protected from eviction).
    pub pinned: bool,
    /// The full content data.
    #[serde(skip)]
    pub data: Vec<u8>,
}

/// Configuration for clipboard history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardHistoryConfig {
    /// Maximum number of entries to retain (excluding pinned).
    pub max_size: usize,
    /// Whether to persist history to disk.
    pub persist: bool,
    /// Path for persistence (if persist is true).
    pub persist_path: Option<PathBuf>,
}

impl Default for ClipboardHistoryConfig {
    fn default() -> Self {
        Self {
            max_size: 100,
            persist: false,
            persist_path: None,
        }
    }
}

/// Clipboard history with FIFO eviction and optional pinning.
#[derive(Debug)]
pub struct ClipboardHistory {
    entries: VecDeque<ClipboardHistoryEntry>,
    config: ClipboardHistoryConfig,
}

impl ClipboardHistory {
    /// Create a new empty clipboard history with the given config.
    pub fn new(config: ClipboardHistoryConfig) -> Self {
        Self {
            entries: VecDeque::new(),
            config,
        }
    }

    /// Add a new entry to the history.
    /// Deduplicates by content_hash; moves to front if duplicate.
    /// Returns the entry ID.
    pub fn add(
        &mut self,
        content_type: ClipboardContentType,
        data: Vec<u8>,
        source_device: DeviceId,
    ) -> String {
        let content_hash = compute_hash(&data);
        let preview = make_preview(&content_type, &data);

        // Check for duplicate
        if let Some(pos) = self.entries.iter().position(|e| e.content_hash == content_hash) {
            let mut entry = self.entries.remove(pos).unwrap();
            let id = entry.id.clone();
            entry.timestamp = Utc::now();
            entry.source_device = source_device;
            self.entries.push_front(entry);
            return id;
        }

        // Create new entry
        let id = uuid::Uuid::new_v4().to_string();
        let entry = ClipboardHistoryEntry {
            id: id.clone(),
            content_type,
            preview,
            content_hash,
            timestamp: Utc::now(),
            source_device,
            pinned: false,
            data,
        };

        self.entries.push_front(entry);
        self.evict_if_needed();
        id
    }

    /// Get all entries (most recent first).
    pub fn entries(&self) -> impl Iterator<Item = &ClipboardHistoryEntry> {
        self.entries.iter()
    }

    /// Get an entry by ID.
    pub fn get(&self, id: &str) -> Option<&ClipboardHistoryEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Pin an entry by ID (protects from eviction).
    pub fn pin(&mut self, id: &str) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.pinned = true;
            true
        } else {
            false
        }
    }

    /// Unpin an entry by ID.
    pub fn unpin(&mut self, id: &str) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.pinned = false;
            true
        } else {
            false
        }
    }

    /// Delete an entry by ID.
    pub fn delete(&mut self, id: &str) -> bool {
        if let Some(pos) = self.entries.iter().position(|e| e.id == id) {
            self.entries.remove(pos);
            true
        } else {
            false
        }
    }

    /// Clear all entries (including pinned).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries in history.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Save history to disk if persistence is enabled.
    pub fn save_to_disk(&self) -> anyhow::Result<()> {
        if !self.config.persist {
            return Ok(());
        }
        let path = self
            .config
            .persist_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("persist_path not configured"))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let serializable: Vec<&ClipboardHistoryEntry> = self.entries.iter().collect();
        let json = serde_json::to_string_pretty(&serializable)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load history from disk if persistence is enabled.
    pub fn load_from_disk(&mut self) -> anyhow::Result<()> {
        if !self.config.persist {
            return Ok(());
        }
        let path = self
            .config
            .persist_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("persist_path not configured"))?;

        if !path.exists() {
            return Ok(());
        }

        let json = std::fs::read_to_string(path)?;
        let entries: Vec<ClipboardHistoryEntry> = serde_json::from_str(&json)?;
        self.entries = entries.into();
        Ok(())
    }

    /// Evict oldest unpinned entries when over capacity.
    fn evict_if_needed(&mut self) {
        while self.entries.len() > self.config.max_size {
            // Find the oldest unpinned entry (from the back)
            if let Some(pos) = self.entries.iter().rposition(|e| !e.pinned) {
                self.entries.remove(pos);
            } else {
                // All entries are pinned, nothing to evict
                break;
            }
        }
    }
}

/// Compute SHA-256 hash of data.
fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Create a preview string from content.
fn make_preview(content_type: &ClipboardContentType, data: &[u8]) -> String {
    match content_type {
        ClipboardContentType::Text => {
            let text = String::from_utf8_lossy(data);
            let preview_len = 100.min(text.len());
            format!("{}...", &text[..preview_len])
        }
        ClipboardContentType::Image => "[Image]".to_string(),
        ClipboardContentType::FileList => {
            let text = String::from_utf8_lossy(data);
            let preview_len = 100.min(text.len());
            format!("Files: {}", &text[..preview_len])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_config() -> ClipboardHistoryConfig {
        ClipboardHistoryConfig {
            max_size: 3,
            persist: false,
            persist_path: None,
        }
    }

    fn make_text_entry(text: &str) -> (ClipboardContentType, Vec<u8>, DeviceId) {
        (
            ClipboardContentType::Text,
            text.as_bytes().to_vec(),
            DeviceId::random(),
        )
    }

    #[test]
    fn add_and_retrieve() {
        let mut history = ClipboardHistory::new(test_config());
        let (ct, data, dev) = make_text_entry("hello");
        let id = history.add(ct, data, dev);

        assert_eq!(history.len(), 1);
        assert!(!history.is_empty());
        assert_eq!(history.get(&id).unwrap().preview, "hello...");
    }

    #[test]
    fn duplicate_detection() {
        let mut history = ClipboardHistory::new(test_config());
        let (ct1, data1, dev1) = make_text_entry("hello");
        let id1 = history.add(ct1, data1, dev1);

        let (ct2, data2, dev2) = make_text_entry("hello");
        let id2 = history.add(ct2, data2, dev2);

        // Should move to front, same ID
        assert_eq!(id1, id2);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn eviction() {
        let mut history = ClipboardHistory::new(test_config());
        // max_size is 3
        history.add(ClipboardContentType::Text, b"a".to_vec(), DeviceId::random());
        history.add(ClipboardContentType::Text, b"b".to_vec(), DeviceId::random());
        history.add(ClipboardContentType::Text, b"c".to_vec(), DeviceId::random());
        assert_eq!(history.len(), 3);

        // Adding a 4th should evict the oldest
        history.add(ClipboardContentType::Text, b"d".to_vec(), DeviceId::random());
        assert_eq!(history.len(), 3);

        // "a" should be evicted (oldest unpinned)
        let entries: Vec<_> = history.entries().collect();
        assert_eq!(entries[0].preview, "d...");
        assert_eq!(entries[1].preview, "c...");
        assert_eq!(entries[2].preview, "b...");
    }

    #[test]
    fn pinning_protects_from_eviction() {
        let mut history = ClipboardHistory::new(test_config());
        let id1 = history.add(ClipboardContentType::Text, b"a".to_vec(), DeviceId::random());
        history.add(ClipboardContentType::Text, b"b".to_vec(), DeviceId::random());
        history.add(ClipboardContentType::Text, b"c".to_vec(), DeviceId::random());

        // Pin "a" (oldest)
        assert!(history.pin(&id1));
        assert!(history.get(&id1).unwrap().pinned);

        // Adding more should evict oldest unpinned
        history.add(ClipboardContentType::Text, b"d".to_vec(), DeviceId::random());
        assert_eq!(history.len(), 3);

        // "a" should still be there
        assert!(history.get(&id1).is_some());
    }

    #[test]
    fn unpinning() {
        let mut history = ClipboardHistory::new(test_config());
        let id1 = history.add(ClipboardContentType::Text, b"a".to_vec(), DeviceId::random());
        history.pin(&id1);
        assert!(history.get(&id1).unwrap().pinned);

        history.unpin(&id1);
        assert!(!history.get(&id1).unwrap().pinned);
    }

    #[test]
    fn delete_entry() {
        let mut history = ClipboardHistory::new(test_config());
        let id1 = history.add(ClipboardContentType::Text, b"a".to_vec(), DeviceId::random());
        history.add(ClipboardContentType::Text, b"b".to_vec(), DeviceId::random());

        assert!(history.delete(&id1));
        assert_eq!(history.len(), 1);
        assert!(history.get(&id1).is_none());
    }

    #[test]
    fn clear_all() {
        let mut history = ClipboardHistory::new(test_config());
        history.add(ClipboardContentType::Text, b"a".to_vec(), DeviceId::random());
        history.add(ClipboardContentType::Text, b"b".to_vec(), DeviceId::random());

        history.clear();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn entries_order() {
        let mut history = ClipboardHistory::new(test_config());
        let id1 = history.add(ClipboardContentType::Text, b"first".to_vec(), DeviceId::random());
        let id2 = history.add(ClipboardContentType::Text, b"second".to_vec(), DeviceId::random());

        let entries: Vec<_> = history.entries().collect();
        assert_eq!(entries[0].id, id2); // most recent first
        assert_eq!(entries[1].id, id1);
    }

    #[test]
    fn persistence_roundtrip() {
        let path = PathBuf::from("/tmp/test_clipboard_history.json");
        let config = ClipboardHistoryConfig {
            max_size: 10,
            persist: true,
            persist_path: Some(path.clone()),
        };

        let mut history = ClipboardHistory::new(config.clone());
        history.add(ClipboardContentType::Text, b"persist this".to_vec(), DeviceId::random());
        history.add(ClipboardContentType::Text, b"and this".to_vec(), DeviceId::random());

        history.save_to_disk().unwrap();

        let mut loaded = ClipboardHistory::new(config);
        loaded.load_from_disk().unwrap();

        assert_eq!(loaded.len(), 2);
        let entries: Vec<_> = loaded.entries().collect();
        assert_eq!(entries[0].preview, "and this...");
        assert_eq!(entries[1].preview, "persist this...");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn nonexistent_get() {
        let history = ClipboardHistory::new(test_config());
        assert!(history.get("nonexistent-id").is_none());
    }

    #[test]
    fn nonexistent_delete() {
        let mut history = ClipboardHistory::new(test_config());
        assert!(!history.delete("nonexistent-id"));
    }

    #[test]
    fn nonexistent_pin() {
        let mut history = ClipboardHistory::new(test_config());
        assert!(!history.pin("nonexistent-id"));
    }

    #[test]
    fn nonexistent_unpin() {
        let mut history = ClipboardHistory::new(test_config());
        assert!(!history.unpin("nonexistent-id"));
    }
}
