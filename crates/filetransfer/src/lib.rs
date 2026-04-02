//! File transfer between paired devices.

use std::path::{Path, PathBuf};
use std::collections::HashMap;

use chrono::Utc;
use hyprbridge_protocol::{DeviceId, FileTransferHeader};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::sync::RwLock;
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferStatus {
    Pending,
    InProgress,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TransferState {
    pub id: String,
    pub header: Option<FileTransferHeader>,
    pub peer_device: DeviceId,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub status: TransferStatus,
    pub file_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct TransferRequest {
    pub id: String,
    pub target_device: DeviceId,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum TransferItem {
    File(PathBuf),
    Folder(PathBuf),
}

#[derive(Debug, Clone)]
pub struct BatchTransferState {
    pub id: String,
    pub total_files: usize,
    pub completed_files: usize,
    pub current_file: Option<String>,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: TransferStatus,
}

pub struct FileManager {
    queue: RwLock<Vec<TransferRequest>>,
    active: RwLock<HashMap<String, TransferState>>,
    download_dir: PathBuf,
}

impl FileManager {
    pub fn new(download_dir: PathBuf) -> Self {
        Self {
            queue: RwLock::new(Vec::new()),
            active: RwLock::new(HashMap::new()),
            download_dir,
        }
    }

    pub async fn send_file(&self, target: DeviceId, path: PathBuf) -> anyhow::Result<String> {
        if !path.exists() {
            anyhow::bail!("file not found: {}", path.display());
        }
        let id = Uuid::new_v4().to_string();
        let request = TransferRequest {
            id: id.clone(),
            target_device: target.clone(),
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

    pub async fn build_header(&self, path: &Path, source_device: &DeviceId) -> anyhow::Result<FileTransferHeader> {
        let data = fs::read(path).await?;
        let hash = compute_hash(&data);
        let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mime = guess_mime(path);
        Ok(FileTransferHeader {
            file_name,
            file_size: data.len() as u64,
            content_hash: hash,
            source_device: source_device.clone(),
            timestamp: Utc::now(),
            mime_type: mime,
        })
    }

    pub async fn receive_file(&self, header: FileTransferHeader, data: Vec<u8>) -> anyhow::Result<PathBuf> {
        let hash = compute_hash(&data);
        if hash != header.content_hash {
            anyhow::bail!("hash mismatch: expected {}, got {}", header.content_hash, hash);
        }
        fs::create_dir_all(&self.download_dir).await?;
        let file_path = self.download_dir.join(&header.file_name);
        fs::write(&file_path, &data).await?;
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

    pub async fn progress(&self, transfer_id: &str) -> Option<TransferState> {
        self.active.read().await.get(transfer_id).cloned()
    }

    pub async fn list_transfers(&self) -> Vec<TransferState> {
        self.active.read().await.values().cloned().collect()
    }

    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }

    pub async fn send_folder(&self, target: DeviceId, path: PathBuf, preserve_structure: bool) -> anyhow::Result<String> {
        if !path.exists() || !path.is_dir() {
            anyhow::bail!("path is not a directory: {}", path.display());
        }
        let items = vec![TransferItem::Folder(path)];
        self.send_batch(target, items, preserve_structure).await
    }

    pub async fn send_batch(&self, target: DeviceId, items: Vec<TransferItem>, _preserve_structure: bool) -> anyhow::Result<String> {
        let batch_id = Uuid::new_v4().to_string();
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
                    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                        if entry.file_type().is_file() {
                            let metadata = fs::metadata(entry.path()).await?;
                            total_bytes += metadata.len();
                            files.push(entry.path().to_path_buf());
                        }
                    }
                }
            }
        }

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

    pub async fn batch_progress(&self, batch_id: &str) -> Option<BatchTransferState> {
        let active = self.active.read().await;
        let state = active.get(batch_id)?;
        Some(BatchTransferState {
            id: state.id.clone(),
            total_files: 0,
            completed_files: 0,
            current_file: None,
            total_bytes: state.total_bytes,
            transferred_bytes: state.bytes_transferred,
            status: state.status.clone(),
        })
    }
}

#[must_use]
pub fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[must_use]
pub fn guess_mime(path: &Path) -> String {
    mime_guess::from_path(path).first_or_octet_stream().to_string()
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
