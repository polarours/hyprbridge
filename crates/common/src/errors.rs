use thiserror::Error;

#[derive(Debug, Error)]
pub enum HyprBridgeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Platform error: {0}")]
    Platform(String),
}
