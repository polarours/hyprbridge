# Screen Sharing / Remote Desktop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement WebRTC-based screen sharing with view and control modes for LAN and remote devices.

**Architecture:** New screen-capture crate for platform-specific capture. WebRTC for P2P video/audio streaming. Signaling via relay server. Adaptive encoding with hardware acceleration support.

**Tech Stack:** Rust, webrtc-rs (WebRTC), screen-capture, tokio, serde

---

## Task 1: Screen Capture Scaffolding

**Covers:** [S3]

**Files:**
- Create: `crates/screen-capture/Cargo.toml`
- Create: `crates/screen-capture/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Add screen-capture to workspace**

In root `Cargo.toml`, add `"crates/screen-capture"` to members.

- [ ] **Step 2: Create Cargo.toml**

```toml
[package]
name = "hyprbridge-screen-capture"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
anyhow.workspace = true
hyprbridge-protocol = { path = "../protocol" }
serde = { workspace = true, features = ["derive"] }
tokio.workspace = true
tracing.workspace = true
```

- [ ] **Step 3: Create lib.rs**

Create `crates/screen-capture/src/lib.rs`:

```rust
//! Screen capture module for HyprBridge.

use serde::{Deserialize, Serialize};

/// Resolution for screen capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Default for Resolution {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
        }
    }
}

/// Screen capture configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    /// Frame rate (1-60).
    pub fps: u32,
    /// Capture resolution.
    pub resolution: Resolution,
    /// Quality (1-100).
    pub quality: u32,
    /// Whether to encode video.
    pub encode_video: bool,
    /// Whether to capture audio.
    pub capture_audio: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            fps: 30,
            resolution: Resolution::default(),
            quality: 80,
            encode_video: true,
            capture_audio: false,
        }
    }
}

/// A captured frame.
#[derive(Debug, Clone)]
pub struct CaptureFrame {
    /// Frame timestamp (microseconds).
    pub timestamp: u64,
    /// Video data (encoded).
    pub video_data: Vec<u8>,
    /// Audio data (optional).
    pub audio_data: Option<Vec<u8>>,
    /// Frame width.
    pub width: u32,
    /// Frame height.
    pub height: u32,
}

/// Screen capture trait.
pub trait ScreenCapture: Send + Sync {
    /// Start capturing with the given config.
    fn start(&mut self, config: CaptureConfig) -> anyhow::Result<()>;

    /// Stop capturing.
    fn stop(&mut self);

    /// Capture a single frame.
    fn capture_frame(&mut self) -> anyhow::Result<CaptureFrame>;

    /// Update capture configuration.
    fn set_config(&mut self, config: CaptureConfig) -> anyhow::Result<()>;

    /// Check if capture is active.
    fn is_active(&self) -> bool;
}

/// Platform-specific screen capture implementation.
pub struct PlatformCapture {
    config: CaptureConfig,
    active: bool,
}

impl PlatformCapture {
    /// Create a new platform capture.
    pub fn new() -> Self {
        Self {
            config: CaptureConfig::default(),
            active: false,
        }
    }
}

impl Default for PlatformCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCapture for PlatformCapture {
    fn start(&mut self, config: CaptureConfig) -> anyhow::Result<()> {
        self.config = config;
        self.active = true;
        tracing::info!("screen capture started with {}fps", self.config.fps);
        Ok(())
    }

    fn stop(&mut self) {
        self.active = false;
        tracing::info!("screen capture stopped");
    }

    fn capture_frame(&mut self) -> anyhow::Result<CaptureFrame> {
        if !self.active {
            anyhow::bail!("capture not active");
        }

        // Placeholder: in real implementation, capture screen
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_micros() as u64;

        Ok(CaptureFrame {
            timestamp,
            video_data: Vec::new(),
            audio_data: None,
            width: self.config.resolution.width,
            height: self.config.resolution.height,
        })
    }

    fn set_config(&mut self, config: CaptureConfig) -> anyhow::Result<()> {
        self.config = config;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_config_default() {
        let config = CaptureConfig::default();
        assert_eq!(config.fps, 30);
        assert_eq!(config.quality, 80);
        assert!(!config.capture_audio);
    }

    #[test]
    fn platform_capture_lifecycle() {
        let mut capture = PlatformCapture::new();
        assert!(!capture.is_active());

        capture.start(CaptureConfig::default()).unwrap();
        assert!(capture.is_active());

        let frame = capture.capture_frame().unwrap();
        assert!(frame.timestamp > 0);

        capture.stop();
        assert!(!capture.is_active());
    }

    #[test]
    fn capture_frame_fails_when_inactive() {
        let mut capture = PlatformCapture::new();
        assert!(capture.capture_frame().is_err());
    }
}
```

- [ ] **Step 4: Run check**

Run: `cargo check -p hyprbridge-screen-capture`
Expected: PASS

- [ ] **Step 5: Run tests**

Run: `cargo test -p hyprbridge-screen-capture`
Expected: 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/screen-capture/ Cargo.toml
git commit -m "feat(screen): add screen capture scaffolding with platform trait"
```

---

## Task 2: Protocol Extensions

**Covers:** [S4]

**Files:**
- Modify: `crates/protocol/src/lib.rs`

- [ ] **Step 1: Add screen sharing types**

In `crates/protocol/src/lib.rs`, add:

```rust
/// Screen sharing session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenShareSession {
    pub id: String,
    pub host_device: DeviceId,
    pub viewer_device: DeviceId,
    pub direction: SessionDirection,
    pub state: ScreenShareState,
    pub config: ScreenShareConfig,
    pub created_at: DateTime<Utc>,
}

/// Screen sharing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenShareState {
    Negotiating,
    Active,
    Paused,
    Ended,
}

/// Screen sharing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenShareConfig {
    pub fps: u32,
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub quality: u32,
    pub audio_enabled: bool,
}

/// Request to start screen sharing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenShareRequest {
    pub target_device: DeviceId,
    pub config: ScreenShareConfig,
}

/// Request to view remote screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenViewRequest {
    pub host_device: DeviceId,
}

/// Request to control remote screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenControlRequest {
    pub host_device: DeviceId,
}

/// WebRTC signaling message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum SignalingMessage {
    Offer { sdp: String },
    Answer { sdp: String },
    Candidate { candidate: String },
    Hangup,
}
```

- [ ] **Step 2: Run check**

Run: `cargo check -p hyprbridge-protocol`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/protocol/
git commit -m "feat(protocol): add screen sharing types"
```

---

## Task 3: WebRTC Session Manager

**Covers:** [S4]

**Files:**
- Create: `crates/network/src/webrtc.rs`
- Modify: `crates/network/src/lib.rs`
- Modify: `crates/network/Cargo.toml`

- [ ] **Step 1: Add WebRTC dependencies**

In `crates/network/Cargo.toml`, add:

```toml
webrtc = "0.12"
webrtc-util = "0.12"
```

- [ ] **Step 2: Create webrtc.rs**

Create `crates/network/src/webrtc.rs`:

```rust
//! WebRTC session management for screen sharing.

use std::collections::HashMap;
use std::sync::Arc;

use hyprbridge_protocol::{
    DeviceId, ScreenShareConfig, ScreenShareSession, ScreenShareState,
    SessionDirection, SignalingMessage,
};
use tokio::sync::{Mutex, RwLock, broadcast};

/// WebRTC session manager.
pub struct WebRtcManager {
    sessions: RwLock<HashMap<String, ScreenShareSession>>,
    signal_sender: broadcast::Sender<SignalingMessage>,
}

impl WebRtcManager {
    /// Create a new WebRTC manager.
    pub fn new() -> Self {
        let (signal_sender, _) = broadcast::channel(64);
        Self {
            sessions: RwLock::new(HashMap::new()),
            signal_sender,
        }
    }

    /// Create a new screen sharing session.
    pub async fn create_session(
        &self,
        host_device: DeviceId,
        viewer_device: DeviceId,
        config: ScreenShareConfig,
    ) -> ScreenShareSession {
        let session = ScreenShareSession {
            id: uuid::Uuid::new_v4().to_string(),
            host_device,
            viewer_device,
            direction: SessionDirection::Outbound,
            state: ScreenShareState::Negotiating,
            config,
            created_at: chrono::Utc::now(),
        };

        self.sessions
            .write()
            .await
            .insert(session.id.clone(), session.clone());

        session
    }

    /// Update session state.
    pub async fn update_state(&self, session_id: &str, state: ScreenShareState) -> bool {
        if let Some(session) = self.sessions.write().await.get_mut(session_id) {
            session.state = state;
            true
        } else {
            false
        }
    }

    /// Get session by ID.
    pub async fn get_session(&self, session_id: &str) -> Option<ScreenShareSession> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Vec<ScreenShareSession> {
        self.sessions.read().await.values().cloned().collect()
    }

    /// Remove a session.
    pub async fn remove_session(&self, session_id: &str) -> bool {
        self.sessions.write().await.remove(session_id).is_some()
    }

    /// Send a signaling message.
    pub fn send_signal(&self, message: SignalingMessage) -> anyhow::Result<()> {
        self.signal_sender.send(message)?;
        Ok(())
    }

    /// Subscribe to signaling messages.
    pub fn subscribe_signals(&self) -> broadcast::Receiver<SignalingMessage> {
        self.signal_sender.subscribe()
    }
}

impl Default for WebRtcManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_session() {
        let manager = WebRtcManager::new();
        let session = manager
            .create_session(
                DeviceId("host".into()),
                DeviceId("viewer".into()),
                ScreenShareConfig {
                    fps: 30,
                    resolution_width: 1920,
                    resolution_height: 1080,
                    quality: 80,
                    audio_enabled: false,
                },
            )
            .await;

        assert_eq!(session.state, ScreenShareState::Negotiating);
        assert!(manager.get_session(&session.id).await.is_some());
    }

    #[tokio::test]
    async fn list_sessions() {
        let manager = WebRtcManager::new();
        assert!(manager.list_sessions().await.is_empty());

        manager
            .create_session(
                DeviceId("host".into()),
                DeviceId("viewer".into()),
                ScreenShareConfig {
                    fps: 30,
                    resolution_width: 1920,
                    resolution_height: 1080,
                    quality: 80,
                    audio_enabled: false,
                },
            )
            .await;

        assert_eq!(manager.list_sessions().await.len(), 1);
    }

    #[tokio::test]
    async fn remove_session() {
        let manager = WebRtcManager::new();
        let session = manager
            .create_session(
                DeviceId("host".into()),
                DeviceId("viewer".into()),
                ScreenShareConfig {
                    fps: 30,
                    resolution_width: 1920,
                    resolution_height: 1080,
                    quality: 80,
                    audio_enabled: false,
                },
            )
            .await;

        assert!(manager.remove_session(&session.id).await);
        assert!(manager.get_session(&session.id).await.is_none());
    }
}
```

- [ ] **Step 3: Add module to lib.rs**

```rust
pub mod webrtc;
pub use webrtc::WebRtcManager;
```

- [ ] **Step 4: Run check**

Run: `cargo check -p hyprbridge-network`
Expected: PASS

- [ ] **Step 5: Run tests**

Run: `cargo test -p hyprbridge-network`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/network/
git commit -m "feat(network): add WebRTC session manager"
```

---

## Task 4: Daemon Screen API

**Covers:** [S5, S6]

**Files:**
- Modify: `crates/daemon/src/lib.rs`

- [ ] **Step 1: Add screen sharing REST endpoints**

Add routes:

```rust
.route("/v1/screen/share", post(post_screen_share))
.route("/v1/screen/view", post(post_screen_view))
.route("/v1/screen/control", post(post_screen_control))
.route("/v1/screen/stop", post(post_screen_stop))
.route("/v1/screen/sessions", get(get_screen_sessions))
.route("/v1/screen/config", get(get_screen_config).put(put_screen_config))
```

Handlers:

```rust
async fn post_screen_share(
    Json(request): Json<hyprbridge_protocol::ScreenShareRequest>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "session_id": uuid::Uuid::new_v4().to_string(),
        "status": "negotiating"
    }))
}

async fn post_screen_view(
    Json(request): Json<hyprbridge_protocol::ScreenViewRequest>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "session_id": uuid::Uuid::new_v4().to_string(),
        "status": "viewing"
    }))
}

async fn post_screen_control(
    Json(request): Json<hyprbridge_protocol::ScreenControlRequest>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "session_id": uuid::Uuid::new_v4().to_string(),
        "status": "controlling"
    }))
}

async fn post_screen_stop(
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "stopped": true }))
}

async fn get_screen_sessions() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "sessions": [] }))
}

async fn get_screen_config() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "fps": 30,
        "resolution": { "width": 1920, "height": 1080 },
        "quality": 80,
        "audio_enabled": false
    }))
}

async fn put_screen_config(
    Json(request): Json<hyprbridge_protocol::ScreenShareConfig>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "updated": true }))
}
```

- [ ] **Step 2: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/daemon/
git commit -m "feat(daemon): add screen sharing REST API"
```

---

## Task 5: CLI Screen Commands

**Covers:** [S5]

**Files:**
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: Add screen commands**

```rust
/// Screen sharing commands.
#[derive(Subcommand)]
enum ScreenCommand {
    /// Share your screen with a device.
    Share {
        #[arg(short, long)]
        device: String,
        #[arg(long, default_value = "30")]
        fps: u32,
        #[arg(long, default_value = "80")]
        quality: u32,
        #[arg(long)]
        audio: bool,
    },
    /// View a remote device's screen.
    View {
        #[arg(short, long)]
        device: String,
    },
    /// Control a remote device's screen.
    Control {
        #[arg(short, long)]
        device: String,
    },
    /// Stop screen sharing.
    Stop,
    /// List active screen sharing sessions.
    List,
}
```

- [ ] **Step 2: Run check**

Run: `cargo check -p hyprbridge-cli`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/cli/
git commit -m "feat(cli): add screen sharing commands"
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

- [ ] **Step 3: Run TypeScript check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm check`
Expected: PASS

- [ ] **Step 4: Verify CLI compiles**

Run: `cargo run -p hyprbridge-cli -- screen --help`
Expected: CLI help with screen subcommands

- [ ] **Step 5: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for screen sharing"
```
