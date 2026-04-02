# Relay Mode Design

## [S1] Problem

HyprBridge currently only works on LAN. Devices on different networks (e.g., home and office, different cities) cannot connect. This limits the tool's usefulness for remote work scenarios.

## [S2] Solution Overview

WebSocket-based relay server + client support:
- Relay server: WebSocket long connection, forwards control frames and data streams
- Client: Connects to server, registers device, gets token, joins room
- Token-based authentication: Device registers, gets token, uses token for subsequent connections
- Auto-detection: If devices are on LAN, use QUIC direct; if not, use relay
- CLI and REST API for relay management

## [S3] Relay Server

New crate `crates/relay-server`:

**Architecture:**
- WebSocket interface (axum + tokio-tungstenite)
- Device registration/authentication
- Room management
- Message routing
- Connection tracking

**Core flow:**
1. Device A connects, sends `Register { device_id, name, role }`
2. Server validates identity, returns `RegisterAck { token, room_id }`
3. Device B connects with token to same room
4. Server forwards messages: A → server → B

**API:**
- `GET /ws` → WebSocket upgrade
- `POST /api/register` → register device, get token
- `GET /api/rooms` → list rooms
- `GET /api/rooms/{id}` → room details

**Files:** `crates/relay-server/Cargo.toml`, `crates/relay-server/src/main.rs`, `crates/relay-server/src/lib.rs`

## [S4] Client Relay Support

In `crates/network`, add relay client:

**RelayClient struct:**
```rust
pub struct RelayClient {
    server_url: String,
    token: Option<String>,
    room_id: Option<String>,
    ws_connection: Option<WsConnection>,
    local_device: DeviceIdentity,
}
```

**Connection flow:**
1. `connect(server_url)` → connect to relay server
2. `register()` → send device info, get token
3. `join_room(room_id)` → join room
4. `send(frame)` → forward control frame
5. `receive()` → receive remote control frame

**Integration with QUIC:**
- Detect if devices are on same LAN
- If LAN → use QUIC direct
- If not → use relay server
- Auto-switch logic

**Files:** `crates/network/src/relay_client.rs`

## [S5] Daemon Integration

- Add `relay_client: Option<RelayClient>` to `DaemonState`
- Config: `[relay]` section with `enabled` and `server_url`
- Control frame routing: prefer QUIC, fallback to relay
- CLI: `relay status|connect|disconnect|rooms`
- REST: `GET /v1/relay/status`, `POST /v1/relay/connect`, `POST /v1/relay/disconnect`

**Files:** `crates/daemon/src/lib.rs`, `crates/config/src/lib.rs`, `crates/cli/src/main.rs`

## [S6] Testing Strategy

- Unit tests for relay server message routing
- Integration test: two clients connecting through relay
- Manual test: connect two daemons via relay server
- Load test: multiple concurrent connections

## [S7] Non-Goals

- End-to-end encryption (relay sees plaintext, use TLS in transit)
- Relay server clustering (single server for v1)
- Relay server authentication beyond token (no OAuth/SSO)
- Automatic relay server discovery (manual URL configuration)
