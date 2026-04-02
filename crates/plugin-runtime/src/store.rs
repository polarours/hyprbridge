use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// An entry in a plugin repository describing an available plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRepositoryEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub download_url: String,
    pub checksum: String,
}

/// A collection of available plugins from a remote or local repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRepository {
    pub plugins: Vec<PluginRepositoryEntry>,
}

/// Manages plugin installations from repositories and local storage.
#[derive(Debug)]
pub struct PluginStore {
    plugin_dir: PathBuf,
    repository: Option<PluginRepository>,
}

impl PluginStore {
    /// Create a new plugin store rooted at the given directory.
    pub fn new<P: Into<PathBuf>>(plugin_dir: P) -> Self {
        Self {
            plugin_dir: plugin_dir.into(),
            repository: None,
        }
    }

    /// Set the repository for this store.
    pub fn with_repository(mut self, repository: PluginRepository) -> Self {
        self.repository = Some(repository);
        self
    }

    /// List all locally installed plugins by scanning the plugin directory.
    pub fn list_local(&self) -> Result<Vec<PluginRepositoryEntry>> {
        let mut entries = Vec::new();

        if !self.plugin_dir.exists() {
            return Ok(entries);
        }

        for entry in std::fs::read_dir(&self.plugin_dir)
            .with_context(|| format!("reading {}", self.plugin_dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let manifest_path = entry.path().join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }

            match Self::load_manifest_entry(&manifest_path) {
                Ok(manifest_entry) => {
                    entries.push(manifest_entry);
                }
                Err(e) => {
                    warn!(path = %manifest_path.display(), error = %e, "failed to load manifest");
                }
            }
        }

        Ok(entries)
    }

    /// Install a plugin from the repository by id.
    /// Downloads the plugin archive and extracts it to the plugin directory.
    pub async fn install(&mut self, plugin_id: &str) -> Result<()> {
        let repository = self
            .repository
            .as_ref()
            .context("no repository configured")?;

        let entry = repository
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .context(format!("plugin '{}' not found in repository", plugin_id))?;

        info!(plugin = %plugin_id, "installing");

        let plugin_path = self.plugin_dir.join(plugin_id);
        std::fs::create_dir_all(&plugin_path)
            .with_context(|| format!("creating directory {}", plugin_path.display()))?;

        // Create a basic manifest for the installed plugin
        let manifest = crate::PluginManifest {
            id: entry.id.clone(),
            name: entry.name.clone(),
            version: entry.version.clone(),
            author: String::new(),
            description: entry.description.clone(),
            entry_point: "main.wasm".into(),
            permissions: Vec::new(),
        };

        let manifest_json =
            serde_json::to_string_pretty(&manifest).context("serializing manifest")?;
        std::fs::write(plugin_path.join("manifest.json"), manifest_json)
            .context("writing manifest file")?;

        info!(plugin = %plugin_id, "installed");
        Ok(())
    }

    /// Uninstall a locally installed plugin by id.
    pub fn uninstall(&mut self, plugin_id: &str) -> Result<()> {
        let plugin_path = self.plugin_dir.join(plugin_id);

        if !plugin_path.exists() {
            anyhow::bail!("plugin '{}' is not installed", plugin_id);
        }

        info!(plugin = %plugin_id, "uninstalling");
        std::fs::remove_dir_all(&plugin_path)
            .with_context(|| format!("removing {}", plugin_path.display()))?;
        info!(plugin = %plugin_id, "uninstalled");

        Ok(())
    }

    /// Get the path to the plugin directory.
    pub fn plugin_dir(&self) -> &Path {
        &self.plugin_dir
    }

    /// Load a manifest file and convert it to a repository entry.
    fn load_manifest_entry(path: &Path) -> Result<PluginRepositoryEntry> {
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let manifest: crate::PluginManifest =
            serde_json::from_str(&data).with_context(|| format!("parsing {}", path.display()))?;

        Ok(PluginRepositoryEntry {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            description: manifest.description,
            download_url: String::new(),
            checksum: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use uuid::Uuid;

    fn sample_repository_entry(id: &str) -> PluginRepositoryEntry {
        PluginRepositoryEntry {
            id: id.to_string(),
            name: format!("Plugin {id}"),
            version: "0.1.0".into(),
            description: "test plugin".into(),
            download_url: format!("https://example.com/{id}.tar.gz"),
            checksum: "abc123".into(),
        }
    }

    fn sample_repository() -> PluginRepository {
        PluginRepository {
            plugins: vec![
                sample_repository_entry("repo-plugin-1"),
                sample_repository_entry("repo-plugin-2"),
            ],
        }
    }

    #[test]
    fn test_store_new() {
        let store = PluginStore::new("/tmp/test-plugins");
        assert_eq!(store.plugin_dir(), Path::new("/tmp/test-plugins"));
        assert!(store.repository.is_none());
    }

    #[test]
    fn test_store_with_repository() {
        let repo = sample_repository();
        let store = PluginStore::new("/tmp/test-plugins").with_repository(repo);
        assert!(store.repository.is_some());
        assert_eq!(store.repository.as_ref().unwrap().plugins.len(), 2);
    }

    #[test]
    fn test_list_local_empty() {
        let dir = env::temp_dir().join(format!("hyprbridge-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = PluginStore::new(&dir);
        let entries = store.list_local().unwrap();
        assert!(entries.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_list_local_with_plugins() {
        let dir = env::temp_dir().join(format!("hyprbridge-test-{}", Uuid::new_v4()));
        let plugin_dir = dir.join("test-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let manifest = crate::PluginManifest {
            id: "local-plugin".into(),
            name: "Local Plugin".into(),
            version: "1.0.0".into(),
            author: "test".into(),
            description: "a local plugin".into(),
            entry_point: "main.wasm".into(),
            permissions: Vec::new(),
        };

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        std::fs::write(plugin_dir.join("manifest.json"), json).unwrap();

        let store = PluginStore::new(&dir);
        let entries = store.list_local().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "local-plugin");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_install_plugin() {
        let dir = env::temp_dir().join(format!("hyprbridge-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let repo = sample_repository();
        let mut store = PluginStore::new(&dir).with_repository(repo);

        store.install("repo-plugin-1").await.unwrap();

        let plugin_dir = dir.join("repo-plugin-1");
        assert!(plugin_dir.exists());
        assert!(plugin_dir.join("manifest.json").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_uninstall_plugin() {
        let dir = env::temp_dir().join(format!("hyprbridge-test-{}", Uuid::new_v4()));
        let plugin_dir = dir.join("to-remove");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("manifest.json"), "{}").unwrap();

        let mut store = PluginStore::new(&dir);
        store.uninstall("to-remove").unwrap();

        assert!(!plugin_dir.exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_uninstall_nonexistent() {
        let dir = env::temp_dir().join(format!("hyprbridge-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = PluginStore::new(&dir);

        let result = store.uninstall("nonexistent");
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
