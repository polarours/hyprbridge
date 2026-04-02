use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use hyprbridge_config::AppConfig;

/// Data types that can be synced to/from the cloud.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum SyncData {
    /// Application configuration.
    Config(AppConfig),
    /// Device trust store for cryptographic verification.
    TrustStore(Vec<u8>),
    /// Device group memberships and assignments.
    DeviceGroups(Vec<u8>),
}

/// Metadata about a synced record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRecord {
    /// Unique identifier for this sync record.
    pub id: String,
    /// Timestamp of the last modification.
    pub modified_at: DateTime<Utc>,
    /// The synced data payload.
    pub data: SyncData,
}

/// Metadata about a registered device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Unique device identifier.
    pub device_id: String,
    /// Human-readable device name.
    pub name: String,
    /// When the device was last seen.
    pub last_seen: DateTime<Utc>,
}

/// Trait for cloud sync providers (e.g., S3, GCS, custom backends).
#[async_trait]
pub trait SyncProvider: Send + Sync {
    /// Return the name of this provider for logging and identification.
    fn name(&self) -> &str;

    /// Push data to the cloud backend.
    async fn push(&self, key: &str, data: &SyncData) -> anyhow::Result<()>;

    /// Pull data from the cloud backend.
    async fn pull(&self, key: &str) -> anyhow::Result<SyncRecord>;

    /// List all devices registered with this provider.
    async fn list_devices(&self) -> anyhow::Result<Vec<DeviceInfo>>;
}
