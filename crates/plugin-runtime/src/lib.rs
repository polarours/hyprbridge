use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub mod host;
pub use host::{
    DefaultPluginHost, InputEvent, InputEventType, LogLevel, PluginCallback, PluginHost, TimerId,
};

pub mod store;
pub use store::PluginStore;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

/// Permissions a plugin can request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginPermission {
    InputCapture,
    InputInjection,
    ClipboardAccess,
    FileAccess,
    NetworkAccess,
    UIExtension,
}

/// Metadata describing a plugin, typically loaded from a manifest file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub entry_point: String,
    pub permissions: Vec<PluginPermission>,
}

/// Runtime state of a loaded plugin instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    Unloaded,
    Loaded,
    Running,
    Error(String),
}

/// A single loaded plugin instance.
#[derive(Debug)]
pub struct PluginInstance {
    pub manifest: PluginManifest,
    pub state: PluginState,
}

impl PluginInstance {
    /// Create a new unloaded instance from a manifest.
    pub fn new(manifest: PluginManifest) -> Self {
        Self {
            manifest,
            state: PluginState::Unloaded,
        }
    }

    /// Transition from Unloaded to Loaded.
    pub fn load(&mut self) -> Result<()> {
        if self.state != PluginState::Unloaded {
            anyhow::bail!(
                "cannot load plugin '{}' in state {:?}",
                self.manifest.id,
                self.state
            );
        }
        info!(plugin = %self.manifest.id, "loaded");
        self.state = PluginState::Loaded;
        Ok(())
    }

    /// Transition from Loaded to Running.
    pub fn start(&mut self) -> Result<()> {
        if self.state != PluginState::Loaded {
            anyhow::bail!(
                "cannot start plugin '{}' in state {:?}",
                self.manifest.id,
                self.state
            );
        }
        info!(plugin = %self.manifest.id, "started");
        self.state = PluginState::Running;
        Ok(())
    }

    /// Transition from Running (or Loaded) back to Unloaded.
    pub fn stop(&mut self) -> Result<()> {
        match self.state {
            PluginState::Running | PluginState::Loaded => {
                info!(plugin = %self.manifest.id, "stopped");
                self.state = PluginState::Unloaded;
                Ok(())
            }
            _ => anyhow::bail!(
                "cannot stop plugin '{}' in state {:?}",
                self.manifest.id,
                self.state
            ),
        }
    }
}

/// Manages a collection of plugins on disk and in memory.
#[derive(Debug)]
pub struct PluginManager {
    plugins_dir: PathBuf,
    instances: HashMap<String, PluginInstance>,
}

impl PluginManager {
    /// Create a new manager rooted at the given plugins directory.
    pub fn new<P: Into<PathBuf>>(plugins_dir: P) -> Self {
        Self {
            plugins_dir: plugins_dir.into(),
            instances: HashMap::new(),
        }
    }

    /// Scan the plugins directory for `manifest.json` files and register them.
    pub fn scan(&mut self) -> Result<()> {
        if !self.plugins_dir.exists() {
            warn!(dir = %self.plugins_dir.display(), "plugins directory does not exist");
            return Ok(());
        }

        let mut found = 0u32;
        for entry in std::fs::read_dir(&self.plugins_dir)
            .with_context(|| format!("reading {}", self.plugins_dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }
            match Self::load_manifest(&manifest_path) {
                Ok(manifest) => {
                    let id = manifest.id.clone();
                    if !self.instances.contains_key(&id) {
                        self.instances
                            .insert(id.clone(), PluginInstance::new(manifest));
                        info!(plugin = %id, "registered from disk");
                    }
                    found += 1;
                }
                Err(e) => {
                    warn!(path = %manifest_path.display(), error = %e, "failed to load manifest");
                }
            }
        }
        info!(found, "scan complete");
        Ok(())
    }

    /// Return references to all registered plugins.
    pub fn list(&self) -> Vec<&PluginInstance> {
        self.instances.values().collect()
    }

    /// Get a reference to a specific plugin by id.
    pub fn get(&self, id: &str) -> Option<&PluginInstance> {
        self.instances.get(id)
    }

    /// Add a plugin instance manually (e.g. programmatically created).
    pub fn add(&mut self, instance: PluginInstance) {
        let id = instance.manifest.id.clone();
        info!(plugin = %id, "added");
        self.instances.insert(id, instance);
    }

    /// Remove a plugin by id. Returns the removed instance if it existed.
    pub fn remove(&mut self, id: &str) -> Option<PluginInstance> {
        let instance = self.instances.remove(id)?;
        info!(plugin = %id, "removed");
        Some(instance)
    }

    /// Load a manifest from a JSON file.
    fn load_manifest(path: &Path) -> Result<PluginManifest> {
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let manifest: PluginManifest =
            serde_json::from_str(&data).with_context(|| format!("parsing {}", path.display()))?;
        Ok(manifest)
    }
}

/// Generate a new random plugin id.
pub fn new_plugin_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: format!("Plugin {id}"),
            version: "0.1.0".into(),
            author: "test".into(),
            description: "test plugin".into(),
            entry_point: "main.wasm".into(),
            permissions: vec![PluginPermission::ClipboardAccess],
        }
    }

    #[test]
    fn test_plugin_creation() {
        let manifest = sample_manifest("p1");
        let instance = PluginInstance::new(manifest.clone());
        assert_eq!(instance.state, PluginState::Unloaded);
        assert_eq!(instance.manifest.id, "p1");
    }

    #[test]
    fn test_plugin_lifecycle() {
        let mut instance = PluginInstance::new(sample_manifest("p2"));

        instance.load().unwrap();
        assert_eq!(instance.state, PluginState::Loaded);

        instance.start().unwrap();
        assert_eq!(instance.state, PluginState::Running);

        instance.stop().unwrap();
        assert_eq!(instance.state, PluginState::Unloaded);
    }

    #[test]
    fn test_plugin_invalid_transitions() {
        let mut instance = PluginInstance::new(sample_manifest("p3"));

        // Can't start without load
        assert!(instance.start().is_err());

        // Can't unload a stopped plugin
        assert!(instance.stop().is_err());

        // Double load
        instance.load().unwrap();
        assert!(instance.load().is_err());
    }

    #[test]
    fn test_manager_add_get_remove() {
        let mut manager = PluginManager::new("/tmp/test-plugins");
        manager.add(PluginInstance::new(sample_manifest("m1")));

        assert!(manager.get("m1").is_some());
        assert!(manager.get("m2").is_none());

        let list = manager.list();
        assert_eq!(list.len(), 1);

        let removed = manager.remove("m1");
        assert!(removed.is_some());
        assert!(manager.get("m1").is_none());
    }

    #[test]
    fn test_manager_scan_missing_dir() {
        let mut manager = PluginManager::new("/tmp/nonexistent-plugins-dir-xyz");
        manager.scan().unwrap();
        assert!(manager.list().is_empty());
    }

    #[test]
    fn test_manager_scan_with_plugins() {
        let dir = std::env::temp_dir().join(format!("hyprbridge-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("test-plugin")).unwrap();

        let manifest = sample_manifest("scanned");
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        std::fs::write(dir.join("test-plugin/manifest.json"), json).unwrap();

        let mut manager = PluginManager::new(&dir);
        manager.scan().unwrap();

        let plugin = manager.get("scanned").unwrap();
        assert_eq!(plugin.manifest.name, "Plugin scanned");

        // Cleanup
        std::fs::remove_dir_all(&dir).ok();
    }
}
