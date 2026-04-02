use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use tokio::fs;

use crate::provider::{DeviceInfo, SyncData, SyncProvider, SyncRecord};

pub struct LocalSyncProvider {
    sync_dir: PathBuf,
}

impl LocalSyncProvider {
    pub fn new(sync_dir: impl AsRef<Path>) -> Self {
        Self {
            sync_dir: sync_dir.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl SyncProvider for LocalSyncProvider {
    fn name(&self) -> &str {
        "local"
    }

    async fn push(&self, key: &str, data: &SyncData) -> Result<()> {
        fs::create_dir_all(&self.sync_dir)
            .await
            .context("failed to create sync directory")?;

        let path = self.sync_dir.join(format!("{key}.json"));
        let json = serde_json::to_vec_pretty(data)?;
        fs::write(path, json)
            .await
            .context("failed to write sync file")?;
        Ok(())
    }

    async fn pull(&self, key: &str) -> Result<SyncRecord> {
        let path = self.sync_dir.join(format!("{key}.json"));
        let bytes = fs::read(&path)
            .await
            .context("failed to read sync file")?;
        let data: SyncData = serde_json::from_slice(&bytes)?;

        let metadata = fs::metadata(&path).await?;
        let modified_at = metadata
            .modified()
            .map(|t| t.into())
            .unwrap_or_else(|_| Utc::now());

        Ok(SyncRecord {
            id: key.to_string(),
            modified_at,
            data,
        })
    }

    async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let devices_dir = self.sync_dir.join("devices");
        if !devices_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&devices_dir)
            .await
            .context("failed to read devices directory")?;
        let mut devices = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let bytes = fs::read(&path).await?;
                let info: DeviceInfo = serde_json::from_slice(&bytes)?;
                devices.push(info);
            }
        }

        Ok(devices)
    }
}
