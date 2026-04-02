use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimerId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputEventType {
    KeyDown,
    KeyUp,
    MouseMove,
    MouseButton,
    MouseScroll,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEvent {
    pub event_type: InputEventType,
    pub data: serde_json::Value,
}

pub type PluginCallback = Box<dyn Fn(InputEvent) + Send + Sync>;

/// Trait that the host exposes to plugins for interacting with the HyprBridge system.
#[async_trait]
pub trait PluginHost: Send + Sync {
    async fn clipboard_read(&self) -> anyhow::Result<String>;
    async fn clipboard_write(&self, text: &str) -> anyhow::Result<()>;
    async fn inject_input(&self, event: InputEvent) -> anyhow::Result<()>;
    async fn config_get(&self, key: &str) -> anyhow::Result<serde_json::Value>;
    async fn config_set(&self, key: &str, value: serde_json::Value) -> anyhow::Result<()>;
    async fn log(&self, level: LogLevel, message: &str) -> anyhow::Result<()>;
    async fn set_timer(
        &self,
        interval_ms: u64,
        callback: PluginCallback,
    ) -> anyhow::Result<TimerId>;
    async fn cancel_timer(&self, id: TimerId) -> anyhow::Result<()>;
    async fn local_device(&self) -> anyhow::Result<serde_json::Value>;
}

/// Default, in-process implementation of [`PluginHost`].
pub struct DefaultPluginHost {
    config: Mutex<HashMap<String, serde_json::Value>>,
}

impl DefaultPluginHost {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for DefaultPluginHost {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PluginHost for DefaultPluginHost {
    async fn clipboard_read(&self) -> anyhow::Result<String> {
        Err(anyhow::anyhow!("clipboard_read not implemented"))
    }

    async fn clipboard_write(&self, _text: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("clipboard_write not implemented"))
    }

    async fn inject_input(&self, _event: InputEvent) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("inject_input not implemented"))
    }

    async fn config_get(&self, key: &str) -> anyhow::Result<serde_json::Value> {
        let config = self.config.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        config
            .get(key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("key not found: {key}"))
    }

    async fn config_set(&self, key: &str, value: serde_json::Value) -> anyhow::Result<()> {
        let mut config = self.config.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        config.insert(key.to_string(), value);
        Ok(())
    }

    async fn log(&self, _level: LogLevel, _message: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn set_timer(
        &self,
        _interval_ms: u64,
        _callback: PluginCallback,
    ) -> anyhow::Result<TimerId> {
        Err(anyhow::anyhow!("set_timer not implemented"))
    }

    async fn cancel_timer(&self, _id: TimerId) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("cancel_timer not implemented"))
    }

    async fn local_device(&self) -> anyhow::Result<serde_json::Value> {
        Err(anyhow::anyhow!("local_device not implemented"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_plugin_host_config_roundtrip() {
        let host = DefaultPluginHost::new();
        let key = "test.key";
        let value = serde_json::json!({"hello": "world"});

        host.config_set(key, value.clone()).await.unwrap();
        let got = host.config_get(key).await.unwrap();
        assert_eq!(got, value);
    }

    #[tokio::test]
    async fn test_default_plugin_host_config_missing_key() {
        let host = DefaultPluginHost::new();
        let result = host.config_get("nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_log_level_serialize() {
        let level = LogLevel::Info;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"Info\"");
    }

    #[test]
    fn test_input_event_serialize() {
        let event = InputEvent {
            event_type: InputEventType::KeyDown,
            data: serde_json::json!({"key": "a"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("KeyDown"));
    }
}
