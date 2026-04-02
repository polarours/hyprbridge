//! Configuration loading and application data paths.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use hyprbridge_core::TopologyLayout;
use hyprbridge_protocol::{DeviceRole, EdgeMapping};
use serde::{Deserialize, Serialize};
use tokio::fs;

/// Persisted user configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Human-readable local device name.
    pub device_name: String,
    /// Runtime role for the device.
    pub role: DeviceRole,
    /// Local daemon address.
    pub local_api_bind: String,
    /// Topology settings.
    pub topology: TopologyConfig,
    /// Allowed CORS origins.
    #[serde(default = "default_cors_origins")]
    pub cors_origins: Vec<String>,
    /// Relay server configuration.
    #[serde(default)]
    pub relay: RelayConfig,
    /// Clipboard configuration.
    #[serde(default)]
    pub clipboard: ClipboardConfig,
    /// Recording configuration.
    #[serde(default)]
    pub recording: RecordingConfig,
    /// Cloud sync configuration.
    #[serde(default)]
    pub cloud_sync: CloudSyncConfig,
}

/// Relay server settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RelayConfig {
    /// Whether relay is enabled.
    pub enabled: bool,
    /// Relay server URL.
    pub server_url: String,
}

/// Recording settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    pub enabled: bool,
    pub max_duration_ms: u64,
    pub storage_path: String,
    pub format: String,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_duration_ms: 3_600_000,
            storage_path: "~/.local/share/hyprbridge/recordings/".into(),
            format: "hybrid".into(),
        }
    }
}

/// Cloud sync settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncConfig {
    pub enabled: bool,
    pub backend: String,
    pub server_url: String,
    pub auth_token: String,
    pub encryption: String,
    pub sync_interval_ms: u64,
}

impl Default for CloudSyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: "local".into(),
            server_url: String::new(),
            auth_token: String::new(),
            encryption: "transport_only".into(),
            sync_interval_ms: 300_000,
        }
    }
}

/// Clipboard settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardConfig {
    pub history_size: usize,
    pub persist_history: bool,
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

fn default_cors_origins() -> Vec<String> {
    vec![
        "http://localhost".into(),
        "http://localhost:*".into(),
        "http://127.0.0.1".into(),
        "http://127.0.0.1:*".into(),
    ]
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_name: "HyprBridge Device".into(),
            role: DeviceRole::Controller,
            local_api_bind: "127.0.0.1:24800".into(),
            topology: TopologyConfig::default(),
            cors_origins: default_cors_origins(),
            relay: RelayConfig::default(),
            clipboard: ClipboardConfig::default(),
            recording: RecordingConfig::default(),
            cloud_sync: CloudSyncConfig::default(),
        }
    }
}

impl AppConfig {
    /// Convert the persisted topology into the domain model.
    pub fn topology_layout(&self) -> Result<TopologyLayout> {
        TopologyLayout::try_from_mappings(self.topology.edges.clone(), self.topology.dead_zone_px)
            .context("invalid topology configuration")
    }

    /// Load configuration from disk.
    pub async fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read config from {}", path.display()))?;

        toml::from_str(&raw).context("failed to parse config")
    }

    /// Save configuration to disk.
    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let raw = toml::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(path, raw)
            .await
            .with_context(|| format!("failed to write {}", path.display()))
    }
}

/// Topology-related settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyConfig {
    /// Mapped edges.
    pub edges: Vec<EdgeMapping>,
    /// Dead zone in pixels.
    pub dead_zone_px: u16,
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            edges: Vec::new(),
            dead_zone_px: 12,
        }
    }
}

/// Filesystem paths used by the application.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Root state directory.
    pub root: PathBuf,
    /// Config file path.
    pub config: PathBuf,
    /// Device identity file path.
    pub identity: PathBuf,
    /// Trusted device store path.
    pub trust_store: PathBuf,
    /// QUIC certificate path.
    pub transport_cert: PathBuf,
    /// QUIC private key path.
    pub transport_key: PathBuf,
    /// Optional future state database path.
    pub state_store: PathBuf,
}

impl AppPaths {
    /// Resolve default application directories.
    pub fn resolve() -> Result<Self> {
        if let Ok(root_override) = std::env::var("HYPRBRIDGE_STATE_DIR") {
            let root = PathBuf::from(root_override);
            return Ok(Self {
                config: root.join("hyprbridge.toml"),
                identity: root.join("device-identity.json"),
                trust_store: root.join("trusted-devices.json"),
                transport_cert: root.join("transport-cert.der"),
                transport_key: root.join("transport-key.der"),
                state_store: root.join("state.json"),
                root,
            });
        }

        let dirs = ProjectDirs::from("io", "HyprBridge", "HyprBridge")
            .context("failed to resolve platform application directories")?;
        let root = dirs.data_local_dir().to_path_buf();

        Ok(Self {
            config: root.join("hyprbridge.toml"),
            identity: root.join("device-identity.json"),
            trust_store: root.join("trusted-devices.json"),
            transport_cert: root.join("transport-cert.der"),
            transport_key: root.join("transport-key.der"),
            state_store: root.join("state.json"),
            root,
        })
    }
}

#[cfg(test)]
mod tests {
    use hyprbridge_protocol::{DeviceId, ScreenEdge};

    use super::{AppConfig, TopologyConfig};

    #[tokio::test]
    async fn config_round_trip_preserves_topology() {
        let config = AppConfig {
            topology: TopologyConfig {
                edges: vec![hyprbridge_protocol::EdgeMapping {
                    local_edge: ScreenEdge::Left,
                    remote_device: DeviceId("peer-1".into()),
                    remote_edge: ScreenEdge::Right,
                }],
                dead_zone_px: 24,
            },
            ..AppConfig::default()
        };

        let temp_dir = std::env::temp_dir().join("hyprbridge-config-test.toml");
        config.save(&temp_dir).await.expect("save should succeed");
        let loaded = AppConfig::load(&temp_dir)
            .await
            .expect("load should succeed");

        assert_eq!(loaded.topology.dead_zone_px, 24);
        assert_eq!(loaded.topology.edges.len(), 1);

        tokio::fs::remove_file(temp_dir)
            .await
            .expect("cleanup should succeed");
    }
}
