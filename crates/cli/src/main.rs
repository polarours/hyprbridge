use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use hyprbridge_diagnostics::{CheckStatus, DoctorCheck, DoctorReport};
use hyprbridge_protocol::{
    ConnectRequest, DeviceId, DeviceIdentity, DeviceRole, DiscoveredDevice, PairApproveRequest,
    PairRequest, PairStartRequest, PairingId, ReturnLocalResponse, ScreenEdge, ServiceStatus,
    TopologySnapshot, TopologyUpdateRequest, TransportSnapshot,
};
use reqwest::Client;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    name = "hyprbridge",
    about = "A modern cross-platform keyboard and mouse sharing tool."
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(
        long,
        global = true,
        default_value = "http://127.0.0.1:24800",
        env = "HYPRBRIDGE_API"
    )]
    api: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve,
    Status,
    Discover,
    Transport,
    Doctor,
    Logs,
    Pair {
        #[command(subcommand)]
        command: PairCommand,
    },
    Topology {
        #[command(subcommand)]
        command: TopologyCommand,
    },
    Connect {
        #[arg(long)]
        device_id: String,
    },
    ReturnLocal,
    Clipboard {
        #[command(subcommand)]
        command: ClipboardCommand,
    },
    /// File transfer.
    Transfer {
        #[command(subcommand)]
        command: TransferCommand,
    },
    /// Relay server management.
    Relay {
        #[command(subcommand)]
        command: RelayCommand,
    },
    /// Group management commands.
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    /// Plugin management commands.
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
    /// Screen sharing commands.
    Screen {
        #[command(subcommand)]
        command: ScreenCommand,
    },
    /// Recording management commands.
    Record {
        #[command(subcommand)]
        command: RecordCommand,
    },
    /// Cloud sync commands.
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },
    /// Local peripheral configuration: mice, keyboards and other input devices.
    Peripheral {
        #[command(subcommand)]
        command: PeripheralCommand,
    },
    /// Direct HID++ access for Logitech devices (DPI and report rate).
    Hidpp {
        #[command(subcommand)]
        command: HidppCommand,
    },
    /// Local workspace introspection and sandboxed file access.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// evdev -> uinput button remapping for local peripherals.
    Remap {
        #[command(subcommand)]
        command: RemapCommand,
    },
}

/// Local remapping commands.
#[derive(Debug, Subcommand)]
enum RemapCommand {
    /// Grab a configured device and start remapping.
    Start {
        /// Configured device name.
        #[arg(long, default_value = "Logitech G304")]
        device: String,
        /// Config file (defaults to the HyprBridge config path).
        #[arg(long)]
        config: Option<std::path::PathBuf>,
        /// Validate config and device selection without grabbing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Print raw button codes without grabbing the device.
    Events {
        /// evdev device name or substring.
        #[arg(long, default_value = "Logitech G304")]
        device: String,
    },
}

/// Workspace commands.
#[derive(Debug, Subcommand)]
enum WorkspaceCommand {
    /// Show host/session capabilities and remote-desktop assessment.
    Info,
    /// List a directory inside the workspace root.
    Files {
        /// Path relative to the workspace root.
        #[arg(long)]
        path: Option<String>,
    },
    /// Read a text file inside the workspace root.
    File {
        /// Path relative to the workspace root.
        #[arg(long)]
        path: String,
    },
}

/// HID++ device commands.
#[derive(Debug, Subcommand)]
enum HidppCommand {
    /// Read current DPI and report rate without changing anything.
    Info,
    /// Read or write sensor DPI.
    Dpi {
        /// Set this DPI value, then report the result.
        #[arg(long)]
        set: Option<u16>,
    },
    /// Read or write polling rate.
    Rate {
        /// Set this rate in Hz, then report the result.
        #[arg(long)]
        set: Option<u32>,
    },
}

/// Local peripheral configuration commands.
#[derive(Debug, Subcommand)]
enum PeripheralCommand {
    /// List locally attached input devices.
    List,
    /// Check peripheral permissions, devices and config.
    Doctor,
    /// Validate the peripheral config file.
    Check,
    /// Show the resolved profile for a device and window class.
    Plan {
        /// Configured device name.
        #[arg(long, default_value = "Logitech G304")]
        device: String,
        /// Focused window class (Hyprland/Sway/Niri).
        #[arg(long)]
        window_class: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum PairCommand {
    Start {
        #[arg(long, default_value_t = 300)]
        ttl: u64,
    },
    Request {
        #[arg(long)]
        code: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        device_id: Option<String>,
        #[arg(long, value_enum, default_value_t = RoleArg::Controller)]
        role: RoleArg,
        #[arg(long)]
        pairing_id: String,
    },
    Approve {
        #[arg(long)]
        device_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum TopologyCommand {
    Show,
    Sample {
        #[arg(long, default_value = "remote-demo")]
        device_id: String,
    },
    Set {
        #[arg(long, value_enum)]
        edge: EdgeArg,
        #[arg(long)]
        device_id: String,
        #[arg(long, value_enum)]
        remote_edge: EdgeArg,
        #[arg(long, default_value_t = 12)]
        dead_zone: u16,
    },
}

/// Clipboard management commands.
#[derive(Debug, Subcommand)]
enum ClipboardCommand {
    Status,
    Push,
    History,
    Pin { id: String },
    Unpin { id: String },
    Delete { id: String },
    Clear,
}

/// File transfer commands.
#[derive(Debug, Subcommand)]
enum TransferCommand {
    /// Send a file to a peer.
    Send {
        /// File path to send.
        #[arg(short, long)]
        file: String,
        /// Target device ID.
        #[arg(short, long)]
        device: String,
    },
    /// Send a folder to a peer.
    SendFolder {
        /// Folder path to send.
        #[arg(short, long)]
        folder: String,
        /// Target device ID.
        #[arg(short, long)]
        device: String,
        /// Preserve folder structure on the receiving device.
        #[arg(long, default_value = "true")]
        preserve_structure: bool,
    },
    /// Send multiple files to a peer.
    SendBatch {
        /// File paths to send.
        #[arg(short, long, num_args = 1..)]
        files: Vec<String>,
        /// Target device ID.
        #[arg(short, long)]
        device: String,
    },
    /// Show transfer status.
    Status,
    /// Show status of a specific batch transfer.
    BatchStatus {
        /// Batch transfer ID.
        batch_id: String,
    },
}

/// Relay server commands.
#[derive(Debug, Subcommand)]
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

/// Group management commands.
#[derive(Debug, Subcommand)]
enum GroupCommand {
    Create { name: String },
    Join { group_id: String },
    Leave { group_id: String },
    List,
    Members { group_id: String },
    Add { group_id: String, device_id: String },
    Remove { group_id: String, device_id: String },
}

/// Screen sharing commands.
#[derive(Debug, Subcommand)]
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

/// Plugin management commands.
#[derive(Debug, Subcommand)]
enum PluginCommand {
    List,
    Search { query: String },
    Install { source: String },
    Uninstall { plugin_id: String },
    Enable { plugin_id: String },
    Disable { plugin_id: String },
    Info { plugin_id: String },
}

/// Recording management commands.
#[derive(Debug, Subcommand)]
enum RecordCommand {
    /// Start recording input events.
    Start {
        #[arg(short, long, default_value = "untitled")]
        name: String,
    },
    /// Stop recording.
    Stop,
    /// List recorded sessions.
    List,
    /// Play a recording.
    Play {
        file: String,
        #[arg(short, long, default_value = "1.0")]
        speed: f64,
    },
    /// Show recording information.
    Info { file: String },
    /// Delete a recording.
    Delete { file: String },
}

/// Cloud sync commands.
#[derive(Debug, Subcommand)]
enum SyncCommand {
    /// Show sync status.
    Status,
    /// Push local data to remote.
    Push,
    /// Pull remote data and merge.
    Pull {
        #[arg(short, long)]
        device: Option<String>,
    },
    /// Configure sync backend.
    Configure,
    /// List synced devices.
    Devices,
    /// Show sync history.
    History,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RoleArg {
    Controller,
    Target,
}

impl From<RoleArg> for DeviceRole {
    fn from(value: RoleArg) -> Self {
        match value {
            RoleArg::Controller => DeviceRole::Controller,
            RoleArg::Target => DeviceRole::Target,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EdgeArg {
    Left,
    Right,
    Top,
    Bottom,
}

impl From<EdgeArg> for ScreenEdge {
    fn from(value: EdgeArg) -> Self {
        match value {
            EdgeArg::Left => ScreenEdge::Left,
            EdgeArg::Right => ScreenEdge::Right,
            EdgeArg::Top => ScreenEdge::Top,
            EdgeArg::Bottom => ScreenEdge::Bottom,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::new();

    match cli.command {
        Command::Serve => hyprbridge_daemon::serve_default().await,
        Command::Status => {
            let status: ServiceStatus = get_json(&client, &cli.api, "/v1/status").await?;
            if cli.json {
                print_value(&status, true);
            } else {
                print_status(&status);
            }
            Ok(())
        }
        Command::Discover => {
            let devices: Vec<DiscoveredDevice> =
                get_json(&client, &cli.api, "/v1/discovery").await?;
            if cli.json {
                print_value(&devices, true);
            } else {
                for device in devices {
                    println!(
                        "{}\t{}\tquic={}\tcert={}\tpairable={}",
                        device.identity.name,
                        device.identity.device_id.0,
                        device.transport.quic_addr,
                        device.transport.certificate_sha256,
                        device.pairable
                    );
                }
            }
            Ok(())
        }
        Command::Transport => {
            let transport: TransportSnapshot = get_json(&client, &cli.api, "/v1/transport").await?;
            print_value(&transport, cli.json);
            Ok(())
        }
        Command::Doctor => {
            let report: DoctorReport = get_json(&client, &cli.api, "/v1/diagnostics").await?;
            if cli.json {
                print_value(&report, true);
            } else {
                print_doctor(&report);
            }
            Ok(())
        }
        Command::Logs => {
            let logs: Vec<hyprbridge_protocol::LogEntry> =
                get_json(&client, &cli.api, "/v1/logs").await?;
            print_value(&logs, cli.json);
            Ok(())
        }
        Command::Pair { command } => handle_pair(command, &client, &cli.api, cli.json).await,
        Command::Topology { command } => {
            handle_topology(command, &client, &cli.api, cli.json).await
        }
        Command::Connect { device_id } => {
            let response: hyprbridge_protocol::ConnectResponse = post_json(
                &client,
                &cli.api,
                "/v1/connect",
                &ConnectRequest {
                    device_id: DeviceId(device_id),
                    edge: None,
                },
            )
            .await?;
            print_value(&response, cli.json);
            Ok(())
        }
        Command::ReturnLocal => {
            let response: ReturnLocalResponse =
                post_empty(&client, &cli.api, "/v1/ownership/return").await?;
            print_value(&response, cli.json);
            Ok(())
        }
        Command::Clipboard { command } => {
            handle_clipboard(command, &client, &cli.api, cli.json).await
        }
        Command::Transfer { command } => {
            handle_transfer(command, &client, &cli.api, cli.json).await
        }
        Command::Relay { command } => handle_relay(command, &client, &cli.api, cli.json).await,
        Command::Group { command } => handle_group(command, &client, &cli.api, cli.json).await,
        Command::Plugin { command } => handle_plugin(command, &client, &cli.api, cli.json).await,
        Command::Screen { command } => handle_screen(command, &client, &cli.api, cli.json).await,
        Command::Record { command } => handle_record(command, &client, &cli.api, cli.json).await,
        Command::Sync { command } => handle_sync(command, &client, &cli.api, cli.json).await,
        Command::Peripheral { command } => handle_peripheral(command, cli.json).await,
        Command::Hidpp { command } => handle_hidpp(command, cli.json).await,
        Command::Workspace { command } => {
            handle_workspace(command, &client, &cli.api, cli.json).await
        }
        Command::Remap { command } => handle_remap(command),
    }
}

/// Handle local peripheral commands. Unlike other commands these do not need
/// the daemon: discovery and config validation are local and unprivileged.
async fn handle_peripheral(command: PeripheralCommand, json: bool) -> Result<()> {
    use hyprbridge_peripherals as peripherals;

    match command {
        PeripheralCommand::List => {
            let devices = peripherals::discovery::list_devices()?;
            if json {
                print_value(&devices, true);
            } else {
                for device in &devices {
                    println!(
                        "{}:{}  {}  [{}]",
                        device.vendor,
                        device.product,
                        device.name,
                        device.event_nodes().join(",")
                    );
                }
            }
            Ok(())
        }
        PeripheralCommand::Doctor => {
            let checks = peripherals::doctor::checks(None);
            if json {
                print_value(&checks, true);
            } else {
                for check in &checks {
                    let mark = match check.ok {
                        Some(true) => "ok",
                        Some(false) => "fail",
                        None => "--",
                    };
                    println!("[{mark}] {:<24} {}", check.id, check.detail);
                }
            }
            Ok(())
        }
        PeripheralCommand::Check => {
            let path = peripherals::config::default_config_path();
            let config = peripherals::config::load(&path)
                .with_context(|| format!("config path: {}", path.display()))?;
            if json {
                print_value(&config, true);
            } else {
                println!(
                    "config OK: {} device(s) at {}",
                    config.devices.len(),
                    path.display()
                );
                for device in &config.devices {
                    println!(
                        "  {}: backend={}, profiles={:?}",
                        device.name,
                        device.backend,
                        device
                            .profiles
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>()
                    );
                }
            }
            Ok(())
        }
        PeripheralCommand::Plan {
            device,
            window_class,
        } => {
            let path = peripherals::config::default_config_path();
            let config = peripherals::config::load(&path)
                .with_context(|| format!("config path: {}", path.display()))?;
            let found = config
                .devices
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(&device))
                .ok_or_else(|| anyhow::anyhow!("no configured device named {device:?}"))?;
            let profile = peripherals::engine::resolve_profile(found, window_class.as_deref());
            if json {
                let plan = serde_json::json!({
                    "device": found.name,
                    "backend": found.backend,
                    "profile": profile.name,
                    "window_class": profile.window_class,
                    "dpi": profile.dpi,
                    "active_dpi": profile.active_dpi,
                    "report_rate": profile.report_rate,
                    "bindings": profile.bindings.iter().map(|binding| serde_json::json!({
                        "event": binding.event,
                        "action": peripherals::engine::describe(&binding.action),
                    })).collect::<Vec<_>>(),
                });
                print_value(&plan, true);
            } else {
                println!("device : {} (backend={})", found.name, found.backend);
                println!("profile: {}", profile.name);
                println!("dpi    : {:?} active={}", profile.dpi, profile.active_dpi);
                println!("rate   : {:?}", profile.report_rate);
                for binding in &profile.bindings {
                    println!(
                        "  {:<16} -> {}",
                        binding.event,
                        peripherals::engine::describe(&binding.action)
                    );
                }
            }
            Ok(())
        }
    }
}

async fn handle_pair(command: PairCommand, client: &Client, api: &str, json: bool) -> Result<()> {
    match command {
        PairCommand::Start { ttl } => {
            let response: hyprbridge_protocol::PairStartResponse = post_json(
                client,
                api,
                "/v1/pair/start",
                &PairStartRequest { ttl_seconds: ttl },
            )
            .await?;
            print_value(&response, json);
        }
        PairCommand::Request {
            code,
            name,
            device_id,
            role,
            pairing_id,
        } => {
            let id = DeviceId(device_id.unwrap_or_else(|| DeviceId::random().0));
            let identity = DeviceIdentity {
                public_key: format!("demo-{}", id.0),
                device_id: id,
                name,
                role: role.into(),
            };
            let response: hyprbridge_protocol::PairStartResponse = post_json(
                client,
                api,
                "/v1/pair/request",
                &PairRequest {
                    device: identity,
                    code,
                    pairing_id: PairingId(pairing_id),
                },
            )
            .await?;
            print_value(&response, json);
        }
        PairCommand::Approve { device_id } => {
            let response: hyprbridge_protocol::PairApproveResponse = post_json(
                client,
                api,
                "/v1/pair/approve",
                &PairApproveRequest {
                    device_id: DeviceId(device_id),
                },
            )
            .await?;
            print_value(&response, json);
        }
    }

    Ok(())
}

async fn handle_topology(
    command: TopologyCommand,
    client: &Client,
    api: &str,
    json: bool,
) -> Result<()> {
    match command {
        TopologyCommand::Show => {
            let topology: TopologySnapshot = get_json(client, api, "/v1/topology").await?;
            print_value(&topology, json);
        }
        TopologyCommand::Sample { device_id } => {
            let topology = TopologySnapshot {
                edges: vec![hyprbridge_protocol::EdgeMapping {
                    local_edge: ScreenEdge::Right,
                    remote_device: DeviceId(device_id),
                    remote_edge: ScreenEdge::Left,
                }],
                dead_zone_px: 12,
            };
            print_value(&topology, true);
        }
        TopologyCommand::Set {
            edge,
            device_id,
            remote_edge,
            dead_zone,
        } => {
            let mut topology: TopologySnapshot = get_json(client, api, "/v1/topology").await?;
            topology
                .edges
                .retain(|mapping| mapping.local_edge != edge.into());
            topology.edges.push(hyprbridge_protocol::EdgeMapping {
                local_edge: edge.into(),
                remote_device: DeviceId(device_id),
                remote_edge: remote_edge.into(),
            });
            topology.dead_zone_px = dead_zone;
            let response: hyprbridge_protocol::TopologyUpdateResponse = put_json(
                client,
                api,
                "/v1/topology",
                &TopologyUpdateRequest { topology },
            )
            .await?;
            print_value(&response, json);
        }
    }

    Ok(())
}

async fn handle_clipboard(
    command: ClipboardCommand,
    client: &Client,
    api: &str,
    json: bool,
) -> Result<()> {
    match command {
        ClipboardCommand::Status => {
            let status: serde_json::Value = get_json(client, api, "/v1/clipboard").await?;
            print_value(&status, json);
        }
        ClipboardCommand::Push => {
            let status: serde_json::Value = post_empty(client, api, "/v1/clipboard/push").await?;
            print_value(&status, json);
        }
        ClipboardCommand::History => {
            let history: serde_json::Value = get_json(client, api, "/v1/clipboard/history").await?;
            print_value(&history, json);
        }
        ClipboardCommand::Pin { id } => {
            let _: serde_json::Value = post_json(
                client,
                api,
                "/v1/clipboard/history/pin",
                &serde_json::json!({ "id": id }),
            )
            .await?;
            println!("Pinned entry {id}");
        }
        ClipboardCommand::Unpin { id } => {
            let _: serde_json::Value = post_json(
                client,
                api,
                "/v1/clipboard/history/unpin",
                &serde_json::json!({ "id": id }),
            )
            .await?;
            println!("Unpinned entry {id}");
        }
        ClipboardCommand::Delete { id } => {
            let _: serde_json::Value =
                post_empty(client, api, &format!("/v1/clipboard/history/{id}")).await?;
            println!("Deleted entry {id}");
        }
        ClipboardCommand::Clear => {
            let _: serde_json::Value =
                post_empty(client, api, "/v1/clipboard/history/clear").await?;
            println!("History cleared");
        }
    }

    Ok(())
}

async fn handle_transfer(
    command: TransferCommand,
    client: &Client,
    api: &str,
    json: bool,
) -> Result<()> {
    match command {
        TransferCommand::Send { file, device } => {
            let request = serde_json::json!({
                "device_id": device,
                "file_path": file,
            });
            let response: serde_json::Value =
                post_json(client, api, "/v1/transfer/send", &request).await?;
            print_value(&response, json);
        }
        TransferCommand::SendFolder {
            folder,
            device,
            preserve_structure,
        } => {
            let request = serde_json::json!({
                "path": folder,
                "device_id": device,
                "preserve_structure": preserve_structure,
            });
            let response: serde_json::Value =
                post_json(client, api, "/v1/transfer/send-folder", &request).await?;
            print_value(&response, json);
        }
        TransferCommand::SendBatch { files, device } => {
            let request = serde_json::json!({
                "files": files,
                "device_id": device,
            });
            let response: serde_json::Value =
                post_json(client, api, "/v1/transfer/send-batch", &request).await?;
            print_value(&response, json);
        }
        TransferCommand::Status => {
            let status: serde_json::Value = get_json(client, api, "/v1/transfer/status").await?;
            print_value(&status, json);
        }
        TransferCommand::BatchStatus { batch_id } => {
            let response: serde_json::Value =
                get_json(client, api, &format!("/v1/transfer/batch/{}", batch_id)).await?;
            print_value(&response, json);
        }
    }
    Ok(())
}

async fn handle_relay(command: RelayCommand, client: &Client, api: &str, json: bool) -> Result<()> {
    match command {
        RelayCommand::Status => {
            let status: serde_json::Value = get_json(client, api, "/v1/relay/status").await?;
            print_value(&status, json);
        }
        RelayCommand::Connect { url } => {
            let request = serde_json::json!({ "url": url });
            let response: serde_json::Value =
                post_json(client, api, "/v1/relay/connect", &request).await?;
            print_value(&response, json);
        }
        RelayCommand::Disconnect => {
            let response: serde_json::Value =
                post_empty(client, api, "/v1/relay/disconnect").await?;
            print_value(&response, json);
        }
    }
    Ok(())
}

async fn handle_group(command: GroupCommand, client: &Client, api: &str, json: bool) -> Result<()> {
    match command {
        GroupCommand::Create { name } => {
            let request = hyprbridge_protocol::CreateGroupRequest { name };
            let response: hyprbridge_protocol::CreateGroupResponse =
                post_json(client, api, "/v1/groups", &request).await?;
            print_value(&response, json);
        }
        GroupCommand::Join { group_id } => {
            let response: serde_json::Value =
                post_empty(client, api, &format!("/v1/groups/{group_id}/join")).await?;
            print_value(&response, json);
        }
        GroupCommand::Leave { group_id } => {
            let response: serde_json::Value =
                post_empty(client, api, &format!("/v1/groups/{group_id}/leave")).await?;
            print_value(&response, json);
        }
        GroupCommand::List => {
            let response: serde_json::Value = get_json(client, api, "/v1/groups").await?;
            print_value(&response, json);
        }
        GroupCommand::Members { group_id } => {
            let response: serde_json::Value =
                get_json(client, api, &format!("/v1/groups/{group_id}/members")).await?;
            print_value(&response, json);
        }
        GroupCommand::Add {
            group_id,
            device_id,
        } => {
            let request = hyprbridge_protocol::AddToGroupRequest {
                group_id: group_id.clone(),
                device_id: DeviceId(device_id),
            };
            let response: serde_json::Value =
                post_json(client, api, &format!("/v1/groups/{group_id}/add"), &request).await?;
            print_value(&response, json);
        }
        GroupCommand::Remove {
            group_id,
            device_id,
        } => {
            let request = hyprbridge_protocol::RemoveFromGroupRequest {
                group_id: group_id.clone(),
                device_id: DeviceId(device_id),
            };
            let response: serde_json::Value = post_json(
                client,
                api,
                &format!("/v1/groups/{group_id}/remove"),
                &request,
            )
            .await?;
            print_value(&response, json);
        }
    }
    Ok(())
}

async fn handle_plugin(
    command: PluginCommand,
    client: &Client,
    api: &str,
    json: bool,
) -> Result<()> {
    match command {
        PluginCommand::List => {
            let response: serde_json::Value = get_json(client, api, "/v1/plugins").await?;
            print_value(&response, json);
        }
        PluginCommand::Search { query } => {
            let response: serde_json::Value =
                get_json(client, api, &format!("/v1/plugins/search?q={query}")).await?;
            print_value(&response, json);
        }
        PluginCommand::Install { source } => {
            let request = serde_json::json!({ "source": source });
            let response: serde_json::Value =
                post_json(client, api, "/v1/plugins/install", &request).await?;
            print_value(&response, json);
        }
        PluginCommand::Uninstall { plugin_id } => {
            let response: serde_json::Value =
                post_empty(client, api, &format!("/v1/plugins/{plugin_id}/uninstall")).await?;
            print_value(&response, json);
        }
        PluginCommand::Enable { plugin_id } => {
            let response: serde_json::Value =
                post_empty(client, api, &format!("/v1/plugins/{plugin_id}/enable")).await?;
            print_value(&response, json);
        }
        PluginCommand::Disable { plugin_id } => {
            let response: serde_json::Value =
                post_empty(client, api, &format!("/v1/plugins/{plugin_id}/disable")).await?;
            print_value(&response, json);
        }
        PluginCommand::Info { plugin_id } => {
            let response: serde_json::Value =
                get_json(client, api, &format!("/v1/plugins/{plugin_id}")).await?;
            print_value(&response, json);
        }
    }
    Ok(())
}

async fn handle_screen(
    command: ScreenCommand,
    client: &Client,
    api: &str,
    json: bool,
) -> Result<()> {
    match command {
        ScreenCommand::Share {
            device,
            fps,
            quality,
            audio,
        } => {
            let request = serde_json::json!({
                "device_id": device,
                "fps": fps,
                "quality": quality,
                "audio": audio,
            });
            let response: serde_json::Value =
                post_json(client, api, "/v1/screen/share", &request).await?;
            print_value(&response, json);
        }
        ScreenCommand::View { device } => {
            let request = serde_json::json!({ "device_id": device });
            let response: serde_json::Value =
                post_json(client, api, "/v1/screen/view", &request).await?;
            print_value(&response, json);
        }
        ScreenCommand::Control { device } => {
            let request = serde_json::json!({ "device_id": device });
            let response: serde_json::Value =
                post_json(client, api, "/v1/screen/control", &request).await?;
            print_value(&response, json);
        }
        ScreenCommand::Stop => {
            let response: serde_json::Value = post_empty(client, api, "/v1/screen/stop").await?;
            print_value(&response, json);
        }
        ScreenCommand::List => {
            let response: serde_json::Value = get_json(client, api, "/v1/screen/sessions").await?;
            print_value(&response, json);
        }
    }
    Ok(())
}

async fn handle_record(
    command: RecordCommand,
    client: &Client,
    api: &str,
    json: bool,
) -> Result<()> {
    match command {
        RecordCommand::Start { name } => {
            let request = serde_json::json!({ "name": name });
            let response: serde_json::Value =
                post_json(client, api, "/v1/record/start", &request).await?;
            print_value(&response, json);
        }
        RecordCommand::Stop => {
            let response: serde_json::Value = post_empty(client, api, "/v1/record/stop").await?;
            print_value(&response, json);
        }
        RecordCommand::List => {
            let response: serde_json::Value = get_json(client, api, "/v1/record/list").await?;
            print_value(&response, json);
        }
        RecordCommand::Play { file, speed } => {
            let request = serde_json::json!({ "file": file, "speed": speed });
            let response: serde_json::Value =
                post_json(client, api, "/v1/record/play", &request).await?;
            print_value(&response, json);
        }
        RecordCommand::Info { file } => {
            let response: serde_json::Value =
                get_json(client, api, &format!("/v1/record/info/{file}")).await?;
            print_value(&response, json);
        }
        RecordCommand::Delete { file } => {
            let response: serde_json::Value =
                post_empty(client, api, &format!("/v1/record/delete/{file}")).await?;
            print_value(&response, json);
        }
    }
    Ok(())
}

async fn handle_sync(command: SyncCommand, client: &Client, api: &str, json: bool) -> Result<()> {
    match command {
        SyncCommand::Status => {
            let status: serde_json::Value = get_json(client, api, "/v1/sync/status").await?;
            print_value(&status, json);
        }
        SyncCommand::Push => {
            let response: serde_json::Value = post_empty(client, api, "/v1/sync/push").await?;
            print_value(&response, json);
        }
        SyncCommand::Pull { device } => {
            let request = serde_json::json!({ "device": device });
            let response: serde_json::Value =
                post_json(client, api, "/v1/sync/pull", &request).await?;
            print_value(&response, json);
        }
        SyncCommand::Configure => {
            let response: serde_json::Value = post_empty(client, api, "/v1/sync/configure").await?;
            print_value(&response, json);
        }
        SyncCommand::Devices => {
            let response: serde_json::Value = get_json(client, api, "/v1/sync/devices").await?;
            print_value(&response, json);
        }
        SyncCommand::History => {
            let response: serde_json::Value = get_json(client, api, "/v1/sync/history").await?;
            print_value(&response, json);
        }
    }
    Ok(())
}

async fn get_json<T>(client: &Client, api: &str, path: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    client
        .get(format!("{api}{path}"))
        .send()
        .await
        .context("failed to reach the HyprBridge daemon")?
        .error_for_status()
        .context("HyprBridge daemon returned an error")?
        .json::<T>()
        .await
        .context("failed to parse daemon response")
}

async fn post_json<T, B>(client: &Client, api: &str, path: &str, body: &B) -> Result<T>
where
    T: serde::de::DeserializeOwned,
    B: Serialize + ?Sized,
{
    client
        .post(format!("{api}{path}"))
        .json(body)
        .send()
        .await
        .context("failed to reach the HyprBridge daemon")?
        .error_for_status()
        .context("HyprBridge daemon returned an error")?
        .json::<T>()
        .await
        .context("failed to parse daemon response")
}

async fn put_json<T, B>(client: &Client, api: &str, path: &str, body: &B) -> Result<T>
where
    T: serde::de::DeserializeOwned,
    B: Serialize + ?Sized,
{
    client
        .put(format!("{api}{path}"))
        .json(body)
        .send()
        .await
        .context("failed to reach the HyprBridge daemon")?
        .error_for_status()
        .context("HyprBridge daemon returned an error")?
        .json::<T>()
        .await
        .context("failed to parse daemon response")
}

async fn post_empty<T>(client: &Client, api: &str, path: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    client
        .post(format!("{api}{path}"))
        .send()
        .await
        .context("failed to reach the HyprBridge daemon")?
        .error_for_status()
        .context("HyprBridge daemon returned an error")?
        .json::<T>()
        .await
        .context("failed to parse daemon response")
}

/// Find the receiver hidraw node that actually accepts HID++ long reports.
/// A Lightspeed receiver exposes several hidraw interfaces; only one of them
/// answers feature calls. Probing is more portable than hard-coding an index.
fn open_hidpp_connection() -> Result<(
    std::path::PathBuf,
    hyprbridge_peripherals::hidpp::HidppConnection<hyprbridge_peripherals::hidpp::HidrawTransport>,
)> {
    use hyprbridge_peripherals::hidpp::{HidppConnection, HidrawTransport};

    let nodes = hyprbridge_peripherals::discovery::receiver_hidraw_nodes();
    let mut last_error: Option<String> = None;
    for node in nodes {
        let Ok(transport) = HidrawTransport::open(&node) else {
            last_error = Some(format!("{}: cannot open", node.display()));
            continue;
        };
        let mut connection = HidppConnection::new(transport, 1);
        match connection.request(0, 0x00, &[0x00, 0x01]) {
            Ok(reply) if !reply.is_empty() => return Ok((node, connection)),
            Ok(_) => last_error = Some(format!("{}: empty reply", node.display())),
            Err(error) => last_error = Some(format!("{}: {error}", node.display())),
        }
    }
    Err(anyhow::anyhow!(
        "no receiver hidraw node answered a HID++ probe ({}); install the udev rule and replug",
        last_error.unwrap_or_else(|| "no nodes found".into())
    ))
}

/// Handle local remapping commands (no daemon needed).
fn handle_remap(command: RemapCommand) -> Result<()> {
    match command {
        RemapCommand::Start {
            device,
            config,
            dry_run,
        } => {
            let mut remapper = hyprbridge_remap::Remapper::new(&device, config.as_deref())
                .context("remap start failed")?;
            if dry_run {
                println!("{}", remapper.summary());
                return Ok(());
            }
            remapper.run().context("remap stopped")?;
            Ok(())
        }
        RemapCommand::Events { device } => {
            hyprbridge_remap::watch_events(&device).context("remap events failed")
        }
    }
}

/// Handle workspace commands through the daemon API.
async fn handle_workspace(
    command: WorkspaceCommand,
    client: &Client,
    api: &str,
    json: bool,
) -> Result<()> {
    match command {
        WorkspaceCommand::Info => {
            let value: serde_json::Value = get_json(client, api, "/v1/workspace/info").await?;
            if json {
                print_value(&value, true);
            } else {
                let inspection = &value["inspection"];
                let platform = &inspection["platform"];
                let desktop = &value["desktop"];
                let text =
                    |value: &serde_json::Value| value.as_str().unwrap_or("unknown").to_string();
                println!("os        : {}", text(&platform["os"]));
                println!("session   : {}", text(&platform["linux"]["session_type"]));
                println!(
                    "desktop   : {}",
                    text(&platform["linux"]["desktop_environment"])
                );
                println!(
                    "backend   : {} ({})",
                    text(&desktop["backend"]),
                    text(&desktop["state"])
                );
                println!("summary   : {}", text(&desktop["summary"]));
                if let Some(notes) = desktop["notes"].as_array() {
                    for note in notes {
                        println!("  - {}", text(note));
                    }
                }
            }
            Ok(())
        }
        WorkspaceCommand::Files { path } => {
            let request = client.get(format!("{api}/v1/workspace/files"));
            let request = match path {
                Some(path) => request.query(&[("path", path)]),
                None => request,
            };
            let value: serde_json::Value = request
                .send()
                .await
                .context("failed to reach the HyprBridge daemon")?
                .error_for_status()?
                .json()
                .await?;
            print_value(&value, json);
            Ok(())
        }
        WorkspaceCommand::File { path } => {
            let value: serde_json::Value = client
                .get(format!("{api}/v1/workspace/file"))
                .query(&[("path", path)])
                .send()
                .await
                .context("failed to reach the HyprBridge daemon")?
                .error_for_status()?
                .json()
                .await?;
            if json {
                print_value(&value, true);
            } else {
                println!("{}", value["content"].as_str().unwrap_or(""));
            }
            Ok(())
        }
    }
}

/// Handle direct HID++ commands against a Logitech receiver.
async fn handle_hidpp(command: HidppCommand, json: bool) -> Result<()> {
    let (node, mut connection) = open_hidpp_connection()?;

    match command {
        HidppCommand::Info => {
            let dpi = connection.read_dpi()?;
            let rate = connection.read_report_rate()?;
            if json {
                print_value(
                    &serde_json::json!({
                        "node": node.display().to_string(),
                        "dpi": dpi,
                        "report_rate": rate,
                    }),
                    true,
                );
            } else {
                println!("node: {}", node.display());
                print_hidpp_dpi(&dpi);
                print_hidpp_rate(&rate);
            }
            Ok(())
        }
        HidppCommand::Dpi { set } => {
            if let Some(dpi) = set {
                connection.set_dpi(dpi)?;
            }
            let status = connection.read_dpi()?;
            if json {
                print_value(&status, true);
            } else {
                print_hidpp_dpi(&status);
            }
            Ok(())
        }
        HidppCommand::Rate { set } => {
            if let Some(hz) = set {
                connection.set_report_rate_hz(hz)?;
            }
            let status = connection.read_report_rate()?;
            if json {
                print_value(&status, true);
            } else {
                print_hidpp_rate(&status);
            }
            Ok(())
        }
    }
}

fn print_hidpp_dpi(status: &Option<hyprbridge_peripherals::hidpp::DpiStatus>) {
    match status {
        Some(status) => println!(
            "dpi    : {} active (default {}, feature 0x{:02X})",
            status.active, status.default, status.feature_index
        ),
        None => println!("dpi    : feature not present on this device"),
    }
}

fn print_hidpp_rate(status: &Option<hyprbridge_peripherals::hidpp::ReportRateStatus>) {
    match status {
        Some(status) => println!(
            "rate   : {} Hz ({} ms, supported mask 0x{:02X}, feature 0x{:02X})",
            status.hz, status.period_ms, status.supported_periods, status.feature_index
        ),
        None => println!("rate   : feature not present on this device"),
    }
}

fn print_value<T>(value: &T, json: bool)
where
    T: Serialize,
{
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).expect("serializing CLI output should succeed")
        );
        return;
    }

    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("serializing CLI output should succeed")
    );
}

fn print_status(status: &ServiceStatus) {
    println!(
        "Device: {}\nRole: {}\nOwnership: {}\nTrusted peers: {}\nDiscovered peers: {}",
        status.identity.name,
        label_role(status.identity.role),
        label_ownership(status.ownership.state),
        status.trusted_peers,
        status.discovered_peers
    );
    println!(
        "Session: {}\nCapture: {}\nInjection: {}\nCursor handoff: {}",
        label_linux_session(status.capability.linux_session),
        label_support(status.capability.input_capture),
        label_support(status.capability.input_injection),
        label_support(status.capability.cursor_handoff)
    );

    if let Some(backend) = status.input_backend.as_ref() {
        println!(
            "Backend: {}\nEdge candidate: {}\nWheel injection: {}",
            backend.backend,
            backend
                .edge_candidate
                .map(label_edge)
                .unwrap_or_else(|| "none".into()),
            label_support(backend.target_wheel_injection)
        );
        if let Some(pointer) = backend.pointer.as_ref() {
            println!(
                "Pointer: {}, {} / {}x{}",
                pointer.x, pointer.y, pointer.width, pointer.height
            );
        }
        if let Some(capture) = backend.controller_capture.as_ref() {
            println!(
                "Controller capture: readable_evdev_devices={}, grabbed_devices={}, exclusive_capture_active={}",
                capture.readable_devices, capture.grabbed_devices, capture.exclusive_capture_active
            );
        }
        if let Some(injection) = backend.target_injection.as_ref() {
            println!(
                "Target injection: primary_backend={}, fallback_backend={}, uinput_writable={}, last_error={}",
                injection.primary_backend,
                injection.fallback_backend.as_deref().unwrap_or("none"),
                injection.uinput_writable,
                injection.last_error.as_deref().unwrap_or("none")
            );
        }
        println!("Detail: {}", backend.detail);
    }
}

fn print_doctor(report: &DoctorReport) {
    for check in &report.checks {
        print_doctor_check(check);
    }
}

fn print_doctor_check(check: &DoctorCheck) {
    println!(
        "[{}] {}\n{}\n",
        match check.status {
            CheckStatus::Ok => "ok",
            CheckStatus::Warning => "warning",
            CheckStatus::Error => "error",
        },
        check.title,
        check.detail
    );
}

fn label_role(role: DeviceRole) -> &'static str {
    match role {
        DeviceRole::Controller => "controller",
        DeviceRole::Target => "target",
    }
}

fn label_linux_session(session: hyprbridge_protocol::LinuxSessionKind) -> &'static str {
    match session {
        hyprbridge_protocol::LinuxSessionKind::X11 => "x11",
        hyprbridge_protocol::LinuxSessionKind::Wayland => "wayland",
        hyprbridge_protocol::LinuxSessionKind::Unknown => "unknown",
    }
}

fn label_support(level: hyprbridge_protocol::SupportLevel) -> &'static str {
    match level {
        hyprbridge_protocol::SupportLevel::Supported => "supported",
        hyprbridge_protocol::SupportLevel::Unsupported => "unsupported",
        hyprbridge_protocol::SupportLevel::Planned => "planned",
    }
}

fn label_ownership(state: hyprbridge_protocol::OwnershipState) -> &'static str {
    match state {
        hyprbridge_protocol::OwnershipState::LocalActive => "local_active",
        hyprbridge_protocol::OwnershipState::CrossingEdge => "crossing_edge",
        hyprbridge_protocol::OwnershipState::RemoteActive => "remote_active",
        hyprbridge_protocol::OwnershipState::ReturningLocal => "returning_local",
        hyprbridge_protocol::OwnershipState::Suspended => "suspended",
    }
}

fn label_edge(edge: ScreenEdge) -> String {
    match edge {
        ScreenEdge::Left => "left",
        ScreenEdge::Right => "right",
        ScreenEdge::Top => "top",
        ScreenEdge::Bottom => "bottom",
    }
    .into()
}
