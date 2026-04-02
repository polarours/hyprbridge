//! Discovery, identity, pairing, and transport runtime.

pub mod relay_client;
pub use relay_client::RelayClient;

pub mod webrtc;
pub use webrtc::{WebRtcManager, WebRtcSession, WebRtcSessionId, WebRtcSessionState};

use std::{
    borrow::Cow,
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{Duration, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use hyprbridge_protocol::{
    BatchFileHeader, BatchTransferHeader, CapabilityReport, ClipboardStreamCommand,
    ClipboardStreamHeader, ControlAck, ControlFrame, DeviceId, DeviceIdentity, DeviceRole,
    DiscoveredDevice, FileTransferCommand, PairApproveRequest, PairApproveResponse,
    PairStartResponse, PairingCode, PairingId, PairingStatus, SessionDirection, SessionState,
    TransportProfile, TransportSession, TransportSnapshot, TrustRecord,
};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo, TxtProperty};
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig};
use rand::{Rng, rngs::OsRng};
use rcgen::generate_simple_self_signed;
use rustls::{
    RootCertStore,
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    fs,
    sync::{Mutex, RwLock},
};
use tracing::warn;

const DEFAULT_SERVICE_TYPE: &str = "_hyprbridge._udp.local.";
const HANDSHAKE_MAX_BYTES: usize = 16 * 1024;

/// Callback used by the transport layer to apply inbound control commands.
#[async_trait]
pub trait ControlCommandHandler: Send + Sync {
    /// Apply a control frame and return an acknowledgement.
    async fn handle(&self, peer: &DeviceIdentity, frame: ControlFrame) -> ControlAck;
}

#[derive(Debug)]
struct NoopControlHandler;

#[async_trait]
impl ControlCommandHandler for NoopControlHandler {
    async fn handle(&self, _peer: &DeviceIdentity, _frame: ControlFrame) -> ControlAck {
        ControlAck {
            accepted: false,
            detail: "no control handler is registered".into(),
        }
    }
}

/// Persistent local device identity.
#[derive(Debug, Clone)]
pub struct LocalIdentity {
    device_id: DeviceId,
    name: String,
    role: DeviceRole,
    signing_key: SigningKey,
}

impl LocalIdentity {
    /// Load an existing identity or create a new one.
    pub async fn load_or_create(path: &Path, name: &str, role: DeviceRole) -> Result<Self> {
        if path.exists() {
            let raw = fs::read_to_string(path)
                .await
                .with_context(|| format!("failed to read {}", path.display()))?;
            let persisted: StoredIdentity =
                serde_json::from_str(&raw).context("failed to parse identity file")?;

            let bytes =
                hex::decode(persisted.secret_key_hex).context("invalid identity key encoding")?;
            let signing_key = SigningKey::from_bytes(
                &bytes
                    .try_into()
                    .map_err(|_| anyhow!("identity key must be 32 bytes"))?,
            );

            return Ok(Self {
                device_id: persisted.device_id,
                name: persisted.name,
                role: persisted.role,
                signing_key,
            });
        }

        let signing_key = SigningKey::generate(&mut OsRng);
        let identity = Self {
            device_id: DeviceId::random(),
            name: name.into(),
            role,
            signing_key,
        };
        identity.save(path).await?;
        Ok(identity)
    }

    /// Persist the identity to disk.
    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let stored = StoredIdentity {
            device_id: self.device_id.clone(),
            name: self.name.clone(),
            role: self.role,
            secret_key_hex: hex::encode(self.signing_key.to_bytes()),
        };

        fs::write(path, serde_json::to_vec_pretty(&stored)?)
            .await
            .with_context(|| format!("failed to write {}", path.display()))
    }

    /// Public identity shared with other devices.
    #[must_use]
    pub fn public_identity(&self) -> DeviceIdentity {
        DeviceIdentity {
            device_id: self.device_id.clone(),
            name: self.name.clone(),
            role: self.role,
            public_key: hex::encode(self.verifying_key().to_bytes()),
        }
    }

    fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredIdentity {
    device_id: DeviceId,
    name: String,
    role: DeviceRole,
    secret_key_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransportHello {
    device: DeviceIdentity,
    version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransportHelloAck {
    accepted: bool,
    responder: DeviceIdentity,
    detail: String,
}

use dashmap::DashMap;

/// In-memory discovery index keyed by mDNS service fullname.
pub struct DiscoveryIndex {
    peers: DashMap<String, DiscoveredDevice>,
}

impl DiscoveryIndex {
    /// Create an empty discovery index.
    #[must_use]
    pub fn new() -> Self {
        Self {
            peers: DashMap::new(),
        }
    }

    /// Add or update a discovered peer.
    pub async fn upsert_peer(&self, service_fullname: String, peer: DiscoveredDevice) {
        self.peers.insert(service_fullname, peer);
    }

    /// Remove a discovered peer by service fullname.
    pub async fn remove_peer(&self, service_fullname: &str) {
        self.peers.remove(service_fullname);
    }

    /// Find a peer by device id.
    pub async fn by_device_id(&self, device_id: &DeviceId) -> Option<DiscoveredDevice> {
        self.peers
            .iter()
            .find(|peer| &peer.identity.device_id == device_id)
            .map(|peer| peer.value().clone())
    }

    /// List known peers.
    pub async fn list(&self) -> Vec<DiscoveredDevice> {
        let mut peers: Vec<DiscoveredDevice> = self
            .peers
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        peers.sort_by(|left, right| left.identity.name.cmp(&right.identity.name));
        peers
    }
}

impl Default for DiscoveryIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Trusted-device store backed by a JSON file.
#[derive(Debug)]
pub struct TrustStore {
    path: PathBuf,
    entries: RwLock<HashMap<String, TrustRecord>>,
    max_peers: usize,
}

impl TrustStore {
    /// Load an existing trust store from disk with default 32-peer limit.
    pub async fn load(path: PathBuf) -> Result<Self> {
        Self::load_with_limit(path, 32).await
    }

    /// Load an existing trust store from disk with a custom peer limit.
    pub async fn load_with_limit(path: PathBuf, max_peers: usize) -> Result<Self> {
        let entries: Vec<TrustRecord> = if path.exists() {
            let raw = fs::read_to_string(&path)
                .await
                .with_context(|| format!("failed to read {}", path.display()))?;
            serde_json::from_str(&raw).context("failed to parse trust store")?
        } else {
            Vec::new()
        };
        let map: HashMap<String, TrustRecord> = entries
            .into_iter()
            .map(|r| (r.identity.device_id.0.clone(), r))
            .collect();
        Ok(Self {
            path,
            entries: RwLock::new(map),
            max_peers,
        })
    }

    /// Return all trusted peers sorted by pairing time.
    pub async fn all(&self) -> Vec<TrustRecord> {
        let mut entries: Vec<TrustRecord> = self.entries.read().await.values().cloned().collect();
        entries.sort_by_key(|a| a.paired_at);
        entries
    }

    /// Test whether a device is already trusted.
    pub async fn contains(&self, device_id: &DeviceId) -> bool {
        self.entries.read().await.contains_key(&device_id.0)
    }

    /// Add a trusted device if it is not already present.
    /// Evicts the oldest entry when the store is at capacity.
    pub async fn trust(&self, identity: DeviceIdentity, note: &str) -> Result<TrustRecord> {
        let mut entries = self.entries.write().await;
        if let Some(existing) = entries.get(&identity.device_id.0) {
            return Ok(existing.clone());
        }

        if entries.len() >= self.max_peers
            && let Some(oldest_key) = entries
                .values()
                .min_by_key(|r| r.paired_at)
                .map(|r| r.identity.device_id.0.clone())
        {
            entries.remove(&oldest_key);
        }

        let record = TrustRecord {
            identity,
            paired_at: Utc::now(),
            note: note.into(),
        };
        entries.insert(record.identity.device_id.0.clone(), record.clone());
        self.save(&entries).await?;
        Ok(record)
    }

    /// Remove a trusted device by id. Returns true if the device was present.
    pub async fn remove(&self, device_id: &DeviceId) -> bool {
        let mut entries = self.entries.write().await;
        let removed = entries.remove(&device_id.0).is_some();
        if removed {
            let _ = self.save(&entries).await;
        }
        removed
    }

    async fn save(&self, entries: &HashMap<String, TrustRecord>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let mut sorted: Vec<&TrustRecord> = entries.values().collect();
        sorted.sort_by_key(|r| r.paired_at);
        fs::write(&self.path, serde_json::to_vec_pretty(&sorted)?)
            .await
            .with_context(|| format!("failed to write {}", self.path.display()))
    }
}

#[derive(Debug, Clone)]
struct PendingPairing {
    id: PairingId,
    code: PairingCode,
    requester: Option<DeviceIdentity>,
    status: PairingStatus,
}

/// Pairing coordinator that manages concurrent local pairing flows.
#[derive(Debug, Default)]
pub struct PairingCoordinator {
    pending: RwLock<Vec<PendingPairing>>,
}

impl PairingCoordinator {
    /// Start a local pairing code.
    pub async fn start(&self, ttl_seconds: u64) -> PairStartResponse {
        let code = PairingCode {
            value: format!("{:06}", OsRng.gen_range(0_u32..1_000_000)),
            expires_at: Utc::now() + Duration::seconds(ttl_seconds as i64),
        };
        let pairing_id = PairingId::random();

        let pending = PendingPairing {
            id: pairing_id.clone(),
            code: code.clone(),
            requester: None,
            status: PairingStatus::AwaitingRequest,
        };
        self.pending.write().await.push(pending);

        PairStartResponse {
            pairing_id,
            code,
            status: PairingStatus::AwaitingRequest,
        }
    }

    /// Register an inbound remote pairing request.
    pub async fn submit_request(
        &self,
        requester: DeviceIdentity,
        code: &str,
        pairing_id: &PairingId,
    ) -> Result<PairStartResponse> {
        let mut pending = self.pending.write().await;
        let current = pending
            .iter_mut()
            .find(|p| &p.id == pairing_id)
            .context("no pairing flow found for the given pairing id")?;

        if current.status == PairingStatus::Expired {
            return Err(anyhow!("pairing flow has been cancelled"));
        }

        if current.code.expires_at < Utc::now() {
            current.status = PairingStatus::Expired;
            return Err(anyhow!("pairing code has expired"));
        }

        if current.code.value != code {
            return Err(anyhow!("pairing code did not match"));
        }

        current.requester = Some(requester);
        current.status = PairingStatus::AwaitingApproval;

        Ok(PairStartResponse {
            pairing_id: current.id.clone(),
            code: current.code.clone(),
            status: current.status,
        })
    }

    /// Approve the pending request and persist trust.
    pub async fn approve(
        &self,
        request: PairApproveRequest,
        trust_store: &TrustStore,
    ) -> Result<PairApproveResponse> {
        let mut pending = self.pending.write().await;
        let current = pending
            .iter_mut()
            .find(|p| {
                p.status == PairingStatus::AwaitingApproval
                    && p.requester
                        .as_ref()
                        .is_some_and(|r| r.device_id == request.device_id)
            })
            .context("no pairing request is pending for this device")?;

        if current.code.expires_at < Utc::now() {
            current.status = PairingStatus::Expired;
            return Err(anyhow!("pairing code has expired"));
        }

        let requester = current
            .requester
            .clone()
            .context("no requester is pending approval")?;
        trust_store
            .trust(requester, "one-time code with local approval")
            .await?;
        current.status = PairingStatus::Paired;

        Ok(PairApproveResponse {
            approved: true,
            status: current.status,
        })
    }

    /// Cancel an in-progress pairing flow.
    pub async fn cancel(&self, pairing_id: &PairingId) -> bool {
        let mut pending = self.pending.write().await;
        if let Some(current) = pending.iter_mut().find(|p| &p.id == pairing_id) {
            if current.status == PairingStatus::Paired {
                return false;
            }
            current.status = PairingStatus::Expired;
            true
        } else {
            false
        }
    }

    /// Remove expired pairing flows from the active list.
    pub async fn cleanup_expired(&self) {
        let mut pending = self.pending.write().await;
        let now = Utc::now();
        pending.retain(|p| p.code.expires_at > now && p.status != PairingStatus::Expired);
    }
}

/// Real QUIC transport runtime with session tracking.
#[derive(Clone)]
pub struct QuicTransport {
    endpoint: Endpoint,
    profile: TransportProfile,
    sessions: Arc<RwLock<Vec<TransportSession>>>,
    connections: Arc<Mutex<HashMap<String, Connection>>>,
}

impl QuicTransport {
    /// Start a QUIC endpoint and accept loop.
    pub async fn start(
        bind_addr: SocketAddr,
        cert_path: &Path,
        key_path: &Path,
        local_identity: DeviceIdentity,
        trust_store: Arc<TrustStore>,
        command_handler: Option<Arc<dyn ControlCommandHandler>>,
    ) -> Result<Self> {
        let (cert_der, key_der) = load_or_create_transport_identity(cert_path, key_path).await?;
        let mut server_config =
            ServerConfig::with_single_cert(vec![cert_der.clone()], key_der.into())
                .context("failed to build QUIC server config")?;
        if let Some(transport) = Arc::get_mut(&mut server_config.transport) {
            transport.max_concurrent_uni_streams(0_u8.into());
        }

        let endpoint =
            Endpoint::server(server_config, bind_addr).context("failed to bind QUIC endpoint")?;
        let profile = TransportProfile {
            service_type: DEFAULT_SERVICE_TYPE.into(),
            quic_addr: endpoint
                .local_addr()
                .context("failed to read QUIC local address")?
                .to_string(),
            certificate_sha256: certificate_digest(&cert_der),
            certificate_der_b64: BASE64.encode(cert_der.as_ref()),
        };
        let sessions = Arc::new(RwLock::new(Vec::new()));
        let connections = Arc::new(Mutex::new(HashMap::new()));

        let command_handler = command_handler.unwrap_or_else(|| Arc::new(NoopControlHandler));
        spawn_accept_loop(
            endpoint.clone(),
            Arc::clone(&sessions),
            Arc::clone(&connections),
            local_identity,
            trust_store,
            command_handler,
        );

        Ok(Self {
            endpoint,
            profile,
            sessions,
            connections,
        })
    }

    /// Snapshot the local transport state.
    pub async fn snapshot(&self) -> TransportSnapshot {
        TransportSnapshot {
            profile: self.profile.clone(),
            sessions: self.sessions.read().await.clone(),
        }
    }

    /// Access the local profile.
    #[must_use]
    pub fn profile(&self) -> &TransportProfile {
        &self.profile
    }

    /// Open an outbound session to a discovered peer.
    pub async fn connect_to_peer(
        &self,
        peer: &DiscoveredDevice,
        local_identity: DeviceIdentity,
    ) -> Result<TransportSession> {
        let cert_der = BASE64
            .decode(&peer.transport.certificate_der_b64)
            .context("peer certificate in discovery record was not valid base64")?;
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(cert_der))
            .context("failed to trust peer QUIC certificate")?;

        let client_config = ClientConfig::with_root_certificates(Arc::new(roots))
            .context("failed to create QUIC client config")?;
        let remote_addr: SocketAddr =
            peer.transport.quic_addr.parse().with_context(|| {
                format!("invalid peer QUIC address `{}`", peer.transport.quic_addr)
            })?;
        let connection = self
            .endpoint
            .connect_with(client_config, remote_addr, "localhost")
            .context("failed to start QUIC connect")?
            .await
            .context("QUIC handshake failed")?;

        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .context("failed to open QUIC stream")?;
        let hello = TransportHello {
            device: local_identity,
            version: 1,
        };
        send.write_all(&serde_json::to_vec(&hello).context("failed to serialize QUIC hello")?)
            .await
            .context("failed to send QUIC hello")?;
        send.finish()
            .context("failed to finish QUIC hello stream")?;

        let raw_ack = recv
            .read_to_end(HANDSHAKE_MAX_BYTES)
            .await
            .context("failed to read QUIC handshake response")?;
        let ack: TransportHelloAck =
            serde_json::from_slice(&raw_ack).context("failed to parse QUIC handshake response")?;

        let session = TransportSession {
            peer: Some(peer.identity.clone()),
            direction: SessionDirection::Outbound,
            state: if ack.accepted {
                SessionState::Connected
            } else {
                SessionState::Rejected
            },
            remote_addr: remote_addr.to_string(),
            updated_at: Utc::now(),
            detail: ack.detail.clone(),
        };
        self.upsert_session(session.clone()).await;

        if !ack.accepted {
            connection.close(0_u32.into(), b"rejected");
            return Err(anyhow!("peer rejected session: {}", ack.detail));
        }

        self.connections
            .lock()
            .await
            .insert(peer.identity.device_id.0.clone(), connection.clone());
        spawn_connection_monitor(
            connection,
            Arc::clone(&self.sessions),
            Arc::clone(&self.connections),
            peer.identity.clone(),
            SessionDirection::Outbound,
        );

        Ok(session)
    }

    /// Send a control frame to an already connected peer.
    pub async fn send_control_frame(
        &self,
        device_id: &DeviceId,
        frame: &ControlFrame,
    ) -> Result<ControlAck> {
        let connection = self.get_or_create_connection(device_id).await?;

        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .context("failed to open QUIC control stream")?;
        send.write_all(&serde_json::to_vec(frame).context("failed to serialize control frame")?)
            .await
            .context("failed to send QUIC control frame")?;
        send.finish()
            .context("failed to finish QUIC control stream")?;

        let raw = recv
            .read_to_end(HANDSHAKE_MAX_BYTES)
            .await
            .context("failed to read QUIC control acknowledgement")?;
        serde_json::from_slice(&raw).context("failed to parse QUIC control acknowledgement")
    }

    /// Check whether a peer still has an active transport connection.
    pub async fn has_active_session(&self, device_id: &DeviceId) -> bool {
        self.connections.lock().await.contains_key(&device_id.0)
    }

    /// Open a dedicated clipboard stream to a peer.
    pub async fn open_clipboard_stream(
        &self,
        device_id: &DeviceId,
    ) -> Result<(quinn::SendStream, quinn::RecvStream)> {
        let connection = self
            .connections
            .lock()
            .await
            .get(&device_id.0)
            .cloned()
            .ok_or_else(|| anyhow!("device {} has no active session", device_id.0))?;

        let (send, recv) = connection
            .open_bi()
            .await
            .context("failed to open clipboard stream")?;
        Ok((send, recv))
    }

    /// Send clipboard content over a dedicated stream.
    pub async fn send_clipboard_content(
        &self,
        device_id: &DeviceId,
        header: ClipboardStreamHeader,
        data: &[u8],
    ) -> Result<()> {
        let (mut send, _recv) = self.open_clipboard_stream(device_id).await?;

        let begin = ClipboardStreamCommand::Begin(header);
        send.write_all(&serde_json::to_vec(&begin)?)
            .await
            .context("failed to send clipboard stream begin")?;

        const CHUNK_SIZE: usize = 64 * 1024;
        for chunk in data.chunks(CHUNK_SIZE) {
            let cmd = ClipboardStreamCommand::Chunk(chunk.to_vec());
            send.write_all(&serde_json::to_vec(&cmd)?)
                .await
                .context("failed to send clipboard stream chunk")?;
        }

        let end = ClipboardStreamCommand::End;
        send.write_all(&serde_json::to_vec(&end)?)
            .await
            .context("failed to send clipboard stream end")?;
        send.finish()
            .context("failed to finish clipboard stream")?;

        Ok(())
    }

    pub async fn send_file_content(
        &self,
        device_id: &DeviceId,
        header: hyprbridge_protocol::FileTransferHeader,
        data: &[u8],
    ) -> anyhow::Result<()> {
        let (mut send, _recv) = self.open_clipboard_stream(device_id).await?;

        let begin = FileTransferCommand::Begin(header);
        send.write_all(&serde_json::to_vec(&begin)?).await?;

        const CHUNK_SIZE: usize = 64 * 1024;
        for chunk in data.chunks(CHUNK_SIZE) {
            let cmd = FileTransferCommand::Chunk(chunk.to_vec());
            send.write_all(&serde_json::to_vec(&cmd)?).await?;
        }

        let end = FileTransferCommand::End;
        send.write_all(&serde_json::to_vec(&end)?).await?;
        send.finish().context("failed to finish file transfer stream")?;

        Ok(())
    }

    pub async fn send_batch_content(
        &self,
        device_id: &DeviceId,
        header: BatchTransferHeader,
        files: Vec<(String, Vec<u8>)>,
    ) -> anyhow::Result<()> {
        let (mut send, _recv) = self.open_clipboard_stream(device_id).await?;

        let begin = FileTransferCommand::Begin(hyprbridge_protocol::FileTransferHeader {
            file_name: header.root_name.clone(),
            file_size: header.total_bytes,
            content_hash: String::new(),
            source_device: DeviceId::random(),
            timestamp: chrono::Utc::now(),
            mime_type: "application/octet-stream".into(),
        });
        send.write_all(&serde_json::to_vec(&begin)?).await?;

        for (relative_path, data) in &files {
            let file_header = BatchFileHeader {
                relative_path: relative_path.clone(),
                file_size: data.len() as u64,
                content_hash: hyprbridge_filetransfer::compute_hash(data),
            };
            let header_json = serde_json::to_string(&file_header)?;
            send.write_all(&serde_json::to_vec(&FileTransferCommand::Chunk(header_json.into_bytes()))?).await?;

            const CHUNK_SIZE: usize = 64 * 1024;
            for chunk in data.chunks(CHUNK_SIZE) {
                send.write_all(&serde_json::to_vec(&FileTransferCommand::Chunk(chunk.to_vec()))?).await?;
            }
        }

        let end = FileTransferCommand::End;
        send.write_all(&serde_json::to_vec(&end)?).await?;
        send.finish().context("failed to finish batch transfer stream")?;
        Ok(())
    }

    async fn get_or_create_connection(&self, device_id: &DeviceId) -> Result<Connection> {
        self.connections
            .lock()
            .await
            .get(&device_id.0)
            .cloned()
            .ok_or_else(|| anyhow!("no active connection to device {}", device_id.0))
    }

    async fn upsert_session(&self, session: TransportSession) {
        upsert_session_record(&self.sessions, session).await;
    }
}

/// mDNS advertisement and browse runtime.
pub struct MdnsRuntime {
    daemon: ServiceDaemon,
    service_fullname: String,
}

impl MdnsRuntime {
    /// Start mDNS advertisement and browse loop.
    pub async fn start(
        identity: DeviceIdentity,
        capability: CapabilityReport,
        discovery: Arc<DiscoveryIndex>,
        transport: &QuicTransport,
    ) -> Result<Self> {
        let daemon = ServiceDaemon::new().context("failed to create mDNS daemon")?;
        let host_name = format!(
            "hyprbridge-{}.local.",
            identity.device_id.0.replace('-', "")
        );
        let quic_addr: SocketAddr = transport
            .profile()
            .quic_addr
            .parse()
            .context("local QUIC address was invalid")?;
        let properties = announcement_properties(&identity, &capability, transport.profile());
        let service_info = ServiceInfo::new(
            DEFAULT_SERVICE_TYPE,
            &identity.name,
            &host_name,
            "127.0.0.1",
            quic_addr.port(),
            properties,
        )
        .context("failed to assemble mDNS service info")?
        .enable_addr_auto();
        let service_fullname = service_info.get_fullname().to_string();
        daemon
            .register(service_info)
            .context("failed to register mDNS service")?;
        let receiver = daemon
            .browse(DEFAULT_SERVICE_TYPE)
            .context("failed to start mDNS browse")?;
        let local_device_id = identity.device_id.clone();

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        if let Some(peer) = discovered_from_service_info(&info, &local_device_id) {
                            discovery
                                .upsert_peer(info.get_fullname().to_string(), peer)
                                .await;
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        discovery.remove_peer(&fullname).await;
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            daemon,
            service_fullname,
        })
    }
}

impl Drop for MdnsRuntime {
    fn drop(&mut self) {
        let _ = self.daemon.stop_browse(DEFAULT_SERVICE_TYPE);
        let _ = self.daemon.unregister(&self.service_fullname);
        let _ = self.daemon.shutdown();
    }
}

fn announcement_properties(
    identity: &DeviceIdentity,
    capability: &CapabilityReport,
    transport: &TransportProfile,
) -> Vec<TxtProperty> {
    vec![
        TxtProperty::from(("device_id", identity.device_id.0.as_str())),
        TxtProperty::from((
            "role",
            serde_json::to_string(&identity.role).unwrap_or_else(|_| "\"target\"".into()),
        )),
        TxtProperty::from(("platform", capability.platform.as_str())),
        TxtProperty::from((
            "linux_session",
            serde_json::to_string(&capability.linux_session)
                .unwrap_or_else(|_| "\"unknown\"".into()),
        )),
        TxtProperty::from((
            "input_capture",
            serde_json::to_string(&capability.input_capture)
                .unwrap_or_else(|_| "\"planned\"".into()),
        )),
        TxtProperty::from((
            "input_injection",
            serde_json::to_string(&capability.input_injection)
                .unwrap_or_else(|_| "\"planned\"".into()),
        )),
        TxtProperty::from((
            "cursor_handoff",
            serde_json::to_string(&capability.cursor_handoff)
                .unwrap_or_else(|_| "\"planned\"".into()),
        )),
        TxtProperty::from((
            "permission_state",
            serde_json::to_string(&capability.permission_state)
                .unwrap_or_else(|_| "\"unknown\"".into()),
        )),
        TxtProperty::from(("cert_sha256", transport.certificate_sha256.as_str())),
        TxtProperty::from(("cert_der_b64", transport.certificate_der_b64.as_str())),
    ]
}

fn discovered_from_service_info(
    info: &ServiceInfo,
    local_device_id: &DeviceId,
) -> Option<DiscoveredDevice> {
    let device_id = DeviceId(info.get_property_val_str("device_id")?.to_string());
    if &device_id == local_device_id {
        return None;
    }

    let public_key = format!("mdns:{}", info.get_property_val_str("cert_sha256")?);
    let role = parse_json_property(info, "role").unwrap_or(DeviceRole::Target);
    let platform = info
        .get_property_val_str("platform")
        .unwrap_or("unknown")
        .to_string();

    let address = info
        .get_addresses()
        .iter()
        .next()
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|| info.get_hostname().trim_end_matches('.').to_string());
    let quic_addr = format!("{address}:{}", info.get_port());

    Some(DiscoveredDevice {
        identity: DeviceIdentity {
            device_id,
            name: info
                .get_fullname()
                .split('.')
                .next()
                .unwrap_or("Unknown")
                .to_string(),
            role,
            public_key,
        },
        endpoint: quic_addr.clone(),
        transport: TransportProfile {
            service_type: DEFAULT_SERVICE_TYPE.into(),
            quic_addr: quic_addr.clone(),
            certificate_sha256: info
                .get_property_val_str("cert_sha256")
                .unwrap_or_default()
                .to_string(),
            certificate_der_b64: info
                .get_property_val_str("cert_der_b64")
                .unwrap_or_default()
                .to_string(),
        },
        capability: CapabilityReport {
            platform,
            linux_session: parse_json_property(info, "linux_session")
                .unwrap_or(hyprbridge_protocol::LinuxSessionKind::Unknown),
            wayland_support: hyprbridge_protocol::WaylandSupportLevel::Unsupported,
            input_capture: parse_json_property(info, "input_capture")
                .unwrap_or(hyprbridge_protocol::SupportLevel::Planned),
            input_injection: parse_json_property(info, "input_injection")
                .unwrap_or(hyprbridge_protocol::SupportLevel::Planned),
            cursor_handoff: parse_json_property(info, "cursor_handoff")
                .unwrap_or(hyprbridge_protocol::SupportLevel::Planned),
            permission_state: parse_json_property(info, "permission_state")
                .unwrap_or(hyprbridge_protocol::PermissionState::Unknown),
            notes: vec![format!(
                "Discovered via mDNS service `{}`",
                info.get_fullname()
            )],
        },
        pairable: true,
    })
}

fn parse_json_property<T>(info: &ServiceInfo, key: &str) -> Option<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(info.get_property_val_str(key)?).ok()
}

async fn load_or_create_transport_identity(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(CertificateDer<'static>, PrivatePkcs8KeyDer<'static>)> {
    if cert_path.exists() && key_path.exists() {
        let cert = fs::read(cert_path)
            .await
            .with_context(|| format!("failed to read {}", cert_path.display()))?;
        let key = fs::read(key_path)
            .await
            .with_context(|| format!("failed to read {}", key_path.display()))?;
        return Ok((CertificateDer::from(cert), PrivatePkcs8KeyDer::from(key)));
    }

    if let Some(parent) = cert_path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let certified = generate_simple_self_signed(vec!["localhost".into()])
        .context("failed to generate QUIC self-signed certificate")?;
    let cert = CertificateDer::from(certified.cert);
    let key = PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der());

    fs::write(cert_path, cert.as_ref())
        .await
        .with_context(|| format!("failed to write {}", cert_path.display()))?;
    fs::write(key_path, key.secret_pkcs8_der())
        .await
        .with_context(|| format!("failed to write {}", key_path.display()))?;

    Ok((cert, key))
}

fn certificate_digest(cert_der: &CertificateDer<'_>) -> String {
    let digest = Sha256::digest(cert_der.as_ref());
    hex::encode(digest)
}

fn spawn_accept_loop(
    endpoint: Endpoint,
    sessions: Arc<RwLock<Vec<TransportSession>>>,
    connections: Arc<Mutex<HashMap<String, Connection>>>,
    local_identity: DeviceIdentity,
    trust_store: Arc<TrustStore>,
    command_handler: Arc<dyn ControlCommandHandler>,
) {
    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let remote_addr = incoming.remote_address();
            let sessions = Arc::clone(&sessions);
            let connections = Arc::clone(&connections);
            let trust_store = Arc::clone(&trust_store);
            let local_identity = local_identity.clone();
            let command_handler = Arc::clone(&command_handler);
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        if let Err(error) = handle_incoming_connection(
                            connection,
                            remote_addr,
                            sessions,
                            connections,
                            local_identity,
                            trust_store,
                            command_handler,
                        )
                        .await
                        {
                            warn!("failed to handle inbound QUIC session: {error}");
                        }
                    }
                    Err(error) => {
                        upsert_session_record(
                            &sessions,
                            TransportSession {
                                peer: None,
                                direction: SessionDirection::Inbound,
                                state: SessionState::Failed,
                                remote_addr: remote_addr.to_string(),
                                updated_at: Utc::now(),
                                detail: error.to_string(),
                            },
                        )
                        .await;
                    }
                }
            });
        }
    });
}

async fn handle_incoming_connection(
    connection: Connection,
    remote_addr: SocketAddr,
    sessions: Arc<RwLock<Vec<TransportSession>>>,
    connections: Arc<Mutex<HashMap<String, Connection>>>,
    local_identity: DeviceIdentity,
    trust_store: Arc<TrustStore>,
    command_handler: Arc<dyn ControlCommandHandler>,
) -> Result<()> {
    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .context("failed to accept inbound QUIC stream")?;
    let raw = recv
        .read_to_end(HANDSHAKE_MAX_BYTES)
        .await
        .context("failed to read inbound QUIC hello")?;

    if let Ok(ClipboardStreamCommand::Begin(header)) =
        serde_json::from_slice::<ClipboardStreamCommand>(&raw)
    {
        let peer = DeviceIdentity {
            device_id: header.source_device.clone(),
            name: String::new(),
            role: DeviceRole::Target,
            public_key: String::new(),
        };
        handle_clipboard_stream(connection, peer, header, recv, command_handler).await;
        return Ok(());
    }

    if let Ok(FileTransferCommand::Begin(header)) =
        serde_json::from_slice::<FileTransferCommand>(&raw)
    {
        tracing::info!(
            "incoming file transfer: {} from {}",
            header.file_name,
            header.source_device.0
        );
        let mut data = Vec::new();
        while let Ok(raw) = recv.read_to_end(HANDSHAKE_MAX_BYTES * 4).await {
            if let Ok(cmd) = serde_json::from_slice::<FileTransferCommand>(&raw) {
                match cmd {
                    FileTransferCommand::Chunk(chunk) => data.extend(chunk),
                    FileTransferCommand::End => break,
                    _ => {}
                }
            }
        }
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = hex::encode(hasher.finalize());
        if hash != header.content_hash {
            tracing::warn!("file transfer hash mismatch");
        }
        return Ok(());
    }

    let hello: TransportHello =
        serde_json::from_slice(&raw).context("failed to parse inbound QUIC hello")?;

    if !trust_store.contains(&hello.device.device_id).await {
        let detail = "device is not in the local trust store".to_string();
        let ack = TransportHelloAck {
            accepted: false,
            responder: local_identity,
            detail: detail.clone(),
        };
        send.write_all(&serde_json::to_vec(&ack)?)
            .await
            .context("failed to send QUIC rejection")?;
        send.finish().context("failed to finish QUIC rejection")?;
        connection.close(0_u32.into(), b"untrusted");

        upsert_session_record(
            &sessions,
            TransportSession {
                peer: Some(hello.device),
                direction: SessionDirection::Inbound,
                state: SessionState::Rejected,
                remote_addr: remote_addr.to_string(),
                updated_at: Utc::now(),
                detail,
            },
        )
        .await;
        return Ok(());
    }

    let peer = hello.device.clone();
    let ack = TransportHelloAck {
        accepted: true,
        responder: local_identity,
        detail: "session established".into(),
    };
    send.write_all(&serde_json::to_vec(&ack)?)
        .await
        .context("failed to send QUIC acceptance")?;
    send.finish().context("failed to finish QUIC acceptance")?;

    upsert_session_record(
        &sessions,
        TransportSession {
            peer: Some(peer.clone()),
            direction: SessionDirection::Inbound,
            state: SessionState::Connected,
            remote_addr: remote_addr.to_string(),
            updated_at: Utc::now(),
            detail: "accepted trusted peer".into(),
        },
    )
    .await;
    connections
        .lock()
        .await
        .insert(peer.device_id.0.clone(), connection.clone());
    spawn_control_stream_loop(
        connection.clone(),
        peer.clone(),
        Arc::clone(&command_handler),
    );
    spawn_connection_monitor(
        connection,
        sessions,
        connections,
        peer,
        SessionDirection::Inbound,
    );

    Ok(())
}

fn spawn_control_stream_loop(
    connection: Connection,
    peer: DeviceIdentity,
    command_handler: Arc<dyn ControlCommandHandler>,
) {
    tokio::spawn(async move {
        while let Ok((mut send, mut recv)) = connection.accept_bi().await {
            let raw = match recv.read_to_end(HANDSHAKE_MAX_BYTES).await {
                Ok(raw) => raw,
                Err(error) => {
                    warn!(
                        "failed to read control frame from {}: {error}",
                        peer.device_id.0
                    );
                    break;
                }
            };

            let ack = match serde_json::from_slice::<ControlFrame>(&raw) {
                Ok(frame) => command_handler.handle(&peer, frame).await,
                Err(error) => ControlAck {
                    accepted: false,
                    detail: format!("invalid control frame: {error}"),
                },
            };

            let payload = match serde_json::to_vec(&ack) {
                Ok(payload) => payload,
                Err(error) => {
                    warn!(
                        "failed to serialize control ack for {}: {error}",
                        peer.device_id.0
                    );
                    break;
                }
            };

            if send.write_all(&payload).await.is_err() {
                break;
            }
            let _ = send.finish();
        }
    });
}

async fn handle_clipboard_stream(
    _connection: Connection,
    peer: DeviceIdentity,
    header: ClipboardStreamHeader,
    mut recv: quinn::RecvStream,
    command_handler: Arc<dyn ControlCommandHandler>,
) {
    let mut data = Vec::new();
    while let Ok(raw) = recv.read_to_end(HANDSHAKE_MAX_BYTES * 4).await {
        if let Ok(cmd) = serde_json::from_slice::<ClipboardStreamCommand>(&raw) {
            match cmd {
                ClipboardStreamCommand::Chunk(chunk) => data.extend(chunk),
                ClipboardStreamCommand::End => break,
                _ => {}
            }
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hex::encode(hasher.finalize());
    if hash != header.content_hash {
        warn!(
            "clipboard stream hash mismatch for peer {}",
            peer.device_id.0
        );
        return;
    }

    let _ = command_handler
        .handle(
            &peer,
            ControlFrame::ClipboardText {
                text: Cow::Owned(String::from_utf8_lossy(&data).to_string()),
            },
        )
        .await;
}

fn spawn_connection_monitor(
    connection: Connection,
    sessions: Arc<RwLock<Vec<TransportSession>>>,
    connections: Arc<Mutex<HashMap<String, Connection>>>,
    peer: DeviceIdentity,
    direction: SessionDirection,
) {
    tokio::spawn(async move {
        let error = connection.closed().await;
        connections.lock().await.remove(&peer.device_id.0);
        upsert_session_record(
            &sessions,
            TransportSession {
                peer: Some(peer),
                direction,
                state: SessionState::Closed,
                remote_addr: connection.remote_address().to_string(),
                updated_at: Utc::now(),
                detail: error.to_string(),
            },
        )
        .await;
    });
}

async fn upsert_session_record(
    sessions: &Arc<RwLock<Vec<TransportSession>>>,
    session: TransportSession,
) {
    let mut state = sessions.write().await;
    if let Some(peer) = &session.peer
        && let Some(existing) = state.iter_mut().find(|existing| {
            existing.peer.as_ref().map(|item| &item.device_id) == Some(&peer.device_id)
                && existing.direction == session.direction
        })
    {
        *existing = session;
    } else {
        state.push(session);
    }
    state.sort_by_key(|right| std::cmp::Reverse(right.updated_at));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hyprbridge_protocol::{
        CapabilityReport, DeviceId, DeviceIdentity, DeviceRole, DiscoveredDevice, PairingStatus,
        SessionDirection, SessionState,
    };

    use super::{
        DiscoveryIndex, LocalIdentity, PairingCoordinator, QuicTransport, TrustStore,
        load_or_create_transport_identity,
    };

    #[tokio::test]
    async fn pairing_flow_moves_to_approval() {
        let pairing = PairingCoordinator::default();
        let response = pairing.start(60).await;

        let temp_root = std::env::temp_dir().join("hyprbridge-network-tests");
        let local = LocalIdentity::load_or_create(
            &temp_root.join("identity.json"),
            "test",
            DeviceRole::Target,
        )
        .await
        .expect("identity should load");
        let requester = local.public_identity();

        let updated = pairing
            .submit_request(requester.clone(), &response.code.value, &response.pairing_id)
            .await
            .expect("request should be accepted");
        assert_eq!(updated.status, PairingStatus::AwaitingApproval);

        let trust = TrustStore::load(temp_root.join("trust.json"))
            .await
            .expect("trust store should load");
        let approved = pairing
            .approve(
                hyprbridge_protocol::PairApproveRequest {
                    device_id: requester.device_id.clone(),
                },
                &trust,
            )
            .await
            .expect("approval should succeed");

        assert!(approved.approved);
    }

    #[tokio::test]
    async fn pairing_cancel_succeeds() {
        let pairing = PairingCoordinator::default();
        let response = pairing.start(60).await;

        let cancelled = pairing.cancel(&response.pairing_id).await;
        assert!(cancelled);

        let temp_root = std::env::temp_dir().join("hyprbridge-network-tests");
        let local = LocalIdentity::load_or_create(
            &temp_root.join("identity.json"),
            "test",
            DeviceRole::Target,
        )
        .await
        .expect("identity should load");
        let requester = local.public_identity();

        let err = pairing
            .submit_request(requester, &response.code.value, &response.pairing_id)
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn transport_identity_is_persisted() {
        let temp_root = std::env::temp_dir().join("hyprbridge-transport-cert-tests");
        let cert_path = temp_root.join("cert.der");
        let key_path = temp_root.join("key.der");
        let (first_cert, _) = load_or_create_transport_identity(&cert_path, &key_path)
            .await
            .expect("first transport identity should load");
        let (second_cert, _) = load_or_create_transport_identity(&cert_path, &key_path)
            .await
            .expect("second transport identity should load");

        assert_eq!(first_cert.as_ref(), second_cert.as_ref());
    }

    #[tokio::test]
    async fn quic_transport_connects_trusted_peers() {
        let root = std::env::temp_dir().join("hyprbridge-transport-quic-tests");
        let server_identity = LocalIdentity::load_or_create(
            &root.join("server-identity.json"),
            "server",
            DeviceRole::Target,
        )
        .await
        .expect("server identity should load");
        let client_identity = LocalIdentity::load_or_create(
            &root.join("client-identity.json"),
            "client",
            DeviceRole::Controller,
        )
        .await
        .expect("client identity should load");
        let trust = Arc::new(
            TrustStore::load(root.join("server-trust.json"))
                .await
                .expect("server trust store should load"),
        );
        trust
            .trust(client_identity.public_identity(), "test trust")
            .await
            .expect("client should become trusted");

        let server = QuicTransport::start(
            "127.0.0.1:0".parse().expect("socket should parse"),
            &root.join("server-cert.der"),
            &root.join("server-key.der"),
            server_identity.public_identity(),
            Arc::clone(&trust),
            None,
        )
        .await
        .expect("server transport should start");

        let client_trust = Arc::new(
            TrustStore::load(root.join("client-trust.json"))
                .await
                .expect("client trust store should load"),
        );
        let client = QuicTransport::start(
            "127.0.0.1:0".parse().expect("socket should parse"),
            &root.join("client-cert.der"),
            &root.join("client-key.der"),
            client_identity.public_identity(),
            client_trust,
            None,
        )
        .await
        .expect("client transport should start");

        let peer = DiscoveredDevice {
            identity: server_identity.public_identity(),
            endpoint: server.profile().quic_addr.clone(),
            transport: server.profile().clone(),
            capability: CapabilityReport::planned("linux"),
            pairable: true,
        };
        let session = client
            .connect_to_peer(&peer, client_identity.public_identity())
            .await
            .expect("trusted peer should connect");

        assert_eq!(session.direction, SessionDirection::Outbound);
        assert_eq!(session.state, SessionState::Connected);
    }

    #[test]
    fn discovery_index_starts_empty() {
        let index = DiscoveryIndex::new();
        let runtime = tokio::runtime::Runtime::new().expect("runtime should build");
        let peers = runtime.block_on(index.list());
        assert!(peers.is_empty());
    }

    #[tokio::test]
    async fn trust_store_evicts_oldest_when_full() {
        let root = std::env::temp_dir().join("hyprbridge-trust-evict-tests");
        let trust = TrustStore::load_with_limit(root.join("trust.json"), 2)
            .await
            .expect("trust store should load");

        let id1 = DeviceIdentity {
            device_id: DeviceId("peer-1".into()),
            name: "peer1".into(),
            role: DeviceRole::Target,
            public_key: "key1".into(),
        };
        let id2 = DeviceIdentity {
            device_id: DeviceId("peer-2".into()),
            name: "peer2".into(),
            role: DeviceRole::Target,
            public_key: "key2".into(),
        };
        let id3 = DeviceIdentity {
            device_id: DeviceId("peer-3".into()),
            name: "peer3".into(),
            role: DeviceRole::Target,
            public_key: "key3".into(),
        };

        trust.trust(id1.clone(), "first").await.unwrap();
        trust.trust(id2.clone(), "second").await.unwrap();
        trust.trust(id3.clone(), "third").await.unwrap();

        let all = trust.all().await;
        assert_eq!(all.len(), 2);
        assert!(!trust.contains(&id1.device_id).await);
        assert!(trust.contains(&id3.device_id).await);
    }
}
