use hyprbridge_clipboard::{ClipboardContent, ClipboardHistory, ClipboardHistoryConfig};
use hyprbridge_protocol::{ClipboardContentType, DeviceId};

#[tokio::test]
async fn clipboard_content_hash_is_deterministic() {
    let content = ClipboardContent {
        content_type: ClipboardContentType::Text,
        data: b"test content".to_vec(),
        file_names: None,
    };
    let hash1 = content.hash();
    let hash2 = content.hash();
    assert_eq!(hash1, hash2);
}

#[tokio::test]
async fn clipboard_history_add_and_retrieve() {
    let mut history = ClipboardHistory::new(ClipboardHistoryConfig::default());
    let id = history.add(ClipboardContentType::Text, b"test".to_vec(), DeviceId::random());
    assert!(!id.is_empty());
    assert_eq!(history.len(), 1);
    let entry = history.get(&id);
    assert!(entry.is_some());
}

#[tokio::test]
async fn clipboard_history_pin_prevents_eviction() {
    let config = hyprbridge_clipboard::ClipboardHistoryConfig {
        max_size: 2,
        ..Default::default()
    };
    let mut history = ClipboardHistory::new(config);
    let id1 = history.add(ClipboardContentType::Text, b"first".to_vec(), DeviceId::random());
    history.pin(&id1);
    history.add(ClipboardContentType::Text, b"second".to_vec(), DeviceId::random());
    history.add(ClipboardContentType::Text, b"third".to_vec(), DeviceId::random());
    assert_eq!(history.len(), 2);
    assert!(history.get(&id1).is_some());
}
