//! HyprBridge daemon runtime and local control API.

use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use http::Method;
use hyprbridge_clipboard::{
    ClipboardContent, ClipboardHistory, ClipboardHistoryConfig, ClipboardManager,
    create_platform_clipboard,
};
use hyprbridge_config::{AppConfig, AppPaths};
use hyprbridge_core::{OwnershipEvent, OwnershipMachine, TopologyLayout};
use hyprbridge_diagnostics::{DoctorReport, build_doctor_report};
use hyprbridge_filetransfer::FileManager;
use hyprbridge_input::{CapabilityProbe, ControllerSample};
use hyprbridge_network::{
    ControlCommandHandler, DiscoveryIndex, LocalIdentity, MdnsRuntime, PairingCoordinator,
    QuicTransport, RelayClient, TrustStore,
};
use hyprbridge_protocol::{
    CapabilityReport, ClipboardContentType, ConnectRequest, ConnectResponse, ControlAck,
    ControlFrame, DiscoveredDevice, EventEnvelope, LogEntry, MouseButton, PairApproveRequest,
    PairApproveResponse, PairCancelRequest, PairCancelResponse, PairRequest, PairStartRequest,
    PairStartResponse, ReturnLocalResponse, ScreenEdge, ServiceStatus, TopologySnapshot,
    TopologyUpdateRequest, TopologyUpdateResponse, TransportSnapshot,
};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, sleep};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{Level, info, warn};

/// Shared daemon application state.
pub struct DaemonState {
    identity: LocalIdentity,
    capability: CapabilityReport,
    discovery: Arc<DiscoveryIndex>,
    trust_store: Arc<TrustStore>,
    pairing: PairingCoordinator,
    topology: RwLock<TopologyLayout>,
    ownership: Mutex<OwnershipMachine>,
    auto_handoff_cooldown: Mutex<Option<(ScreenEdge, Instant)>>,
    remote_relay: Mutex<Option<JoinHandle<()>>>,
    transport: QuicTransport,
    _mdns: Option<MdnsRuntime>,
    mdns_healthy: RwLock<bool>,
    logs: RwLock<Vec<LogEntry>>,
    events: broadcast::Sender<EventEnvelope>,
    clipboard: Arc<ClipboardManager>,
    clipboard_history: Arc<RwLock<ClipboardHistory>>,
    file_manager: Arc<FileManager>,
    relay_client: Arc<RwLock<Option<RelayClient>>>,
}

struct DaemonControlHandler {
    clipboard: Arc<ClipboardManager>,
}

#[async_trait]
impl ControlCommandHandler for DaemonControlHandler {
    async fn handle(
        &self,
        peer: &hyprbridge_protocol::DeviceIdentity,
        frame: ControlFrame,
    ) -> ControlAck {
        match frame {
            ControlFrame::PointerEnter { edge } => {
                #[cfg(target_os = "linux")]
                if hyprbridge_platform_linux::place_pointer_on_entry(edge).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!(
                            "placed remote pointer for trusted peer {} on {:?} edge",
                            peer.device_id.0, edge
                        ),
                    };
                }
                #[cfg(target_os = "macos")]
                if hyprbridge_platform_mac::macos_place_pointer_on_entry(edge).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!(
                            "placed remote pointer for trusted peer {} on {:?} edge",
                            peer.device_id.0, edge
                        ),
                    };
                }
                #[cfg(target_os = "windows")]
                if hyprbridge_platform_win::win_place_pointer_on_entry(edge).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!(
                            "placed remote pointer for trusted peer {} on {:?} edge",
                            peer.device_id.0, edge
                        ),
                    };
                }
                ControlAck {
                    accepted: false,
                    detail: "no native target backend could place the pointer".into(),
                }
            }
            ControlFrame::PointerMove { delta_x, delta_y } => {
                #[cfg(target_os = "linux")]
                if hyprbridge_platform_linux::inject_pointer_relative(delta_x, delta_y).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!("applied pointer delta ({delta_x}, {delta_y})"),
                    };
                }
                #[cfg(target_os = "macos")]
                if hyprbridge_platform_mac::macos_inject_pointer_relative(delta_x, delta_y).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!("applied pointer delta ({delta_x}, {delta_y})"),
                    };
                }
                #[cfg(target_os = "windows")]
                if hyprbridge_platform_win::win_inject_pointer_relative(delta_x, delta_y).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!("applied pointer delta ({delta_x}, {delta_y})"),
                    };
                }
                ControlAck {
                    accepted: false,
                    detail: "no native target backend could inject pointer motion".into(),
                }
            }
            ControlFrame::PointerScroll {
                horizontal_steps,
                vertical_steps,
            } => {
                #[cfg(target_os = "linux")]
                if hyprbridge_platform_linux::inject_pointer_scroll(
                    horizontal_steps,
                    vertical_steps,
                )
                .await
                {
                    return ControlAck {
                        accepted: true,
                        detail: format!(
                            "applied pointer scroll ({horizontal_steps}, {vertical_steps})"
                        ),
                    };
                }
                #[cfg(target_os = "macos")]
                if hyprbridge_platform_mac::macos_inject_pointer_scroll(
                    horizontal_steps,
                    vertical_steps,
                )
                .await
                {
                    return ControlAck {
                        accepted: true,
                        detail: format!(
                            "applied pointer scroll ({horizontal_steps}, {vertical_steps})"
                        ),
                    };
                }
                #[cfg(target_os = "windows")]
                if hyprbridge_platform_win::win_inject_pointer_scroll(
                    horizontal_steps,
                    vertical_steps,
                )
                .await
                {
                    return ControlAck {
                        accepted: true,
                        detail: format!(
                            "applied pointer scroll ({horizontal_steps}, {vertical_steps})"
                        ),
                    };
                }
                ControlAck {
                    accepted: false,
                    detail: "no native target backend could inject pointer scrolling".into(),
                }
            }
            ControlFrame::PointerButton { button, pressed } => {
                #[cfg(target_os = "linux")]
                if hyprbridge_platform_linux::inject_pointer_button(button, pressed).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!("set pointer button {:?} to {pressed}", button),
                    };
                }
                #[cfg(target_os = "macos")]
                if hyprbridge_platform_mac::macos_inject_pointer_button(button, pressed).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!("set pointer button {:?} to {pressed}", button),
                    };
                }
                #[cfg(target_os = "windows")]
                if hyprbridge_platform_win::win_inject_pointer_button(button, pressed).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!("set pointer button {:?} to {pressed}", button),
                    };
                }
                ControlAck {
                    accepted: false,
                    detail: "no native target backend could inject pointer buttons".into(),
                }
            }
            ControlFrame::KeyboardKey { keycode, pressed } => {
                #[cfg(target_os = "linux")]
                if hyprbridge_platform_linux::inject_keyboard_key(keycode, pressed).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!("set keycode {keycode} to {pressed}"),
                    };
                }
                #[cfg(target_os = "macos")]
                if hyprbridge_platform_mac::macos_inject_keyboard_key(keycode, pressed).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!("set keycode {keycode} to {pressed}"),
                    };
                }
                #[cfg(target_os = "windows")]
                if hyprbridge_platform_win::win_inject_keyboard_key(keycode, pressed).await {
                    return ControlAck {
                        accepted: true,
                        detail: format!("set keycode {keycode} to {pressed}"),
                    };
                }
                ControlAck {
                    accepted: false,
                    detail: "no native target backend could inject keyboard events".into(),
                }
            }
            ControlFrame::ReturnLocal => {
                #[cfg(target_os = "linux")]
                {
                    return ControlAck {
                        accepted: hyprbridge_platform_linux::focus_local_target().await,
                        detail: "return-local acknowledged and target focus restored".into(),
                    };
                }
                #[cfg(target_os = "macos")]
                {
                    return ControlAck {
                        accepted: hyprbridge_platform_mac::macos_focus_local_target().await,
                        detail: "return-local acknowledged and target focus restored".into(),
                    };
                }
                #[cfg(target_os = "windows")]
                {
                    return ControlAck {
                        accepted: hyprbridge_platform_win::win_focus_local_target().await,
                        detail: "return-local acknowledged and target focus restored".into(),
                    };
                }
                #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
                ControlAck {
                    accepted: true,
                    detail: "return-local acknowledged".into(),
                }
            }
            ControlFrame::ClipboardText { text } => {
                let content = ClipboardContent {
                    content_type: ClipboardContentType::Text,
                    data: text.into_owned().into_bytes(),
                    file_names: None,
                };
                match self
                    .clipboard
                    .apply_remote(content, peer.device_id.clone())
                    .await
                {
                    Ok(true) => ControlAck {
                        accepted: true,
                        detail: "clipboard text applied".into(),
                    },
                    Ok(false) => ControlAck {
                        accepted: true,
                        detail: "clipboard text already up to date".into(),
                    },
                    Err(e) => ControlAck {
                        accepted: false,
                        detail: format!("failed to apply clipboard: {e}"),
                    },
                }
            }
            ControlFrame::FileTransferNotify { header } => {
                tracing::info!(
                    "incoming file transfer from {}: {} ({} bytes)",
                    peer.device_id.0,
                    header.file_name,
                    header.file_size
                );
                ControlAck {
                    accepted: true,
                    detail: format!(
                        "file transfer notification received for {}",
                        header.file_name
                    ),
                }
            }
        }
    }
}

impl DaemonState {
    async fn new(config: AppConfig, paths: &AppPaths) -> Result<Self> {
        let identity =
            LocalIdentity::load_or_create(&paths.identity, &config.device_name, config.role)
                .await?;
        let capability = probe_capability();
        let discovery = Arc::new(DiscoveryIndex::new());
        let trust_store = Arc::new(TrustStore::load(paths.trust_store.clone()).await?);
        let api_bind: SocketAddr = config
            .local_api_bind
            .parse()
            .with_context(|| format!("invalid bind address `{}`", config.local_api_bind))?;
        let platform = create_platform_clipboard();
        let clipboard = Arc::new(ClipboardManager::new(
            identity.public_identity().device_id.clone(),
            platform,
        ));
        let transport = QuicTransport::start(
            derive_quic_bind_addr(api_bind),
            &paths.transport_cert,
            &paths.transport_key,
            identity.public_identity(),
            Arc::clone(&trust_store),
            Some(Arc::new(DaemonControlHandler {
                clipboard: Arc::clone(&clipboard),
            })),
        )
        .await?;
        let mdns = match MdnsRuntime::start(
            identity.public_identity(),
            capability.clone(),
            Arc::clone(&discovery),
            &transport,
        )
        .await
        {
            Ok(runtime) => Some(runtime),
            Err(error) => {
                warn!("mDNS runtime could not start: {error}");
                None
            }
        };
        let topology = config.topology_layout()?;
        let (events, _) = broadcast::channel(128);
        let mdns_healthy = mdns.is_some();
        let download_dir = std::env::temp_dir().join("HyprBridge").join("downloads");
        let file_manager = Arc::new(FileManager::new(download_dir));

        let history_config = ClipboardHistoryConfig {
            max_size: config.clipboard.history_size,
            persist: config.clipboard.persist_history,
            persist_path: config
                .clipboard
                .history_path
                .clone()
                .map(std::path::PathBuf::from),
        };
        let clipboard_history = Arc::new(RwLock::new(ClipboardHistory::new(history_config)));

        Ok(Self {
            identity,
            capability,
            discovery,
            trust_store,
            pairing: PairingCoordinator::default(),
            topology: RwLock::new(topology),
            ownership: Mutex::new(OwnershipMachine::default()),
            auto_handoff_cooldown: Mutex::new(None),
            remote_relay: Mutex::new(None),
            transport,
            _mdns: mdns,
            mdns_healthy: RwLock::new(mdns_healthy),
            logs: RwLock::new(Vec::new()),
            events,
            clipboard,
            clipboard_history,
            file_manager,
            relay_client: Arc::new(RwLock::new(None)),
        })
    }

    async fn status(&self) -> ServiceStatus {
        let discovered_peers = self.discovery.list().await;
        let dead_zone_px = self.topology.read().await.dead_zone_px;
        ServiceStatus {
            identity: self.identity.public_identity(),
            capability: self.capability.clone(),
            trusted_peers: self.trust_store.all().await.len(),
            discovered_peers: discovered_peers.len(),
            ownership: self.ownership.lock().await.snapshot(),
            topology: self.topology.read().await.snapshot(),
            input_backend: inspect_input_backend(dead_zone_px).await,
            transport: self.transport.snapshot().await,
            mdns_healthy: *self.mdns_healthy.read().await,
            observed_at: Utc::now(),
        }
    }

    async fn broadcast(&self, event: EventEnvelope) {
        let _ = self.events.send(event);
    }

    async fn log(&self, level: &str, target: &str, message: impl Into<String>) {
        let entry = LogEntry {
            at: Utc::now(),
            level: level.into(),
            target: target.into(),
            message: message.into(),
        };
        self.logs.write().await.push(entry.clone());
        self.broadcast(EventEnvelope::Log(entry)).await;
    }
}

async fn inspect_input_backend(
    dead_zone_px: u16,
) -> Option<hyprbridge_protocol::InputBackendSnapshot> {
    #[cfg(target_os = "linux")]
    {
        hyprbridge_platform_linux::inspect_input_backend(dead_zone_px).await
    }
    #[cfg(target_os = "macos")]
    {
        hyprbridge_platform_mac::macos_inspect_input_backend(dead_zone_px).await
    }
    #[cfg(target_os = "windows")]
    {
        hyprbridge_platform_win::win_inspect_input_backend(dead_zone_px).await
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Run the daemon with the default application config path.
pub async fn serve_default() -> Result<()> {
    let paths = AppPaths::resolve()?;
    let config = if paths.config.exists() {
        AppConfig::load(&paths.config).await?
    } else {
        let config = AppConfig::default();
        config.save(&paths.config).await?;
        config
    };

    serve(config, paths).await
}

/// Run the daemon with an explicit config and app paths.
pub async fn serve(config: AppConfig, paths: AppPaths) -> Result<()> {
    let bind_addr: SocketAddr = config
        .local_api_bind
        .parse()
        .with_context(|| format!("invalid bind address `{}`", config.local_api_bind))?;
    let cors_origins = config.cors_origins.clone();
    let state = Arc::new(DaemonState::new(config, &paths).await?);

    state
        .log("info", "daemon", "HyprBridge daemon starting")
        .await;
    state
        .broadcast(EventEnvelope::Status(Box::new(state.status().await)))
        .await;
    spawn_edge_monitor(Arc::clone(&state));

    let clipboard_clone = Arc::clone(&state.clipboard);
    let history_clone = Arc::clone(&state.clipboard_history);
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_millis(500)).await;
            if let Some(event) = clipboard_clone.poll_once().await {
                tracing::info!("clipboard changed locally");
                if let hyprbridge_clipboard::ClipboardEvent::LocalChanged(content) = event {
                    let mut history = history_clone.write().await;
                    history.add(
                        content.content_type.clone(),
                        content.data.clone(),
                        clipboard_clone.build_header(&content).source_device,
                    );
                }
            }
        }
    });

    let app = Router::new()
        .route("/v1/status", get(get_status))
        .route("/v1/discovery", get(get_discovery))
        .route("/v1/transport", get(get_transport))
        .route("/v1/pair/start", post(post_pair_start))
        .route("/v1/pair/request", post(post_pair_request))
        .route("/v1/pair/approve", post(post_pair_approve))
        .route("/v1/pair/cancel", post(post_pair_cancel))
        .route("/v1/topology", get(get_topology).put(put_topology))
        .route("/v1/connect", post(post_connect))
        .route("/v1/ownership/return", post(post_return_local))
        .route("/v1/diagnostics", get(get_diagnostics))
        .route("/v1/peripherals", get(get_peripherals))
        .route("/v1/workspace/info", get(get_workspace_info))
        .route("/v1/workspace/files", get(get_workspace_files))
        .route("/v1/workspace/file", get(get_workspace_file))
        .route("/v1/logs", get(get_logs))
        .route("/v1/events", get(get_events))
        .route("/v1/clipboard", get(get_clipboard))
        .route("/v1/clipboard/push", post(post_clipboard_push))
        .route("/v1/clipboard/history", get(get_clipboard_history))
        .route("/v1/clipboard/history/pin", post(post_clipboard_pin))
        .route("/v1/clipboard/history/unpin", post(post_clipboard_unpin))
        .route("/v1/clipboard/history/clear", post(post_clipboard_clear))
        .route("/v1/transfer/send", post(post_transfer_send))
        .route("/v1/transfer/send-folder", post(post_transfer_send_folder))
        .route("/v1/transfer/send-batch", post(post_transfer_send_batch))
        .route("/v1/transfer/batch/{id}", get(get_transfer_batch_status))
        .route("/v1/transfer/status", get(get_transfer_status))
        .route("/v1/relay/status", get(get_relay_status))
        .route("/v1/relay/connect", post(post_relay_connect))
        .route("/v1/relay/disconnect", post(post_relay_disconnect))
        .route("/v1/groups", get(get_groups).post(post_create_group))
        .route("/v1/groups/{id}/join", post(post_join_group))
        .route("/v1/groups/{id}/leave", post(post_leave_group))
        .route("/v1/groups/{id}/members", get(get_group_members))
        .route("/v1/groups/{id}/add", post(post_add_to_group))
        .route("/v1/groups/{id}/remove", post(post_remove_from_group))
        .route("/v1/plugins", get(get_plugins))
        .route("/v1/plugins/install", post(post_plugin_install))
        .route("/v1/plugins/uninstall", post(post_plugin_uninstall))
        .route("/v1/plugins/{id}/enable", post(post_plugin_enable))
        .route("/v1/plugins/{id}/disable", post(post_plugin_disable))
        .route("/v1/plugins/{id}", get(get_plugin_info))
        .route("/v1/screen/share", post(post_screen_share))
        .route("/v1/screen/view", post(post_screen_view))
        .route("/v1/screen/control", post(post_screen_control))
        .route("/v1/screen/stop", post(post_screen_stop))
        .route("/v1/screen/sessions", get(get_screen_sessions))
        .route(
            "/v1/screen/config",
            get(get_screen_config).put(put_screen_config),
        )
        .route("/v1/record/start", post(post_record_start))
        .route("/v1/record/stop", post(post_record_stop))
        .route("/v1/record/list", get(get_record_list))
        .route("/v1/record/play", post(post_record_play))
        .route("/v1/record/pause", post(post_record_pause))
        .route("/v1/record/stop-playback", post(post_record_stop_playback))
        .route(
            "/v1/record/{id}",
            get(get_record_info).delete(delete_record),
        )
        .route("/v1/sync/status", get(get_sync_status))
        .route("/v1/sync/push", post(post_sync_push))
        .route("/v1/sync/pull", post(post_sync_pull))
        .route("/v1/sync/devices", get(get_sync_devices))
        .route("/v1/sync/history", get(get_sync_history))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer({
            let mut cors = CorsLayer::new().allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
            ]);
            for origin in &cors_origins {
                if origin.contains('*') {
                    cors = cors.allow_origin(tower_http::cors::Any);
                    break;
                } else if let Ok(val) = origin.parse::<http::HeaderValue>() {
                    cors = cors.allow_origin(val);
                }
            }
            cors
        });

    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .try_init()
        .ok();

    let listener = TcpListener::bind(bind_addr).await?;
    info!("HyprBridge daemon listening on http://{bind_addr}");
    axum::serve(listener, app)
        .await
        .context("daemon server failed")
}

async fn get_status(State(state): State<Arc<DaemonState>>) -> Json<ServiceStatus> {
    Json(state.status().await)
}

async fn get_discovery(State(state): State<Arc<DaemonState>>) -> Json<Vec<DiscoveredDevice>> {
    Json(state.discovery.list().await)
}

async fn get_transport(State(state): State<Arc<DaemonState>>) -> Json<TransportSnapshot> {
    Json(state.transport.snapshot().await)
}

async fn post_pair_start(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<PairStartRequest>,
) -> Json<PairStartResponse> {
    let response = state.pairing.start(request.ttl_seconds.max(30)).await;
    state
        .log(
            "info",
            "pairing",
            format!("Started pairing code {}", response.code.value),
        )
        .await;
    state
        .broadcast(EventEnvelope::Pairing(response.clone()))
        .await;
    Json(response)
}

async fn post_pair_request(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<PairRequest>,
) -> Result<Json<PairStartResponse>, ApiError> {
    let response = state
        .pairing
        .submit_request(request.device.clone(), &request.code, &request.pairing_id)
        .await?;
    state
        .log(
            "info",
            "pairing",
            format!(
                "Received pairing request from {}",
                request.device.device_id.0
            ),
        )
        .await;
    state
        .broadcast(EventEnvelope::Pairing(response.clone()))
        .await;
    Ok(Json(response))
}

async fn post_pair_approve(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<PairApproveRequest>,
) -> Result<Json<PairApproveResponse>, ApiError> {
    let response = state
        .pairing
        .approve(request.clone(), state.trust_store.as_ref())
        .await?;
    state
        .log(
            "info",
            "pairing",
            format!("Approved device {}", request.device_id.0),
        )
        .await;
    state
        .broadcast(EventEnvelope::Status(Box::new(state.status().await)))
        .await;
    Ok(Json(response))
}

async fn post_pair_cancel(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<PairCancelRequest>,
) -> Json<PairCancelResponse> {
    let cancelled = state.pairing.cancel(&request.pairing_id).await;
    Json(PairCancelResponse { cancelled })
}

async fn get_topology(State(state): State<Arc<DaemonState>>) -> Json<TopologySnapshot> {
    Json(state.topology.read().await.snapshot())
}

async fn put_topology(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<TopologyUpdateRequest>,
) -> Result<Json<TopologyUpdateResponse>, ApiError> {
    let layout =
        TopologyLayout::try_from_mappings(request.topology.edges, request.topology.dead_zone_px)?;
    let snapshot = layout.snapshot();
    *state.topology.write().await = layout;
    state.log("info", "topology", "Updated topology").await;
    state
        .broadcast(EventEnvelope::Status(Box::new(state.status().await)))
        .await;
    Ok(Json(TopologyUpdateResponse { topology: snapshot }))
}

async fn post_connect(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<ConnectRequest>,
) -> Result<Json<ConnectResponse>, ApiError> {
    if !state.trust_store.contains(&request.device_id).await {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            format!("device {} is not trusted", request.device_id.0),
        ));
    }

    let peer = state
        .discovery
        .by_device_id(&request.device_id)
        .await
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                format!(
                    "device {} is not currently discoverable",
                    request.device_id.0
                ),
            )
        })?;

    state
        .transport
        .connect_to_peer(&peer, state.identity.public_identity())
        .await?;
    if let Some(remote_edge) = resolve_remote_entry_edge(&state, &request.device_id).await {
        let _ = state
            .transport
            .send_control_frame(
                &request.device_id,
                &ControlFrame::PointerEnter { edge: remote_edge },
            )
            .await;
    }

    let mut machine = state.ownership.lock().await;
    let edge = request
        .edge
        .unwrap_or(hyprbridge_protocol::ScreenEdge::Right);
    machine.apply(OwnershipEvent::EdgeCrossed(edge));
    let snapshot = machine.apply(OwnershipEvent::RemoteAccepted(request.device_id.clone()));
    drop(machine);

    state
        .log(
            "info",
            "ownership",
            format!("Transferred ownership to {}", request.device_id.0),
        )
        .await;
    spawn_remote_input_relay(Arc::clone(&state), request.device_id.clone()).await;
    state
        .broadcast(EventEnvelope::Status(Box::new(state.status().await)))
        .await;
    Ok(Json(ConnectResponse {
        ownership: snapshot,
    }))
}

async fn post_return_local(
    State(state): State<Arc<DaemonState>>,
) -> Result<Json<ReturnLocalResponse>, ApiError> {
    let active_remote = state
        .ownership
        .lock()
        .await
        .snapshot()
        .active_remote
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::CONFLICT,
                "no remote device currently owns input",
            )
        })?;

    {
        let mut relay = state.remote_relay.lock().await;
        if let Some(handle) = relay.take() {
            handle.abort();
        }
    }
    maybe_disable_remote_capture(&state).await;

    let held_inputs = controller_sample().await;
    if let Some(sample) = held_inputs.as_ref() {
        release_remote_inputs(&state, &active_remote, sample).await?;
    }

    let _ = state
        .transport
        .send_control_frame(&active_remote, &ControlFrame::ReturnLocal)
        .await;

    let mut machine = state.ownership.lock().await;
    // Two-step FSM transition: RemoteActive -> ReturningLocal -> LocalActive
    machine.apply(OwnershipEvent::ReturnLocal);
    let snapshot = machine.apply(OwnershipEvent::ReturnLocal);
    drop(machine);

    let _ = recenter_controller_pointer().await;
    state
        .log(
            "info",
            "ownership",
            format!(
                "Returned ownership to local machine from {}",
                active_remote.0
            ),
        )
        .await;
    state
        .broadcast(EventEnvelope::Status(Box::new(state.status().await)))
        .await;
    Ok(Json(ReturnLocalResponse {
        ownership: snapshot,
    }))
}

/// Build a workspace service rooted at the current user's home directory.
fn workspace_service() -> Result<hyprbridge_workspace::WorkspaceService, StatusCode> {
    let home = std::env::var("HOME").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    hyprbridge_workspace::WorkspaceService::new(home).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Local inspection and desktop-capability assessment.
async fn get_workspace_info() -> Json<serde_json::Value> {
    use hyprbridge_workspace::{assess_desktop, inspect_local, workspace_capability};

    let inspection = inspect_local();
    let desktop = assess_desktop(&inspection);
    let capability = workspace_capability(&inspection.environment);
    Json(serde_json::json!({
        "inspection": inspection,
        "desktop": desktop,
        "capability": capability,
    }))
}

/// List a directory inside the workspace root.
async fn get_workspace_files(
    Query(query): Query<hyprbridge_workspace::PathQuery>,
) -> Result<Json<Vec<hyprbridge_workspace::DirectoryEntry>>, StatusCode> {
    let service = workspace_service()?;
    service
        .list_dir(query.path.as_deref())
        .map(Json)
        .map_err(|_| StatusCode::BAD_REQUEST)
}

/// Read a text file inside the workspace root.
async fn get_workspace_file(
    Query(query): Query<hyprbridge_workspace::PathQuery>,
) -> Result<Json<hyprbridge_workspace::FileDocument>, StatusCode> {
    let path = query.path.ok_or(StatusCode::BAD_REQUEST)?;
    let service = workspace_service()?;
    service
        .read_text(&path)
        .map(Json)
        .map_err(|_| StatusCode::BAD_REQUEST)
}

/// Local peripheral devices, receivers and diagnostics.
async fn get_peripherals() -> Json<hyprbridge_peripherals::PeripheralSnapshot> {
    Json(hyprbridge_peripherals::snapshot())
}

async fn get_diagnostics(State(state): State<Arc<DaemonState>>) -> Json<DoctorReport> {
    let dead_zone_px = state.topology.read().await.dead_zone_px;
    let input_backend = inspect_input_backend(dead_zone_px).await;
    Json(build_doctor_report(
        &state.capability,
        input_backend.as_ref(),
    ))
}

async fn get_logs(State(state): State<Arc<DaemonState>>) -> Json<Vec<LogEntry>> {
    Json(state.logs.read().await.clone())
}

async fn get_events(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DaemonState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| events_socket(socket, state))
}

async fn events_socket(mut socket: WebSocket, state: Arc<DaemonState>) {
    let initial = EventEnvelope::Status(Box::new(state.status().await));
    if let Ok(raw) = serde_json::to_string(&initial) {
        let _ = socket.send(Message::Text(raw.into())).await;
    }

    let mut receiver = state.events.subscribe();
    while let Ok(event) = receiver.recv().await {
        match serde_json::to_string(&event) {
            Ok(raw) => {
                if socket.send(Message::Text(raw.into())).await.is_err() {
                    break;
                }
            }
            Err(error) => {
                let _ = socket
                    .send(Message::Text(
                        format!("{{\"type\":\"error\",\"payload\":\"{error}\"}}").into(),
                    ))
                    .await;
                break;
            }
        }
    }
}

async fn get_clipboard(State(state): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    let last_hash = state.clipboard.last_hash().await;
    Json(serde_json::json!({
        "last_hash": last_hash,
    }))
}

async fn post_clipboard_push(State(state): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    let _ = state.clipboard.poll_once().await;
    Json(serde_json::json!({
        "pushed": true,
    }))
}

async fn get_clipboard_history(State(state): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    let history = state.clipboard_history.read().await;
    let entries: Vec<_> = history
        .entries()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "preview": e.preview,
                "content_type": format!("{:?}", e.content_type).to_lowercase(),
                "timestamp": e.timestamp.to_rfc3339(),
                "pinned": e.pinned,
            })
        })
        .collect();
    Json(serde_json::json!({ "entries": entries }))
}

async fn post_clipboard_pin(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let id = request["id"].as_str().unwrap_or("");
    let mut history = state.clipboard_history.write().await;
    Json(serde_json::json!({ "pinned": history.pin(id) }))
}

async fn post_clipboard_unpin(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let id = request["id"].as_str().unwrap_or("");
    let mut history = state.clipboard_history.write().await;
    Json(serde_json::json!({ "unpinned": history.unpin(id) }))
}

async fn post_clipboard_clear(State(state): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    state.clipboard_history.write().await.clear();
    Json(serde_json::json!({ "cleared": true }))
}

fn spawn_edge_monitor(state: Arc<DaemonState>) {
    if !cfg!(target_os = "linux") && !cfg!(target_os = "macos") && !cfg!(target_os = "windows") {
        return;
    }

    tokio::spawn(async move {
        loop {
            sleep(Duration::from_millis(120)).await;
            if let Err(error) = monitor_edge_once(Arc::clone(&state)).await {
                state
                    .log(
                        "warn",
                        "edge-monitor",
                        format!("edge monitor error: {error}"),
                    )
                    .await;
            }
        }
    });
}

async fn monitor_edge_once(state: Arc<DaemonState>) -> Result<()> {
    let dead_zone_px = state.topology.read().await.dead_zone_px;
    let candidate = hyprbridge_platform_linux::current_edge_candidate(dead_zone_px).await;

    if candidate.is_none() {
        *state.auto_handoff_cooldown.lock().await = None;
        return Ok(());
    }

    let edge = candidate.expect("checked above");
    if auto_handoff_is_cooling_down(&state, edge).await {
        return Ok(());
    }

    if state.ownership.lock().await.snapshot().state
        != hyprbridge_protocol::OwnershipState::LocalActive
    {
        return Ok(());
    }

    let mapping = match state.topology.read().await.route_for(edge) {
        Ok(mapping) => mapping.clone(),
        Err(_) => return Ok(()),
    };

    if !state.trust_store.contains(&mapping.remote_device).await {
        return Ok(());
    }

    let peer = match state.discovery.by_device_id(&mapping.remote_device).await {
        Some(peer) => peer,
        None => return Ok(()),
    };

    match state
        .transport
        .connect_to_peer(&peer, state.identity.public_identity())
        .await
    {
        Ok(_) => {
            let _ = state
                .transport
                .send_control_frame(
                    &mapping.remote_device,
                    &ControlFrame::PointerEnter {
                        edge: mapping.remote_edge,
                    },
                )
                .await;
            let _ = hyprbridge_platform_linux::perform_local_handoff(edge).await;
            let mut machine = state.ownership.lock().await;
            machine.apply(OwnershipEvent::EdgeCrossed(edge));
            machine.apply(OwnershipEvent::RemoteAccepted(
                mapping.remote_device.clone(),
            ));
            drop(machine);
            *state.auto_handoff_cooldown.lock().await = Some((edge, Instant::now()));
            state
                .log(
                    "info",
                    "edge-monitor",
                    format!(
                        "Auto-handoff triggered on {:?} edge toward {}",
                        edge, mapping.remote_device.0
                    ),
                )
                .await;
            spawn_remote_input_relay(Arc::clone(&state), mapping.remote_device.clone()).await;
            state
                .broadcast(EventEnvelope::Status(Box::new(state.status().await)))
                .await;
        }
        Err(error) => {
            *state.auto_handoff_cooldown.lock().await = Some((edge, Instant::now()));
            state
                .log(
                    "warn",
                    "edge-monitor",
                    format!(
                        "Failed auto-handoff on {:?} edge toward {}: {error}",
                        edge, mapping.remote_device.0
                    ),
                )
                .await;
        }
    }

    Ok(())
}

async fn auto_handoff_is_cooling_down(state: &DaemonState, edge: ScreenEdge) -> bool {
    const EDGE_COOLDOWN: Duration = Duration::from_millis(900);

    match *state.auto_handoff_cooldown.lock().await {
        Some((last_edge, last_at)) => last_edge == edge && last_at.elapsed() < EDGE_COOLDOWN,
        None => false,
    }
}

async fn resolve_remote_entry_edge(
    state: &DaemonState,
    device_id: &hyprbridge_protocol::DeviceId,
) -> Option<ScreenEdge> {
    state
        .topology
        .read()
        .await
        .bindings
        .values()
        .find(|mapping| &mapping.remote_device == device_id)
        .map(|mapping| mapping.remote_edge)
}

async fn spawn_remote_input_relay(
    state: Arc<DaemonState>,
    device_id: hyprbridge_protocol::DeviceId,
) {
    if !cfg!(target_os = "linux") && !cfg!(target_os = "macos") && !cfg!(target_os = "windows") {
        return;
    }

    {
        let mut relay = state.remote_relay.lock().await;
        if let Some(handle) = relay.take() {
            handle.abort();
        }
    }
    maybe_enable_remote_capture(&state).await;

    let task_state = Arc::clone(&state);
    let handle = tokio::spawn(async move {
        if let Err(error) = run_remote_input_relay(Arc::clone(&task_state), device_id.clone()).await
        {
            task_state
                .log(
                    "warn",
                    "remote-relay",
                    format!("Remote relay for {} stopped: {error}", device_id.0),
                )
                .await;
            restore_local_ownership(task_state, device_id).await;
        }
    });
    *state.remote_relay.lock().await = Some(handle);
}

async fn run_remote_input_relay(
    state: Arc<DaemonState>,
    device_id: hyprbridge_protocol::DeviceId,
) -> Result<()> {
    let mut previous = controller_sample()
        .await
        .context("local controller backend is unavailable")?;
    let anchor = anchor_from_pointer(&previous.pointer);
    if !recenter_controller_pointer().await {
        return Err(anyhow::anyhow!(
            "failed to recenter local controller pointer for remote relay"
        ));
    }
    previous.pointer.x = i32::from(anchor.0);
    previous.pointer.y = i32::from(anchor.1);

    loop {
        sleep(Duration::from_millis(16)).await;
        if !ownership_matches_remote(&state, &device_id).await {
            return Ok(());
        }
        if !state.transport.has_active_session(&device_id).await {
            return Err(anyhow::anyhow!(
                "transport session for {} is no longer active",
                device_id.0
            ));
        }

        let mut current = controller_sample()
            .await
            .context("controller sample is no longer available")?;

        let delta_x = current.pointer.x - i32::from(anchor.0);
        let delta_y = current.pointer.y - i32::from(anchor.1);
        if delta_x != 0 || delta_y != 0 {
            state
                .transport
                .send_control_frame(
                    &device_id,
                    &ControlFrame::PointerMove {
                        delta_x: clamp_i32_to_i16(delta_x),
                        delta_y: clamp_i32_to_i16(delta_y),
                    },
                )
                .await
                .context("failed to relay pointer motion")?;
            if !recenter_controller_pointer().await {
                return Err(anyhow::anyhow!(
                    "failed to recenter local pointer after motion relay"
                ));
            }
            current.pointer.x = i32::from(anchor.0);
            current.pointer.y = i32::from(anchor.1);
        }

        relay_button_changes(
            &state,
            &device_id,
            previous.buttons_mask,
            current.buttons_mask,
        )
        .await?;
        relay_scroll_changes(&state, &device_id, &current).await?;
        relay_key_changes(&state, &device_id, &previous.keymap, &current.keymap).await?;
        previous = current;
    }
}

async fn controller_sample() -> Option<ControllerSample> {
    #[cfg(target_os = "linux")]
    {
        hyprbridge_platform_linux::controller_sample().await
    }
    #[cfg(target_os = "macos")]
    {
        hyprbridge_platform_mac::macos_controller_sample().await
    }
    #[cfg(target_os = "windows")]
    {
        hyprbridge_platform_win::win_controller_sample().await
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

async fn recenter_controller_pointer() -> bool {
    #[cfg(target_os = "linux")]
    {
        hyprbridge_platform_linux::recenter_controller_pointer().await
    }
    #[cfg(target_os = "macos")]
    {
        hyprbridge_platform_mac::macos_recenter_controller_pointer().await
    }
    #[cfg(target_os = "windows")]
    {
        hyprbridge_platform_win::win_recenter_controller_pointer().await
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

async fn ownership_matches_remote(
    state: &DaemonState,
    device_id: &hyprbridge_protocol::DeviceId,
) -> bool {
    let snapshot = state.ownership.lock().await.snapshot();
    snapshot.state == hyprbridge_protocol::OwnershipState::RemoteActive
        && snapshot.active_remote.as_ref() == Some(device_id)
}

async fn restore_local_ownership(
    state: Arc<DaemonState>,
    device_id: hyprbridge_protocol::DeviceId,
) {
    {
        let mut relay = state.remote_relay.lock().await;
        relay.take();
    }
    maybe_disable_remote_capture(&state).await;

    {
        let mut machine = state.ownership.lock().await;
        machine.apply(OwnershipEvent::RemoteDisconnected(device_id.clone()));
    }

    let _ = recenter_controller_pointer().await;
    state
        .log(
            "info",
            "ownership",
            format!("Returned ownership to local machine from {}", device_id.0),
        )
        .await;
    state
        .broadcast(EventEnvelope::Status(Box::new(state.status().await)))
        .await;
}

async fn maybe_enable_remote_capture(state: &DaemonState) {
    #[cfg(target_os = "linux")]
    if let Some(report) = hyprbridge_platform_linux::set_remote_capture_active(true).await {
        state
            .log(
                "info",
                "controller-capture",
                format!(
                    "Activated exclusive Linux controller capture on {}/{} device(s)",
                    report.affected_devices, report.attempted_devices
                ),
            )
            .await;
    }
    #[cfg(target_os = "linux")]
    {
        hyprbridge_platform_linux::grab_pointer_for_remote();
    }
    #[cfg(target_os = "macos")]
    {
        hyprbridge_platform_mac::macos_set_remote_capture_active(true).await;
    }
    #[cfg(target_os = "windows")]
    {
        hyprbridge_platform_win::win_set_remote_capture_active(true).await;
    }
}

async fn maybe_disable_remote_capture(state: &DaemonState) {
    #[cfg(target_os = "linux")]
    if let Some(report) = hyprbridge_platform_linux::set_remote_capture_active(false).await {
        state
            .log(
                "info",
                "controller-capture",
                format!(
                    "Released exclusive Linux controller capture on {}/{} device(s)",
                    report.affected_devices, report.attempted_devices
                ),
            )
            .await;
    }
    #[cfg(target_os = "linux")]
    {
        hyprbridge_platform_linux::ungrab_pointer_for_remote();
    }
    #[cfg(target_os = "macos")]
    {
        hyprbridge_platform_mac::macos_set_remote_capture_active(false).await;
    }
    #[cfg(target_os = "windows")]
    {
        hyprbridge_platform_win::win_set_remote_capture_active(false).await;
    }
}

async fn relay_button_changes(
    state: &Arc<DaemonState>,
    device_id: &hyprbridge_protocol::DeviceId,
    previous_mask: u16,
    current_mask: u16,
) -> Result<()> {
    for (mask_bit, button) in button_mappings() {
        let was_pressed = previous_mask & mask_bit != 0;
        let is_pressed = current_mask & mask_bit != 0;
        if was_pressed != is_pressed {
            state
                .transport
                .send_control_frame(
                    device_id,
                    &ControlFrame::PointerButton {
                        button,
                        pressed: is_pressed,
                    },
                )
                .await
                .with_context(|| format!("failed to relay pointer button {:?}", button))?;
        }
    }
    Ok(())
}

async fn release_remote_inputs(
    state: &Arc<DaemonState>,
    device_id: &hyprbridge_protocol::DeviceId,
    sample: &ControllerSample,
) -> Result<()> {
    for (mask_bit, button) in button_mappings() {
        if sample.buttons_mask & mask_bit != 0 {
            state
                .transport
                .send_control_frame(
                    device_id,
                    &ControlFrame::PointerButton {
                        button,
                        pressed: false,
                    },
                )
                .await
                .with_context(|| format!("failed to release pointer button {:?}", button))?;
        }
    }

    for (byte_index, byte) in sample.keymap.iter().enumerate() {
        if *byte == 0 {
            continue;
        }

        for bit in 0..8_u8 {
            let mask = 1_u8 << bit;
            if byte & mask == 0 {
                continue;
            }

            let keycode = (byte_index as u8).saturating_mul(8).saturating_add(bit);
            if keycode < 8 {
                continue;
            }

            state
                .transport
                .send_control_frame(
                    device_id,
                    &ControlFrame::KeyboardKey {
                        keycode,
                        pressed: false,
                    },
                )
                .await
                .with_context(|| format!("failed to release keycode {keycode}"))?;
        }
    }

    Ok(())
}

async fn relay_scroll_changes(
    state: &Arc<DaemonState>,
    device_id: &hyprbridge_protocol::DeviceId,
    current: &ControllerSample,
) -> Result<()> {
    if current.scroll_horizontal_steps == 0 && current.scroll_vertical_steps == 0 {
        return Ok(());
    }

    state
        .transport
        .send_control_frame(
            device_id,
            &ControlFrame::PointerScroll {
                horizontal_steps: current.scroll_horizontal_steps,
                vertical_steps: current.scroll_vertical_steps,
            },
        )
        .await
        .with_context(|| {
            format!(
                "failed to relay pointer scroll ({}, {})",
                current.scroll_horizontal_steps, current.scroll_vertical_steps
            )
        })?;

    Ok(())
}

async fn relay_key_changes(
    state: &Arc<DaemonState>,
    device_id: &hyprbridge_protocol::DeviceId,
    previous: &[u8; 32],
    current: &[u8; 32],
) -> Result<()> {
    for (byte_index, (&before, &after)) in previous.iter().zip(current.iter()).enumerate() {
        let changed = before ^ after;
        if changed == 0 {
            continue;
        }

        for bit in 0..8_u8 {
            let mask = 1_u8 << bit;
            if changed & mask == 0 {
                continue;
            }

            let keycode = (byte_index as u8).saturating_mul(8).saturating_add(bit);
            if keycode < 8 {
                continue;
            }

            state
                .transport
                .send_control_frame(
                    device_id,
                    &ControlFrame::KeyboardKey {
                        keycode,
                        pressed: after & mask != 0,
                    },
                )
                .await
                .with_context(|| format!("failed to relay keycode {keycode}"))?;
        }
    }
    Ok(())
}

fn anchor_from_pointer(pointer: &hyprbridge_protocol::PointerSnapshot) -> (i16, i16) {
    (
        clamp_i32_to_i16(i32::from(pointer.width / 2)),
        clamp_i32_to_i16(i32::from(pointer.height / 2)),
    )
}

fn button_mappings() -> [(u16, MouseButton); 3] {
    [
        (1 << 8, MouseButton::Left),
        (1 << 9, MouseButton::Middle),
        (1 << 10, MouseButton::Right),
    ]
}

fn clamp_i32_to_i16(value: i32) -> i16 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn probe_capability() -> CapabilityReport {
    if cfg!(target_os = "linux") {
        hyprbridge_platform_linux::probe_local()
    } else if cfg!(target_os = "macos") {
        hyprbridge_platform_mac::MacProbe.probe()
    } else if cfg!(target_os = "windows") {
        hyprbridge_platform_win::WindowsProbe.probe()
    } else {
        CapabilityReport::planned(std::env::consts::OS)
    }
}

fn derive_quic_bind_addr(api_addr: SocketAddr) -> SocketAddr {
    SocketAddr::new(
        match api_addr.ip() {
            std::net::IpAddr::V4(_) => std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            std::net::IpAddr::V6(_) => std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
        },
        api_addr.port().saturating_add(1),
    )
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(value: anyhow::Error) -> Self {
        Self::new(StatusCode::BAD_REQUEST, value.to_string())
    }
}

impl From<hyprbridge_core::TopologyError> for ApiError {
    fn from(value: hyprbridge_core::TopologyError) -> Self {
        Self::new(StatusCode::BAD_REQUEST, value.to_string())
    }
}

async fn post_transfer_send(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<hyprbridge_protocol::FileSendRequest>,
) -> Result<Json<hyprbridge_protocol::FileSendResponse>, ApiError> {
    let path = std::path::PathBuf::from(&request.file_path);
    let transfer_id = state
        .file_manager
        .send_file(request.device_id.clone(), path)
        .await?;
    Ok(Json(hyprbridge_protocol::FileSendResponse {
        transfer_id,
        status: "queued".into(),
    }))
}

async fn get_transfer_status(State(state): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    let transfers = state.file_manager.list_transfers().await;
    Json(serde_json::json!({ "transfers": transfers.len() }))
}

async fn post_transfer_send_folder(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = std::path::PathBuf::from(request["path"].as_str().unwrap_or(""));
    let device_id =
        hyprbridge_protocol::DeviceId(request["device_id"].as_str().unwrap_or("").to_string());
    let preserve = request["preserve_structure"].as_bool().unwrap_or(true);
    let batch_id = state
        .file_manager
        .send_folder(device_id, path, preserve)
        .await?;
    Ok(Json(serde_json::json!({ "batch_id": batch_id })))
}

async fn post_transfer_send_batch(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let files: Vec<String> = request["files"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let device_id =
        hyprbridge_protocol::DeviceId(request["device_id"].as_str().unwrap_or("").to_string());
    let items: Vec<hyprbridge_filetransfer::TransferItem> = files
        .iter()
        .map(|f| hyprbridge_filetransfer::TransferItem::File(std::path::PathBuf::from(f)))
        .collect();
    let batch_id = state
        .file_manager
        .send_batch(device_id, items, true)
        .await?;
    Ok(Json(serde_json::json!({ "batch_id": batch_id })))
}

async fn get_transfer_batch_status(
    State(state): State<Arc<DaemonState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let progress = state.file_manager.batch_progress(&id).await;
    match progress {
        Some(p) => Json(serde_json::json!({
            "id": p.id,
            "total_files": p.total_files,
            "completed_files": p.completed_files,
            "current_file": p.current_file,
            "total_bytes": p.total_bytes,
            "transferred_bytes": p.transferred_bytes,
        })),
        None => Json(serde_json::json!({ "error": "batch not found" })),
    }
}

async fn get_relay_status(State(state): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    let client = state.relay_client.read().await;
    let connected = match client.as_ref() {
        Some(c) => c.is_connected().await,
        None => false,
    };
    Json(serde_json::json!({
        "connected": connected,
        "room_id": None::<String>,
    }))
}

async fn post_relay_connect(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let url = request["url"]
        .as_str()
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "url is required"))?;
    let client = RelayClient::new(url.to_string(), state.identity.public_identity().clone());
    client
        .connect()
        .await
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    *state.relay_client.write().await = Some(client);
    Ok(Json(serde_json::json!({ "connected": true })))
}

async fn post_relay_disconnect(State(state): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    *state.relay_client.write().await = None;
    Json(serde_json::json!({ "disconnected": true }))
}

async fn get_groups() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "groups": [] }))
}

async fn post_create_group(
    Json(request): Json<hyprbridge_protocol::CreateGroupRequest>,
) -> Json<hyprbridge_protocol::CreateGroupResponse> {
    Json(hyprbridge_protocol::CreateGroupResponse {
        group: hyprbridge_protocol::DeviceGroup {
            id: "group-placeholder".into(),
            name: request.name,
            members: vec![],
            created_at: chrono::Utc::now(),
        },
    })
}

async fn post_join_group(Path(_id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "joined": _id }))
}

async fn post_leave_group(Path(_id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "left": _id }))
}

async fn get_group_members(Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "group_id": id, "members": [] }))
}

async fn post_add_to_group(
    Path(_id): Path<String>,
    Json(request): Json<hyprbridge_protocol::AddToGroupRequest>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "added": request.device_id }))
}

async fn post_remove_from_group(
    Path(_id): Path<String>,
    Json(request): Json<hyprbridge_protocol::RemoveFromGroupRequest>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "removed": request.device_id }))
}

async fn get_plugins() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "plugins": [] }))
}

async fn post_plugin_install(Json(_request): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "installed": true }))
}

async fn post_plugin_uninstall(Json(_request): Json<serde_json::Value>) -> Json<serde_json::Value> {
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

async fn post_screen_share(
    Json(_request): Json<hyprbridge_protocol::ScreenShareRequest>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "session_id": uuid::Uuid::new_v4().to_string(),
        "status": "negotiating"
    }))
}

async fn post_screen_view(
    Json(_request): Json<hyprbridge_protocol::ScreenViewRequest>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "session_id": uuid::Uuid::new_v4().to_string(),
        "status": "viewing"
    }))
}

async fn post_screen_control(
    Json(_request): Json<hyprbridge_protocol::ScreenControlRequest>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "session_id": uuid::Uuid::new_v4().to_string(),
        "status": "controlling"
    }))
}

async fn post_screen_stop(Json(_request): Json<serde_json::Value>) -> Json<serde_json::Value> {
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
    Json(_request): Json<hyprbridge_protocol::ScreenShareConfig>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "updated": true }))
}

async fn post_record_start(Json(request): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let name = request["name"].as_str().unwrap_or("untitled");
    Json(serde_json::json!({
        "session_id": uuid::Uuid::new_v4().to_string(),
        "name": name,
        "status": "recording"
    }))
}

async fn post_record_stop() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "stopped" }))
}

async fn get_record_list() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "recordings": [] }))
}

async fn post_record_play(Json(_request): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "playing" }))
}

async fn post_record_pause() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "paused" }))
}

async fn post_record_stop_playback() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "stopped" }))
}

async fn get_record_info(Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": id }))
}

async fn delete_record(Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "deleted": id }))
}

async fn get_sync_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "enabled": false,
        "backend": "none",
        "last_sync": null
    }))
}

async fn post_sync_push(Json(_request): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "pushed": true }))
}

async fn post_sync_pull(Json(_request): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "pulled": true }))
}

async fn get_sync_devices() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "devices": [] }))
}

async fn get_sync_history() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "history": [] }))
}
