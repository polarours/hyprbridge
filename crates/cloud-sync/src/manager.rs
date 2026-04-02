use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::conflict::ConflictResolver;
use crate::provider::{DeviceInfo, SyncData, SyncProvider, SyncRecord};

/// Coordinates sync operations across devices.
///
/// Wraps a `SyncProvider` and provides higher-level push/pull with conflict
/// resolution, per-device tracking, and sync-state bookkeeping.
pub struct SyncManager {
    provider: Arc<dyn SyncProvider>,
    local_device: String,
    last_sync: RwLock<Option<DateTime<Utc>>>,
}

impl SyncManager {
    pub fn new(provider: Arc<dyn SyncProvider>, local_device: impl Into<String>) -> Self {
        Self {
            provider,
            local_device: local_device.into(),
            last_sync: RwLock::new(None),
        }
    }

    /// Push `data` to the cloud keyed by `local_device/record_id`.
    pub async fn push(&self, key: &str, data: &SyncData) -> Result<()> {
        self.provider.push(key, data).await
    }

    /// Pull remote data and merge it with `local` via `ConflictResolver`.
    ///
    /// Returns the merged record. If the remote is newer the merged result
    /// reflects both sides; if the local is newer, the caller's data wins.
    pub async fn pull(
        &self,
        key: &str,
        local: &SyncData,
    ) -> Result<SyncRecord> {
        let remote = self.provider.pull(key).await?;
        let resolver = ConflictResolver::new();
        let merged = resolver.merge(local, &remote.data)?;

        self.update_sync_time().await;

        Ok(SyncRecord {
            id: remote.id,
            modified_at: remote.modified_at,
            data: merged,
        })
    }

    /// List all devices registered with the provider.
    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        self.provider.list_devices().await
    }

    /// Return the local device identifier.
    pub fn local_device(&self) -> &str {
        &self.local_device
    }

    /// Return the last successful sync timestamp (if any).
    pub async fn last_sync(&self) -> Option<DateTime<Utc>> {
        *self.last_sync.read().await
    }

    /// Record the current time as the last sync time.
    pub async fn update_sync_time(&self) {
        *self.last_sync.write().await = Some(Utc::now());
    }
}
