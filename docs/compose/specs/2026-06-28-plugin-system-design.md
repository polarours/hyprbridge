# Plugin System Design

## [S1] Problem

HyprBridge has no extension mechanism. Users cannot add custom input handlers, transport protocols, or UI components. This limits the tool's adaptability to specific workflows and environments.

## [S2] Solution Overview

WASM-based plugin system with WASI sandboxing:
- Plugins compiled to WASM, executed in sandboxed runtime
- Host Functions expose HyprBridge API to plugins
- Plugin Manager handles lifecycle (load/unload/enable/disable)
- Local directory + remote repository for plugin discovery
- CLI + REST API for plugin management

## [S3] Plugin Runtime

New crate `crates/plugin-runtime`:

```rust
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub entry_point: String,
    pub permissions: Vec<PluginPermission>,
}

pub enum PluginPermission {
    InputCapture,
    InputInjection,
    ClipboardAccess,
    FileAccess,
    NetworkAccess,
    UIExtension,
}

pub trait Plugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;
    fn initialize(&mut self, host: &dyn PluginHost) -> Result<()>;
    fn on_input(&mut self, event: &InputEvent) -> Option<InputEvent>;
    fn on_clipboard(&mut self, content: &ClipboardContent) -> Option<ClipboardContent>;
    fn shutdown(&mut self);
}
```

**Files:** `crates/plugin-runtime/Cargo.toml`, `crates/plugin-runtime/src/lib.rs`

## [S4] Host API

Plugin communicates with host via Host Functions:

```rust
pub trait PluginHost: Send + Sync {
    fn clipboard_read(&self) -> Result<String>;
    fn clipboard_write(&self, content: &str) -> Result<()>;
    fn inject_input(&self, event: &InputEvent) -> Result<()>;
    fn config_get(&self, key: &str) -> Option<String>;
    fn config_set(&self, key: &str, value: &str) -> Result<()>;
    fn log(&self, level: LogLevel, message: &str);
    fn set_timer(&self, interval_ms: u64, callback: PluginCallback) -> TimerId;
}

pub enum UIExtensionPoint {
    Sidebar,
    Toolbar,
    ContextMenu,
    SettingsPage,
    StatusBar,
}
```

**Files:** `crates/plugin-runtime/src/host.rs`

## [S5] Plugin Store

- Local directory: `~/.local/share/hyprbridge/plugins/`
- Remote repository: GitHub-based plugin index
- Auto-update checking
- Local cache of installed plugins

**Files:** `crates/plugin-runtime/src/store.rs`

## [S6] CLI Commands

```bash
hyprbridge-cli plugin list
hyprbridge-cli plugin search <query>
hyprbridge-cli plugin install <plugin-id>
hyprbridge-cli plugin uninstall <plugin-id>
hyprbridge-cli plugin enable <plugin-id>
hyprbridge-cli plugin disable <plugin-id>
hyprbridge-cli plugin info <plugin-id>
```

**Files:** `crates/cli/src/main.rs`

## [S7] REST API

```bash
GET    /v1/plugins
POST   /v1/plugins/install
POST   /v1/plugins/uninstall
POST   /v1/plugins/:id/enable
POST   /v1/plugins/:id/disable
GET    /v1/plugins/:id
```

**Files:** `crates/daemon/src/lib.rs`

## [S8] Testing Strategy

- Unit tests for plugin manifest parsing
- Integration test: load and execute a test plugin
- Security test: verify WASM sandbox isolation
- Manual test: install/enable/disable plugin via CLI

## [S9] Non-Goals

- Plugin marketplace with ratings/reviews (v2)
- Plugin dependency management (v2)
- Plugin hot-reload without restart (v2)
- Plugin debugging tools (v2)
- Plugin signing/verification (v2)
