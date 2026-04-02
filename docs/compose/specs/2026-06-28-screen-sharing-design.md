# Screen Sharing / Remote Desktop Design

## [S1] Problem

HyprBridge only shares input (keyboard/mouse) between devices. Users cannot see or control remote screens. This limits the tool's utility for remote collaboration and technical support scenarios.

## [S2] Solution Overview

WebRTC-based screen sharing with view and control modes:
- Screen capture with configurable FPS, resolution, quality
- WebRTC P2P video/audio streaming
- Signaling via existing relay server
- Adaptive encoding (H.264/VP8, hardware acceleration)
- View-only and control modes
- LAN and remote (relay) support

## [S3] Screen Capture Module

New crate `crates/screen-capture`:

```rust
pub struct CaptureConfig {
    pub fps: u32,
    pub resolution: Resolution,
    pub quality: u32,
    pub encode_video: bool,
    pub capture_audio: bool,
}

pub struct CaptureFrame {
    pub timestamp: u64,
    pub video_data: Vec<u8>,
    pub audio_data: Option<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

pub trait ScreenCapture: Send + Sync {
    fn start(&mut self, config: CaptureConfig) -> Result<()>;
    fn stop(&mut self);
    fn capture_frame(&mut self) -> Result<CaptureFrame>;
    fn set_config(&mut self, config: CaptureConfig) -> Result<()>;
}
```

**Files:** `crates/screen-capture/Cargo.toml`, `crates/screen-capture/src/lib.rs`

## [S4] WebRTC Signaling

Extend relay server with WebRTC signaling:

```rust
pub enum SignalingMessage {
    Offer { sdp: String },
    Answer { sdp: String },
    Candidate { candidate: String },
    Hangup,
}

pub struct WebRtcSession {
    pub id: String,
    pub peer: DeviceId,
    pub direction: SessionDirection,
    pub state: SessionState,
    pub video_track: Option<VideoTrack>,
    pub audio_track: Option<AudioTrack>,
}
```

**Files:** `crates/relay-server/src/lib.rs`, `crates/network/src/webrtc.rs`

## [S5] CLI Commands

```bash
hyprbridge-cli screen share --device <id>
hyprbridge-cli screen share --device <id> --audio
hyprbridge-cli screen view --device <id>
hyprbridge-cli screen control --device <id>
hyprbridge-cli screen stop
hyprbridge-cli screen list
```

**Files:** `crates/cli/src/main.rs`

## [S6] REST API

```bash
POST   /v1/screen/share
POST   /v1/screen/view
POST   /v1/screen/control
POST   /v1/screen/stop
GET    /v1/screen/sessions
GET    /v1/screen/config
PUT    /v1/screen/config
```

**Files:** `crates/daemon/src/lib.rs`

## [S7] Web UI

New "Screen Sharing" panel:
- Remote screen preview (WebRTC video element)
- Share/view/control buttons
- Config panel (FPS, resolution, quality, audio)
- Session status display

**Files:** `packages/ui/src/screen-sharing-panel.tsx`

## [S8] Testing Strategy

- Unit tests for screen capture frame generation
- Integration test: two devices sharing screen via WebRTC
- Manual test: LAN screen sharing
- Performance test: measure latency and frame rate

## [S9] Non-Goals

- Multi-monitor support (v2)
- File transfer during screen sharing (v2)
- Recording screen sharing sessions (v2)
- Annotation/drawing tools (v2)
