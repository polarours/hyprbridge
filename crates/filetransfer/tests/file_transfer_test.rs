use hyprbridge_filetransfer::{compute_hash, FileManager};
use hyprbridge_protocol::DeviceId;
use std::path::PathBuf;

#[tokio::test]
async fn file_manager_send_batch() {
    let manager = FileManager::new(PathBuf::from("/tmp/test-downloads"));
    let batch_id = manager
        .send_batch(DeviceId("test-peer".into()), vec![], true)
        .await
        .unwrap();
    let progress = manager.batch_progress(&batch_id).await;
    assert!(progress.is_some());
}

#[test]
fn compute_hash_deterministic() {
    let data = b"hello world";
    let hash1 = compute_hash(data);
    let hash2 = compute_hash(data);
    assert_eq!(hash1, hash2);
}
