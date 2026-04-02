# Relay Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement WebSocket-based relay server and client for non-LAN device connections.

**Architecture:** Relay server uses axum + tokio-tungstenite for WebSocket connections. Client connects to server, registers device, gets token, joins room. Server forwards messages between devices in the same room. Token-based authentication.

**Tech Stack:** Rust, axum, tokio-tungstenite, serde, uuid, ed25519-dalek

---

## Task 1: Relay Server Scaffolding

**Covers:** [S3]

**Files:**
- Create: `crates/relay-server/Cargo.toml`
- Create: `crates/relay-server/src/main.rs`
- Create: `crates/relay-server/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Add relay-server to workspace members**

In root `Cargo.toml`, add `"crates/relay-server"` to the `members` list.

- [ ] **Step 2: Create Cargo.toml**

Create `crates/relay-server/Cargo.toml`:

```toml
[package]
name = "hyprbridge-relay-server"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
axum = { workspace = true, features = ["ws"] }
hyprbridge-protocol = { path = "../protocol" }
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tokio = { workspace = true, features = ["full"] }
tokio-tungstenite = "0.24"
tower-http = { workspace = true, features = ["cors", "trace"] }
tracing.workspace = true
tracing-subscriber = { workspace = true, features = ["env-filter"] }
uuid.workspace = true
```

- [ ] **Step 3: Create lib.rs with core types**

Create `crates/relay-server/src/lib.rs`:

```rust
//! Relay server for non-LAN HyprBridge connections.

use std::collections::HashMap;
use std::sync::Arc;

use hyprbridge_protocol::{DeviceId, DeviceIdentity, DeviceRole};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};

/// Message sent between devices through the relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RelayMessage {
    /// Register a device with the relay.
    Register {
        device: DeviceIdentity,
    },
    /// Registration acknowledgement.
    RegisterAck {
        token: String,
        room_id: String,
    },
    /// Join a room.
    JoinRoom {
        room_id: String,
        token: String,
    },
    /// Room join acknowledgement.
    JoinRoomAck {
        room_id: String,
        devices: Vec<DeviceIdentity>,
    },
    /// Forward a control frame to another device.
    Forward {
        target: DeviceId,
        frame: hyprbridge_protocol::ControlFrame,
    },
    /// Control frame received from another device.
    Received {
        source: DeviceIdentity,
        frame: hyprbridge_protocol::ControlFrame,
    },
    /// Error message.
    Error {
        detail: String,
    },
}

/// A room containing connected devices.
#[derive(Debug, Clone)]
pub struct Room {
    /// Unique room identifier.
    pub id: String,
    /// Connected devices: device_id -> (identity, sender).
    pub devices: HashMap<DeviceId, DeviceSender>,
}

/// Sender for a device's WebSocket messages.
#[derive(Debug, Clone)]
pub struct DeviceSender {
    /// Device identity.
    pub identity: DeviceIdentity,
    /// Broadcast sender for messages.
    pub sender: broadcast::Sender<String>,
}

/// Shared relay server state.
pub struct RelayState {
    /// Active rooms: room_id -> Room.
    pub rooms: RwLock<HashMap<String, Room>>,
    /// Device to room mapping: device_id -> room_id.
    pub device_rooms: RwLock<HashMap<DeviceId, String>>,
    /// Device tokens: token -> device_id.
    pub tokens: RwLock<HashMap<String, DeviceId>>,
}

impl RelayState {
    /// Create a new relay state.
    pub fn new() -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            device_rooms: RwLock::new(HashMap::new()),
            tokens: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new device and return token + room_id.
    pub async fn register(&self, device: DeviceIdentity) -> (String, String) {
        let token = uuid::Uuid::new_v4().to_string();
        let room_id = format!("room-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());

        self.tokens.write().await.insert(token.clone(), device.device_id.clone());
        self.device_rooms.write().await.insert(device.device_id.clone(), room_id.clone());

        let mut rooms = self.rooms.write().await;
        let room = rooms.entry(room_id.clone()).or_insert_with(|| Room {
            id: room_id.clone(),
            devices: HashMap::new(),
        });

        let (sender, _) = broadcast::channel(64);
        room.devices.insert(device.device_id.clone(), DeviceSender {
            identity: device,
            sender,
        });

        (token, room_id)
    }

    /// Join an existing room with token.
    pub async fn join_room(&self, room_id: &str, token: &str) -> Result<Vec<DeviceIdentity>, String> {
        let device_id = self.tokens.read().await.get(token).cloned()
            .ok_or("invalid token")?;

        let device_rooms = self.device_rooms.read().await;
        if device_rooms.contains_key(&device_id) {
            return Err("device already in a room".into());
        }
        drop(device_rooms);

        let mut rooms = self.rooms.write().await;
        let room = rooms.get_mut(room_id).ok_or("room not found")?;

        let (sender, _) = broadcast::channel(64);

        // Get device identity from registration
        // For now, create a placeholder - real implementation stores identity in token map
        let identity = DeviceIdentity {
            device_id: device_id.clone(),
            name: "relay-client".into(),
            role: DeviceRole::Target,
            public_key: String::new(),
        };

        room.devices.insert(device_id.clone(), DeviceSender {
            identity: identity.clone(),
            sender,
        });

        self.device_rooms.write().await.insert(device_id, room_id.to_string());

        let devices: Vec<DeviceIdentity> = room.devices.values()
            .map(|d| d.identity.clone())
            .collect();

        Ok(devices)
    }

    /// Get broadcast sender for a device.
    pub async fn get_sender(&self, device_id: &DeviceId) -> Option<broadcast::Sender<String>> {
        let room_id = self.device_rooms.read().await.get(device_id)?.clone();
        let rooms = self.rooms.read().await;
        rooms.get(&room_id)?.devices.get(device_id).map(|d| d.sender.clone())
    }
}

impl Default for RelayState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_returns_token_and_room() {
        let state = RelayState::new();
        let device = DeviceIdentity {
            device_id: DeviceId("test-device".into()),
            name: "Test".into(),
            role: DeviceRole::Target,
            public_key: "key".into(),
        };
        let (token, room_id) = state.register(device).await;
        assert!(!token.is_empty());
        assert!(!room_id.is_empty());
    }

    #[tokio::test]
    async fn join_room_requires_valid_token() {
        let state = RelayState::new();
        let result = state.join_room("room-123", "invalid-token").await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 4: Create main.rs**

Create `crates/relay-server/src/main.rs`:

```rust
//! HyprBridge relay server entry point.

use axum::{
    Router,
    extract::State,
    response::IntoResponse,
    routing::get,
};
use hyprbridge_relay_server::RelayState;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let state = Arc::new(RelayState::new());

    let app = Router::new()
        .route("/health", get(health))
        .with_state(state)
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    info!("Relay server listening on 0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> impl IntoResponse {
    "ok"
}
```

- [ ] **Step 5: Run check**

Run: `cargo check -p hyprbridge-relay-server`
Expected: PASS

- [ ] **Step 6: Run tests**

Run: `cargo test -p hyprbridge-relay-server`
Expected: 2 tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/relay-server/ Cargo.toml
git commit -m "feat(relay): add relay server scaffolding with room management"
```

---

## Task 2: WebSocket Handler

**Covers:** [S3]

**Files:**
- Modify: `crates/relay-server/src/lib.rs`
- Modify: `crates/relay-server/src/main.rs`

- [ ] **Step 1: Add WebSocket handler to lib.rs**

In `crates/relay-server/src/lib.rs`, add:

```rust
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use futures_util::{SinkExt, StreamExt};

/// Handle a WebSocket connection.
pub async fn handle_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RelayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<RelayState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut registered_device: Option<DeviceId> = None;
    let mut rx: Option<broadcast::Receiver<String>> = None;

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text,
            Ok(_) => continue,
            Err(_) => break,
        };

        let relay_msg: RelayMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(_) => {
                let err = RelayMessage::Error { detail: "invalid message format".into() };
                let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                continue;
            }
        };

        match relay_msg {
            RelayMessage::Register { device } => {
                let (token, room_id) = state.register(device.clone()).await;
                registered_device = Some(device.device_id.clone());

                // Subscribe to broadcast
                if let Some(sender_rx) = state.get_sender(&device.device_id).await {
                    rx = Some(sender_rx);
                }

                let ack = RelayMessage::RegisterAck { token, room_id };
                let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
            }
            RelayMessage::JoinRoom { room_id, token } => {
                match state.join_room(&room_id, &token).await {
                    Ok(devices) => {
                        let ack = RelayMessage::JoinRoomAck { room_id, devices };
                        let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
                    }
                    Err(e) => {
                        let err = RelayMessage::Error { detail: e };
                        let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
                    }
                }
            }
            RelayMessage::Forward { target, frame } => {
                if let Some(device_id) = &registered_device {
                    // Find target's broadcast sender
                    let rooms = state.rooms.read().await;
                    for room in rooms.values() {
                        if let Some(target_sender) = room.devices.get(&target) {
                            let msg = RelayMessage::Received {
                                source: room.devices.get(device_id).map(|d| d.identity.clone()).unwrap_or_default(),
                                frame,
                            };
                            let _ = target_sender.sender.send(serde_json::to_string(&msg).unwrap());
                            break;
                        }
                    }
                }
            }
            _ => {
                let err = RelayMessage::Error { detail: "unexpected message type".into() };
                let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
            }
        }
    }

    // Cleanup on disconnect
    if let Some(device_id) = registered_device {
        let mut device_rooms = state.device_rooms.write().await;
        if let Some(room_id) = device_rooms.remove(&device_id) {
            let mut rooms = state.rooms.write().await;
            if let Some(room) = rooms.get_mut(&room_id) {
                room.devices.remove(&device_id);
                if room.devices.is_empty() {
                    rooms.remove(&room_id);
                }
            }
        }
    }
}
```

- [ ] **Step 2: Add WebSocket route to main.rs**

In `crates/relay-server/src/main.rs`, add:

```rust
use hyprbridge_relay_server::handle_ws;

// In Router:
.route("/ws", get(handle_ws))
```

- [ ] **Step 3: Run check**

Run: `cargo check -p hyprbridge-relay-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/relay-server/
git commit -m "feat(relay): add WebSocket handler with room management"
```

---

## Task 3: Relay Client

**Covers:** [S4]

**Files:**
- Create: `crates/network/src/relay_client.rs`
- Modify: `crates/network/src/lib.rs`
- Modify: `crates/network/Cargo.toml`

- [ ] **Step 1: Add dependencies to network Cargo.toml**

In `crates/network/Cargo.toml`, add:

```toml
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
```

- [ ] **Step 2: Create relay_client.rs**

Create `crates/network/src/relay_client.rs`:

```rust
//! Relay client for non-LAN connections.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use hyprbridge_protocol::{
    ControlFrame, DeviceId, DeviceIdentity, DeviceRole,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use std::sync::Arc;

/// Relay message protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RelayMessage {
    Register { device: DeviceIdentity },
    RegisterAck { token: String, room_id: String },
    JoinRoom { room_id: String, token: String },
    JoinRoomAck { room_id: String, devices: Vec<DeviceIdentity> },
    Forward { target: DeviceId, frame: ControlFrame },
    Received { source: DeviceIdentity, frame: ControlFrame },
    Error { detail: String },
}

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Client for connecting to a relay server.
pub struct RelayClient {
    server_url: String,
    token: RwLock<Option<String>>,
    room_id: RwLock<Option<String>>,
    local_device: DeviceIdentity,
    ws_writer: RwLock<Option<futures_util::sink::SplitSink<WsStream, Message>>>,
    frame_sender: broadcast::Sender<(DeviceIdentity, ControlFrame)>,
}

impl RelayClient {
    /// Create a new relay client.
    pub fn new(server_url: String, local_device: DeviceIdentity) -> Self {
        let (frame_sender, _) = broadcast::channel(64);
        Self {
            server_url,
            token: RwLock::new(None),
            room_id: RwLock::new(None),
            local_device,
            ws_writer: RwLock::new(None),
            frame_sender,
        }
    }

    /// Connect to the relay server.
    pub async fn connect(&self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.server_url)
            .await
            .context("failed to connect to relay server")?;

        let (writer, mut reader) = ws_stream.split();
        *self.ws_writer.write().await = Some(writer);

        // Spawn reader task
        let frame_sender = self.frame_sender.clone();
        tokio::spawn(async move {
            while let Some(msg) = reader.next().await {
                if let Ok(Message::Text(text)) = msg {
                    if let Ok(RelayMessage::Received { source, frame }) = serde_json::from_str(&text) {
                        let _ = frame_sender.send((source, frame));
                    }
                }
            }
        });

        Ok(())
    }

    /// Register with the relay server.
    pub async fn register(&self) -> Result<()> {
        let msg = RelayMessage::Register {
            device: self.local_device.clone(),
        };
        self.send_message(&msg).await?;

        // Wait for ack (simplified - real implementation would use a oneshot channel)
        Ok(())
    }

    /// Join a room.
    pub async fn join_room(&self, room_id: &str) -> Result<()> {
        let token = self.token.read().await.clone()
            .context("not registered")?;

        let msg = RelayMessage::JoinRoom {
            room_id: room_id.to_string(),
            token,
        };
        self.send_message(&msg).await?;

        *self.room_id.write().await = Some(room_id.to_string());
        Ok(())
    }

    /// Send a control frame through the relay.
    pub async fn send_frame(&self, target: &DeviceId, frame: ControlFrame) -> Result<()> {
        let msg = RelayMessage::Forward {
            target: target.clone(),
            frame,
        };
        self.send_message(&msg).await
    }

    /// Subscribe to incoming frames.
    pub fn subscribe_frames(&self) -> broadcast::Receiver<(DeviceIdentity, ControlFrame)> {
        self.frame_sender.subscribe()
    }

    /// Get the current room ID.
    pub async fn room_id(&self) -> Option<String> {
        self.room_id.read().await.clone()
    }

    /// Check if connected.
    pub async fn is_connected(&self) -> bool {
        self.ws_writer.read().await.is_some()
    }

    async fn send_message(&self, msg: &RelayMessage) -> Result<()> {
        let mut writer = self.ws_writer.write().await;
        let writer = writer.as_mut().context("not connected")?;
        let text = serde_json::to_string(msg).context("failed to serialize message")?;
        writer.send(Message::Text(text.into())).await
            .context("failed to send message")?;
        Ok(())
    }
}

impl std::fmt::Debug for RelayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayClient")
            .field("server_url", &self.server_url)
            .finish()
    }
}
```

- [ ] **Step 3: Add module to lib.rs**

In `crates/network/src/lib.rs`, add:

```rust
pub mod relay_client;
pub use relay_client::RelayClient;
```

- [ ] **Step 4: Run check**

Run: `cargo check -p hyprbridge-network`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/network/
git commit -m "feat(network): add relay client for non-LAN connections"
```

---

## Task 4: Daemon Relay Integration

**Covers:** [S5]

**Files:**
- Modify: `crates/daemon/src/lib.rs`
- Modify: `crates/daemon/Cargo.toml`
- Modify: `crates/config/src/lib.rs`
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: Add relay config**

In `crates/config/src/lib.rs`, add to `AppConfig`:

```rust
/// Relay server configuration.
#[serde(default)]
pub relay: RelayConfig,

/// Relay server settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RelayConfig {
    /// Whether relay is enabled.
    pub enabled: bool,
    /// Relay server URL.
    pub server_url: String,
}
```

- [ ] **Step 2: Add relay to DaemonState**

In `crates/daemon/src/lib.rs`:

```rust
use hyprbridge_network::RelayClient;

pub struct DaemonState {
    // ... existing fields ...
    relay_client: Arc<RwLock<Option<RelayClient>>>,
}
```

Initialize in `DaemonState::new`:

```rust
relay_client: Arc::new(RwLock::new(None)),
```

- [ ] **Step 3: Add relay CLI commands**

In `crates/cli/src/main.rs`, add:

```rust
/// Relay server commands.
#[derive(Subcommand)]
enum RelayCommand {
    /// Show relay connection status.
    Status,
    /// Connect to a relay server.
    Connect {
        /// Relay server URL.
        url: String,
    },
    /// Disconnect from relay.
    Disconnect,
}
```

- [ ] **Step 4: Add relay REST endpoints**

In `serve()`, add routes:

```rust
.route("/v1/relay/status", get(get_relay_status))
.route("/v1/relay/connect", post(post_relay_connect))
.route("/v1/relay/disconnect", post(post_relay_disconnect))
```

- [ ] **Step 5: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/daemon/ crates/config/ crates/cli/
git commit -m "feat(daemon): integrate relay client with config and API"
```

---

## Task 5: Final Verification

**Covers:** [S6]

**Files:** None

- [ ] **Step 1: Run all workspace tests**

Run: `cargo test --workspace`
Expected: All tests PASS

- [ ] **Step 2: Run workspace check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace`
Expected: No new warnings

- [ ] **Step 4: Verify CLI compiles**

Run: `cargo run -p hyprbridge-cli -- --help`
Expected: CLI help with relay subcommand

- [ ] **Step 5: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for relay mode"
```
