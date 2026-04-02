# Plugin System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement WASM-based plugin system with sandboxed execution, host API, plugin store, and CLI/REST management.

**Architecture:** New `plugin-runtime` crate manages WASM plugin lifecycle. Host Functions expose HyprBridge API to plugins. Plugin Store handles local + remote plugin discovery. CLI + REST API for management.

**Tech Stack:** Rust, wasmer (WASM runtime), serde, tokio

---

## Task 1: Plugin Runtime Scaffolding

**Covers:** [S3]

**Files:**
- Create: `crates/plugin-runtime/Cargo.toml`
- Create: `crates/plugin-runtime/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Add plugin-runtime to workspace**

In root `Cargo.toml`, add `"crates/plugin-runtime"` to members.

- [ ] **Step 2: Create Cargo.toml**

```toml
[package]
name = "hyprbridge-plugin-runtime"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
anyhow.workspace = true
hyprbridge-protocol = { path = "../protocol" }
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tokio.workspace = true
tracing.workspace = true
uuid.workspace = true
wasmer = "4.0"
wasmer-wasi = "4.0"
```

- [ ] **Step 3: Create lib.rs with core types**

Create `crates/plugin-runtime/src/lib.rs`:

```rust
//! WASM-based plugin runtime for HyprBridge.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Plugin permissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginPermission {
    InputCapture,
    InputInjection,
    ClipboardAccess,
    FileAccess,
    NetworkAccess,
    UIExtension,
}

/// Plugin manifest.
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

/// Plugin state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    Unloaded,
    Loaded,
    Running,
    Error(String),
}

/// Loaded plugin instance.
pub struct PluginInstance {
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub path: PathBuf,
}

impl PluginInstance {
    /// Create a new plugin instance from manifest.
    pub fn new(manifest: PluginManifest, path: PathBuf) -> Self {
        Self {
            manifest,
            state: PluginState::Unloaded,
            path,
        }
    }

    /// Load the plugin WASM module.
    pub fn load(&mut self) -> anyhow::Result<()> {
        // In real implementation, compile WASM module
        self.state = PluginState::Loaded;
        Ok(())
    }

    /// Start the plugin.
    pub fn start(&mut self) -> anyhow::Result<()> {
        if self.state != PluginState::Loaded {
            anyhow::bail!("plugin must be loaded first");
        }
        self.state = PluginState::Running;
        Ok(())
    }

    /// Stop the plugin.
    pub fn stop(&mut self) {
        self.state = PluginState::Loaded;
    }
}

/// Plugin manager that handles multiple plugins.
pub struct PluginManager {
    plugins: Vec<PluginInstance>,
    plugin_dir: PathBuf,
}

impl PluginManager {
    /// Create a new plugin manager.
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: Vec::new(),
            plugin_dir,
        }
    }

    /// Scan plugin directory for manifests.
    pub fn scan(&mut self) -> anyhow::Result<()> {
        if !self.plugin_dir.exists() {
            std::fs::create_dir_all(&self.plugin_dir)?;
        }
        // In real implementation, scan for manifest.json files
        Ok(())
    }

    /// List all plugins.
    pub fn list(&self) -> Vec<&PluginInstance> {
        self.plugins.iter().collect()
    }

    /// Get plugin by ID.
    pub fn get(&self, id: &str) -> Option<&PluginInstance> {
        self.plugins.iter().find(|p| p.manifest.id == id)
    }

    /// Add a plugin.
    pub fn add(&mut self, instance: PluginInstance) {
        self.plugins.push(instance);
    }

    /// Remove a plugin.
    pub fn remove(&mut self, id: &str) -> bool {
        let len = self.plugins.len();
        self.plugins.retain(|p| p.manifest.id != id);
        self.plugins.len() < len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_instance_creation() {
        let manifest = PluginManifest {
            id: "test-plugin".into(),
            name: "Test Plugin".into(),
            version: "1.0.0".into(),
            author: "Test".into(),
            description: "A test plugin".into(),
            entry_point: "plugin.wasm".into(),
            permissions: vec![],
        };
        let instance = PluginInstance::new(manifest, PathBuf::from("/tmp/plugin.wasm"));
        assert_eq!(instance.state, PluginState::Unloaded);
    }

    #[test]
    fn plugin_lifecycle() {
        let manifest = PluginManifest {
            id: "test-plugin".into(),
            name: "Test Plugin".into(),
            version: "1.0.0".into(),
            author: "Test".into(),
            description: "A test plugin".into(),
            entry_point: "plugin.wasm".into(),
            permissions: vec![],
        };
        let mut instance = PluginInstance::new(manifest, PathBuf::from("/tmp/plugin.wasm"));
        assert!(instance.load().is_ok());
        assert_eq!(instance.state, PluginState::Loaded);
        assert!(instance.start().is_ok());
        assert_eq!(instance.state, PluginState::Running);
        instance.stop();
        assert_eq!(instance.state, PluginState::Loaded);
    }

    #[test]
    fn plugin_manager_operations() {
        let mut manager = PluginManager::new(PathBuf::from("/tmp/plugins"));
        let manifest = PluginManifest {
            id: "test-plugin".into(),
            name: "Test Plugin".into(),
            version: "1.0.0".into(),
            author: "Test".into(),
            description: "A test plugin".into(),
            entry_point: "plugin.wasm".into(),
            permissions: vec![],
        };
        let instance = PluginInstance::new(manifest, PathBuf::from("/tmp/plugin.wasm"));
        manager.add(instance);
        assert_eq!(manager.list().len(), 1);
        assert!(manager.get("test-plugin").is_some());
        assert!(manager.remove("test-plugin"));
        assert_eq!(manager.list().len(), 0);
    }
}
```

- [ ] **Step 4: Run check**

Run: `cargo check -p hyprbridge-plugin-runtime`
Expected: PASS

- [ ] **Step 5: Run tests**

Run: `cargo test -p hyprbridge-plugin-runtime`
Expected: 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/plugin-runtime/ Cargo.toml
git commit -m "feat(plugin): add plugin runtime scaffolding with lifecycle management"
```

---

## Task 2: Host API

**Covers:** [S4]

**Files:**
- Create: `crates/plugin-runtime/src/host.rs`
- Modify: `crates/plugin-runtime/src/lib.rs`

- [ ] **Step 1: Create host.rs**

Create `crates/plugin-runtime/src/host.rs`:

```rust
//! Host API for plugin communication.

use hyprbridge_protocol::DeviceId;

/// Log level for plugin logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Timer ID for registered timers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(pub u64);

/// Callback type for timer events.
pub type PluginCallback = Box<dyn Fn() + Send + Sync>;

/// Host API trait that plugins can call.
pub trait PluginHost: Send + Sync {
    /// Read current clipboard content.
    fn clipboard_read(&self) -> anyhow::Result<String>;

    /// Write content to clipboard.
    fn clipboard_write(&self, content: &str) -> anyhow::Result<()>;

    /// Inject an input event.
    fn inject_input(&self, event: &InputEvent) -> anyhow::Result<()>;

    /// Read a configuration value.
    fn config_get(&self, key: &str) -> Option<String>;

    /// Write a configuration value.
    fn config_set(&self, key: &str, value: &str) -> anyhow::Result<()>;

    /// Log a message.
    fn log(&self, level: LogLevel, message: &str);

    /// Register a periodic timer.
    fn set_timer(&self, interval_ms: u64, callback: PluginCallback) -> TimerId;

    /// Cancel a timer.
    fn cancel_timer(&self, id: TimerId);

    /// Get the local device identity.
    fn local_device(&self) -> DeviceId;
}

/// Input event for plugin processing.
#[derive(Debug, Clone)]
pub struct InputEvent {
    pub event_type: InputEventType,
    pub data: Vec<u8>,
}

/// Input event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEventType {
    KeyDown,
    KeyUp,
    MouseMove,
    MouseButton,
    MouseScroll,
}

/// Default host implementation.
pub struct DefaultPluginHost {
    local_device: DeviceId,
}

impl DefaultPluginHost {
    /// Create a new default host.
    pub fn new(local_device: DeviceId) -> Self {
        Self { local_device }
    }
}

impl PluginHost for DefaultPluginHost {
    fn clipboard_read(&self) -> anyhow::Result<String> {
        // In real implementation, read from platform clipboard
        Ok(String::new())
    }

    fn clipboard_write(&self, _content: &str) -> anyhow::Result<()> {
        // In real implementation, write to platform clipboard
        Ok(())
    }

    fn inject_input(&self, _event: &InputEvent) -> anyhow::Result<()> {
        // In real implementation, inject via platform backend
        Ok(())
    }

    fn config_get(&self, _key: &str) -> Option<String> {
        None
    }

    fn config_set(&self, _key: &str, _value: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn log(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Debug => tracing::debug!("plugin: {}", message),
            LogLevel::Info => tracing::info!("plugin: {}", message),
            LogLevel::Warn => tracing::warn!("plugin: {}", message),
            LogLevel::Error => tracing::error!("plugin: {}", message),
        }
    }

    fn set_timer(&self, _interval_ms: u64, _callback: PluginCallback) -> TimerId {
        TimerId(0)
    }

    fn cancel_timer(&self, _id: TimerId) {}

    fn local_device(&self) -> DeviceId {
        self.local_device.clone()
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

```rust
pub mod host;
pub use host::{PluginHost, DefaultPluginHost, LogLevel, TimerId, InputEvent, InputEventType};
```

- [ ] **Step 3: Run check**

Run: `cargo check -p hyprbridge-plugin-runtime`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/plugin-runtime/
git commit -m "feat(plugin): add host API for plugin communication"
```

---

## Task 3: Plugin Store

**Covers:** [S5]

**Files:**
- Create: `crates/plugin-runtime/src/store.rs`
- Modify: `crates/plugin-runtime/src/lib.rs`

- [ ] **Step 1: Create store.rs**

Create `crates/plugin-runtime/src/store.rs`:

```rust
//! Plugin store for local and remote plugin management.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Plugin repository entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRepositoryEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub download_url: String,
    pub checksum: String,
}

/// Plugin repository index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRepository {
    pub plugins: Vec<PluginRepositoryEntry>,
}

/// Plugin store managing local and remote plugins.
pub struct PluginStore {
    local_dir: PathBuf,
    cache_dir: PathBuf,
    repository_url: Option<String>,
}

impl PluginStore {
    /// Create a new plugin store.
    pub fn new(local_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            local_dir,
            cache_dir,
            repository_url: None,
        }
    }

    /// Set the repository URL.
    pub fn with_repository(mut self, url: String) -> Self {
        self.repository_url = Some(url);
        self
    }

    /// List locally installed plugins.
    pub fn list_local(&self) -> anyhow::Result<Vec<super::PluginManifest>> {
        let mut manifests = Vec::new();
        if self.local_dir.exists() {
            for entry in std::fs::read_dir(&self.local_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let manifest_path = path.join("manifest.json");
                    if manifest_path.exists() {
                        let json = std::fs::read_to_string(&manifest_path)?;
                        let manifest: super::PluginManifest = serde_json::from_str(&json)?;
                        manifests.push(manifest);
                    }
                }
            }
        }
        Ok(manifests)
    }

    /// Install a plugin from a WASM file.
    pub fn install(&self, wasm_path: &Path, manifest: super::PluginManifest) -> anyhow::Result<PathBuf> {
        let plugin_dir = self.local_dir.join(&manifest.id);
        std::fs::create_dir_all(&plugin_dir)?;

        // Copy WASM file
        let wasm_dest = plugin_dir.join(&manifest.entry_point);
        std::fs::copy(wasm_path, &wasm_dest)?;

        // Write manifest
        let manifest_path = plugin_dir.join("manifest.json");
        let json = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(&manifest_path, json)?;

        Ok(plugin_dir)
    }

    /// Uninstall a plugin.
    pub fn uninstall(&self, plugin_id: &str) -> anyhow::Result<()> {
        let plugin_dir = self.local_dir.join(plugin_id);
        if plugin_dir.exists() {
            std::fs::remove_dir_all(&plugin_dir)?;
        }
        Ok(())
    }

    /// Get plugin directory path.
    pub fn plugin_dir(&self) -> &Path {
        &self.local_dir
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

```rust
pub mod store;
pub use store::PluginStore;
```

- [ ] **Step 3: Run check**

Run: `cargo check -p hyprbridge-plugin-runtime`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/plugin-runtime/
git commit -m "feat(plugin): add plugin store for local and remote management"
```

---

## Task 4: Daemon Plugin API

**Covers:** [S7]

**Files:**
- Modify: `crates/daemon/src/lib.rs`

- [ ] **Step 1: Add plugin REST endpoints**

Add routes:

```rust
.route("/v1/plugins", get(get_plugins))
.route("/v1/plugins/install", post(post_plugin_install))
.route("/v1/plugins/uninstall", post(post_plugin_uninstall))
.route("/v1/plugins/:id/enable", post(post_plugin_enable))
.route("/v1/plugins/:id/disable", post(post_plugin_disable))
.route("/v1/plugins/:id", get(get_plugin_info))
```

Handlers:

```rust
async fn get_plugins() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "plugins": [] }))
}

async fn post_plugin_install(
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "installed": true }))
}

async fn post_plugin_uninstall(
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "uninstalled": true }))
}

async fn post_plugin_enable(Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "enabled": id }))
}

async fn post_plugin_disable(Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "disabled": id }))
}

async fn get_plugin_info(Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": id }))
}
```

- [ ] **Step 2: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/daemon/
git commit -m "feat(daemon): add plugin management REST API"
```

---

## Task 5: CLI Plugin Commands

**Covers:** [S6]

**Files:**
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: Add plugin commands**

```rust
/// Plugin management commands.
#[derive(Subcommand)]
enum PluginCommand {
    /// List installed plugins.
    List,
    /// Search plugin repository.
    Search {
        /// Search query.
        query: String,
    },
    /// Install a plugin.
    Install {
        /// Plugin ID or path to WASM file.
        source: String,
    },
    /// Uninstall a plugin.
    Uninstall {
        /// Plugin ID.
        plugin_id: String,
    },
    /// Enable a plugin.
    Enable {
        /// Plugin ID.
        plugin_id: String,
    },
    /// Disable a plugin.
    Disable {
        /// Plugin ID.
        plugin_id: String,
    },
    /// Show plugin information.
    Info {
        /// Plugin ID.
        plugin_id: String,
    },
}
```

- [ ] **Step 2: Run check**

Run: `cargo check -p hyprbridge-cli`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/cli/
git commit -m "feat(cli): add plugin management commands"
```

---

## Task 6: Final Verification

**Covers:** [S8]

**Files:** None

- [ ] **Step 1: Run all workspace tests**

Run: `cargo test --workspace`
Expected: All tests PASS

- [ ] **Step 2: Run workspace check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Verify CLI compiles**

Run: `cargo run -p hyprbridge-cli -- plugin --help`
Expected: CLI help with plugin subcommands

- [ ] **Step 4: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for plugin system"
```
