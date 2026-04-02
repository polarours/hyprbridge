//! Shared protocol contracts for HyprBridge.

use std::borrow::Cow;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable device identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(pub String);

impl DeviceId {
    /// Create a new random device identifier.
    #[must_use]
    pub fn random() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Parse and validate a device ID string as a UUID.
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        let _ = Uuid::parse_str(s)?;
        Ok(Self(s.to_string()))
    }
}

/// Runtime role for a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceRole {
    /// The machine with the locally attached keyboard/mouse.
    Controller,
    /// A machine that can receive ownership and injected input.
    Target,
}

/// Linux desktop session family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinuxSessionKind {
    /// X11 session.
    X11,
    /// Wayland session.
    Wayland,
    /// Not a Linux session or not detected.
    Unknown,
}

/// Wayland support confidence level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaylandSupportLevel {
    /// No useful support is available.
    Unsupported,
    /// Some capability exists but not enough for seamless control.
    Partial,
    /// The environment looks compatible with the current backend.
    Supported,
}

/// Generic support state for a platform feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    /// The feature is available now.
    Supported,
    /// The feature is not available in the current environment.
    Unsupported,
    /// The project knows about the feature but does not implement it fully yet.
    Planned,
}

/// Permission state surfaced to users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    /// All required permissions appear to be granted.
    Granted,
    /// A permission is required but appears to be missing.
    Missing,
    /// The runtime cannot determine the permission state.
    Unknown,
}

/// Device identity summary shared across network and control surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    /// Stable device identifier.
    pub device_id: DeviceId,
    /// Human-friendly device name.
    pub name: String,
    /// Advertised role.
    pub role: DeviceRole,
    /// Hex-encoded public key fingerprint.
    pub public_key: String,
}

/// Unique identifier for a pairing flow.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PairingId(pub String);

impl PairingId {
    #[must_use]
    pub fn random() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

/// One-time pairing code shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCode {
    /// Human-readable short code.
    pub value: String,
    /// Expiration timestamp.
    pub expires_at: DateTime<Utc>,
}

/// Status of a pending pairing flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingStatus {
    /// Waiting for a remote request to use the code.
    AwaitingRequest,
    /// A remote request was received and needs local approval.
    AwaitingApproval,
    /// The flow completed successfully.
    Paired,
    /// The flow expired or was cancelled.
    Expired,
}

/// Known capability snapshot for a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityReport {
    /// Operating system family string.
    pub platform: String,
    /// Linux session type when relevant.
    pub linux_session: LinuxSessionKind,
    /// Wayland support level when relevant.
    pub wayland_support: WaylandSupportLevel,
    /// Capability to capture local input globally.
    pub input_capture: SupportLevel,
    /// Capability to inject remote input.
    pub input_injection: SupportLevel,
    /// Capability to transfer ownership at screen boundaries.
    pub cursor_handoff: SupportLevel,
    /// Permission posture.
    pub permission_state: PermissionState,
    /// Human-readable notes.
    pub notes: Vec<String>,
}

impl CapabilityReport {
    /// Create a generic placeholder capability report.
    #[must_use]
    pub fn planned(platform: impl Into<String>) -> Self {
        Self {
            platform: platform.into(),
            linux_session: LinuxSessionKind::Unknown,
            wayland_support: WaylandSupportLevel::Unsupported,
            input_capture: SupportLevel::Planned,
            input_injection: SupportLevel::Planned,
            cursor_handoff: SupportLevel::Planned,
            permission_state: PermissionState::Unknown,
            notes: Vec::new(),
        }
    }
}

/// Snapshot of the local transport runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportSnapshot {
    /// Locally advertised transport profile.
    pub profile: TransportProfile,
    /// Active or recent session records.
    pub sessions: Vec<TransportSession>,
}

/// QUIC transport profile for a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportProfile {
    /// mDNS service type.
    pub service_type: String,
    /// QUIC listen address.
    pub quic_addr: String,
    /// SHA-256 digest of the advertised QUIC certificate.
    pub certificate_sha256: String,
    /// Base64-encoded DER certificate for peer trust bootstrapping.
    pub certificate_der_b64: String,
}

/// Direction of a transport session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionDirection {
    /// An inbound connection from another device.
    Inbound,
    /// An outbound connection initiated locally.
    Outbound,
}

/// Current transport session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// QUIC handshake completed and the peer was accepted.
    Connected,
    /// Peer was rejected, usually because it was untrusted.
    Rejected,
    /// Connection failed before it became active.
    Failed,
    /// Connection was closed after previously being active.
    Closed,
}

/// Metadata about a transport session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportSession {
    /// Peer identity if known.
    pub peer: Option<DeviceIdentity>,
    /// Direction of the session.
    pub direction: SessionDirection,
    /// Current state.
    pub state: SessionState,
    /// Remote socket address string.
    pub remote_addr: String,
    /// When the session state last changed.
    pub updated_at: DateTime<Utc>,
    /// Human-readable detail.
    pub detail: String,
}

/// Basic status payload returned by the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    /// Local device identity.
    pub identity: DeviceIdentity,
    /// Runtime capability snapshot.
    pub capability: CapabilityReport,
    /// Number of trusted peers.
    pub trusted_peers: usize,
    /// Number of discovered peers.
    pub discovered_peers: usize,
    /// Current ownership state summary.
    pub ownership: OwnershipSnapshot,
    /// Current topology summary.
    pub topology: TopologySnapshot,
    /// Local native input backend runtime snapshot when available.
    pub input_backend: Option<InputBackendSnapshot>,
    /// Transport session and endpoint status.
    pub transport: TransportSnapshot,
    /// Whether mDNS discovery is currently operational.
    pub mdns_healthy: bool,
    /// Last update timestamp.
    pub observed_at: DateTime<Utc>,
}

/// Topology edge identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenEdge {
    /// Left edge.
    Left,
    /// Right edge.
    Right,
    /// Top edge.
    Top,
    /// Bottom edge.
    Bottom,
}

/// Topology mapping summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TopologySnapshot {
    /// Local edge bindings.
    pub edges: Vec<EdgeMapping>,
    /// Dead zone in pixels.
    pub dead_zone_px: u16,
}

/// Pointer position and root screen extent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointerSnapshot {
    /// Pointer x position in root coordinates.
    pub x: i32,
    /// Pointer y position in root coordinates.
    pub y: i32,
    /// Root screen width.
    pub width: u16,
    /// Root screen height.
    pub height: u16,
}

/// Local input backend runtime state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputBackendSnapshot {
    /// Backend identifier such as `x11`.
    pub backend: String,
    /// Whether the backend is currently usable.
    pub available: bool,
    /// Current pointer snapshot when available.
    pub pointer: Option<PointerSnapshot>,
    /// Current edge candidate inside the configured dead zone.
    pub edge_candidate: Option<ScreenEdge>,
    /// Optional controller-side capture runtime details.
    pub controller_capture: Option<ControllerCaptureSnapshot>,
    /// Optional target-side injection runtime details.
    pub target_injection: Option<TargetInjectionSnapshot>,
    /// Whether target-side wheel injection is available on this backend.
    pub target_wheel_injection: SupportLevel,
    /// Human-readable detail.
    pub detail: String,
}

/// Controller-side capture runtime state for backends that support it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerCaptureSnapshot {
    /// Number of readable controller input devices contributing state.
    pub readable_devices: usize,
    /// Number of devices currently grabbed for exclusive relay use.
    pub grabbed_devices: usize,
    /// Whether the controller is currently trying to hold exclusive capture.
    pub exclusive_capture_active: bool,
}

/// Target-side injection runtime details for the active backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetInjectionSnapshot {
    /// Preferred injection backend currently selected.
    pub primary_backend: String,
    /// Optional fallback backend when the primary path is unavailable.
    pub fallback_backend: Option<String>,
    /// Whether `/dev/uinput` is currently writable when relevant.
    pub uinput_writable: bool,
    /// Last known runtime error from the preferred path when available.
    pub last_error: Option<String>,
}

/// An edge-to-device mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeMapping {
    /// Local screen edge.
    pub local_edge: ScreenEdge,
    /// Target device.
    pub remote_device: DeviceId,
    /// Expected remote entry edge.
    pub remote_edge: ScreenEdge,
}

/// Ownership state summary shared with CLIs and UIs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnershipSnapshot {
    /// Current state.
    pub state: OwnershipState,
    /// Optional active remote device.
    pub active_remote: Option<DeviceId>,
}

/// Ownership control state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipState {
    /// Local machine owns the pointer and keyboard.
    LocalActive,
    /// The pointer is crossing an edge and awaiting transfer.
    CrossingEdge,
    /// A remote machine owns input.
    RemoteActive,
    /// The system is restoring local ownership.
    ReturningLocal,
    /// Ownership is paused due to an error or user lock.
    Suspended,
}

/// Discovery record shown in control surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    /// Remote identity.
    pub identity: DeviceIdentity,
    /// Host:port endpoint summary.
    pub endpoint: String,
    /// QUIC transport details discovered over LAN.
    pub transport: TransportProfile,
    /// Summary of remote capability posture.
    pub capability: CapabilityReport,
    /// Whether the device is currently pairable.
    pub pairable: bool,
}

/// Trust record persisted locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRecord {
    /// Trusted device identity.
    pub identity: DeviceIdentity,
    /// When the trust relation was created.
    pub paired_at: DateTime<Utc>,
    /// Free-form note such as the approval mechanism.
    pub note: String,
}

/// Request payload to start a pairing code locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairStartRequest {
    /// How long the code should remain valid, in seconds.
    pub ttl_seconds: u64,
}

/// Response payload for a local pairing code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairStartResponse {
    /// Unique pairing flow identifier.
    pub pairing_id: PairingId,
    /// Generated code.
    pub code: PairingCode,
    /// Current state.
    pub status: PairingStatus,
}

/// Remote pairing request submitted against an active code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequest {
    /// Requesting device identity.
    pub device: DeviceIdentity,
    /// One-time code displayed on the target.
    pub code: String,
    /// The pairing flow to submit this request against.
    pub pairing_id: PairingId,
}

/// Request payload for local approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairApproveRequest {
    /// Device ID being approved.
    pub device_id: DeviceId,
}

/// Response for pair approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairApproveResponse {
    /// Whether approval succeeded.
    pub approved: bool,
    /// Current pairing status.
    pub status: PairingStatus,
}

/// Request to cancel an in-progress pairing flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairCancelRequest {
    /// The pairing flow to cancel.
    pub pairing_id: PairingId,
}

/// Response to a pairing cancellation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairCancelResponse {
    /// Whether the pairing flow was successfully cancelled.
    pub cancelled: bool,
}

/// Request to update topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyUpdateRequest {
    /// Desired topology snapshot.
    pub topology: TopologySnapshot,
}

/// Response after updating topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyUpdateResponse {
    /// Persisted topology snapshot.
    pub topology: TopologySnapshot,
}

/// Request to simulate or trigger a connection handoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectRequest {
    /// Remote device to activate.
    pub device_id: DeviceId,
    /// Optional screen edge to cross. Defaults to `Right` when omitted.
    #[serde(default)]
    pub edge: Option<ScreenEdge>,
}

/// Response after a connection request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectResponse {
    /// Updated ownership snapshot.
    pub ownership: OwnershipSnapshot,
}

/// Response after returning ownership to the local machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnLocalResponse {
    /// Updated ownership snapshot.
    pub ownership: OwnershipSnapshot,
}

/// Runtime control command sent over an established trusted transport session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ControlFrame {
    /// Place the remote pointer just inside the declared entry edge.
    PointerEnter { edge: ScreenEdge },
    /// Move the remote pointer relative to its current position.
    PointerMove { delta_x: i16, delta_y: i16 },
    /// Scroll the remote pointer by wheel steps.
    PointerScroll {
        horizontal_steps: i16,
        vertical_steps: i16,
    },
    /// Change a remote pointer button state.
    PointerButton { button: MouseButton, pressed: bool },
    /// Change a remote keyboard key state using a native scan/keycode.
    KeyboardKey { keycode: u8, pressed: bool },
    /// Return ownership to the local machine and stop remote injection.
    ReturnLocal,
    /// Clipboard text update (small payload, fits in control frame).
    ClipboardText { text: Cow<'static, str> },
    /// Notify peer of incoming file transfer.
    FileTransferNotify { header: FileTransferHeader },
}

/// Response to a control command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlAck {
    /// Whether the command was applied successfully.
    pub accepted: bool,
    /// Human-readable result detail.
    pub detail: String,
}

/// Cross-platform mouse button identifier used by control frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "code", rename_all = "snake_case")]
pub enum MouseButton {
    /// Primary/left mouse button.
    Left,
    /// Middle mouse button.
    Middle,
    /// Secondary/right mouse button.
    Right,
    /// Any other native button code.
    Other(u8),
}

/// Clipboard content type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardContentType {
    /// Plain text.
    Text,
    /// Image data (PNG/JPEG).
    Image,
    /// List of file paths.
    FileList,
}

/// Clipboard update header sent over dedicated stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardStreamHeader {
    /// Type of clipboard content.
    pub content_type: ClipboardContentType,
    /// SHA-256 hash of content for deduplication.
    pub content_hash: String,
    /// Total size in bytes.
    pub size_bytes: u64,
    /// Device that copied this content.
    pub source_device: DeviceId,
    /// When the content was copied.
    pub timestamp: DateTime<Utc>,
    /// File names for FileList type.
    pub file_names: Option<Vec<String>>,
}

/// Command sent over a dedicated clipboard QUIC stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardStreamCommand {
    /// Begin a clipboard update stream.
    Begin(ClipboardStreamHeader),
    /// Data chunk (up to 64KB).
    Chunk(Vec<u8>),
    /// End of stream.
    End,
}

/// File transfer metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTransferHeader {
    pub file_name: String,
    pub file_size: u64,
    pub content_hash: String,
    pub source_device: DeviceId,
    pub timestamp: DateTime<Utc>,
    pub mime_type: String,
}

/// Command sent over a dedicated file transfer QUIC stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileTransferCommand {
    Begin(FileTransferHeader),
    Chunk(Vec<u8>),
    End,
    Ack { accepted: bool, detail: String },
}

/// Request to send a file to a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSendRequest {
    pub device_id: DeviceId,
    pub file_path: String,
}

/// Response after initiating a file send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSendResponse {
    pub transfer_id: String,
    pub status: String,
}

/// Batch transfer header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTransferHeader {
    pub batch_id: String,
    pub total_files: usize,
    pub total_bytes: u64,
    pub preserve_structure: bool,
    pub root_name: String,
}

/// Individual file header within a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchFileHeader {
    pub relative_path: String,
    pub file_size: u64,
    pub content_hash: String,
}

/// Batch transfer end marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTransferEnd {
    pub batch_id: String,
    pub total_files: usize,
    pub overall_hash: String,
}

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

/// Device group information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceGroup {
    pub id: String,
    pub name: String,
    pub members: Vec<DeviceId>,
    pub created_at: DateTime<Utc>,
}

/// Request to create a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
}

/// Response after creating a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupResponse {
    pub group: DeviceGroup,
}

/// Request to join a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinGroupRequest {
    pub group_id: String,
}

/// Request to add a device to a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddToGroupRequest {
    pub group_id: String,
    pub device_id: DeviceId,
}

/// Request to remove a device from a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveFromGroupRequest {
    pub group_id: String,
    pub device_id: DeviceId,
}

/// Live event payloads from the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum EventEnvelope {
    /// Full status update.
    Status(Box<ServiceStatus>),
    /// Discovery update.
    Discovery(Vec<DiscoveredDevice>),
    /// Pairing state update.
    Pairing(PairStartResponse),
    /// Log line.
    Log(LogEntry),
}

/// Structured log entry for display surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp.
    pub at: DateTime<Utc>,
    /// Severity level.
    pub level: String,
    /// Logger target.
    pub target: String,
    /// Message body.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::DeviceId;

    #[test]
    fn valid_uuid_parse() {
        let id = DeviceId::parse("550e8400-e29b-41d4-a716-446655440000");
        assert!(id.is_ok());
    }

    #[test]
    fn invalid_uuid_rejected() {
        let id = DeviceId::parse("not-a-uuid");
        assert!(id.is_err());
    }
}
