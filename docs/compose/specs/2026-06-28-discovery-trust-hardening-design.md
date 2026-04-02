# Discovery & Trust Hardening

## [S1] Problem

The discovery, trust, and pairing subsystem has several reliability weaknesses:
- Ownership FSM can get stuck in `CrossingEdge` state with no timeout
- Daemon has a double `ReturnLocal` call bug and hardcoded `ScreenEdge::Right`
- mDNS discovery fails silently with no retry
- Pairing is single-slot with no concurrency or cancellation
- CORS is wide open (`allow_origin(Any)`)
- TrustStore uses linear scan, no eviction, no revocation
- Daemon crate has zero tests
- `DeviceId` has no format validation

## [S2] Solution Overview

Three-layer incremental hardening:

1. **Critical Bug Fixes** — Ownership FSM timeout, double ReturnLocal, hardcoded edge
2. **Resilience** — mDNS retry, pairing concurrency, CORS hardening
3. **Infrastructure** — TrustStore refactor, daemon tests, DeviceId validation

## [S3] Layer 1: Critical Bug Fixes

### [S3.1] Ownership FSM Timeout

Add `crossing_timeout: Duration` (default 5s) to `OwnershipMachine::new()`. Track `entered_crossing_at: Option<Instant>` in the state. On `tick()` or explicit timeout check, if `CrossingEdge` has been active longer than `crossing_timeout`, auto-transition to `ReturningLocal`.

**Files:** `crates/core/src/lib.rs`

### [S3.2] Double ReturnLocal Fix

In `crates/daemon/src/lib.rs`, `post_return_local` dispatches `ReturnLocal` event twice. Remove the duplicate call. Verify with a test that ownership returns to `LocalActive` after a single `ReturnLocal`.

**Files:** `crates/daemon/src/lib.rs`

### [S3.3] Hardcoded ScreenEdge

The connect handler hardcodes `ScreenEdge::Right`. Change `POST /v1/connect` to accept an optional `edge: ScreenEdge` parameter. If not provided, default to `Right` for backward compatibility. Update the edge monitor to pass the detected edge.

**Files:** `crates/daemon/src/lib.rs`, `crates/protocol/src/lib.rs`

## [S4] Layer 2: Resilience Improvements

### [S4.1] mDNS Retry/Reconnect

`MdnsRuntime` startup gets retry with exponential backoff (3 attempts, 1s/2s/4s). If all attempts fail, daemon stays alive with `mdns_healthy: false` in status. A background task retries every 30s. Expose `mdns_healthy: bool` in `ServiceStatus`.

**Files:** `crates/network/src/lib.rs`, `crates/daemon/src/lib.rs`, `crates/protocol/src/lib.rs`

### [S4.2] Pairing Concurrency

Replace `Mutex<Option<PendingPairing>>` with `RwLock<Vec<PendingPairing>>`. Each pairing gets a unique `PairingId` (UUID). Add `cancel(PairingId)` method. Pairing codes expire per-flow via existing TTL. Update REST API: `POST /v1/pair/cancel` takes `PairingId`.

**Files:** `crates/network/src/lib.rs`, `crates/daemon/src/lib.rs`, `crates/protocol/src/lib.rs`

### [S4.3] CORS Hardening

Replace `allow_origin(Any)` with configurable allowed origins. Read from `HyprBridgeConfig` (new `cors_origins: Vec<String>` field). Default to `["http://localhost:*", "http://127.0.0.1:*"]`. Also support `HYPRBRIDGE_CORS_ORIGINS` env var.

**Files:** `crates/daemon/src/lib.rs`, `crates/config/src/lib.rs`

## [S5] Layer 3: Infrastructure Hardening

### [S5.1] TrustStore Refactor

Replace `Vec<TrustRecord>` with `HashMap<DeviceId, TrustRecord>`. Add:
- `remove(device_id) -> bool` for revocation
- `last_seen: Option<Instant>` to `TrustRecord`
- `max_trusted_peers: usize` (configurable, default 32) with LRU eviction on `trust()` when limit exceeded
- `list()` returns sorted by `trusted_at` for stable output

**Files:** `crates/network/src/lib.rs`

### [S5.2] Daemon Test Suite

Add integration tests covering:
- Ownership FSM lifecycle (all transitions including timeout)
- Pairing flow (start → request → approve, expiry, concurrent flows, cancellation)
- Trust store CRUD and eviction
- mDNS registration and discovery index updates
- REST API endpoint responses (status, discovery, transport, pair, connect, return)

**Files:** `crates/daemon/tests/` (new test files)

### [S5.3] DeviceId Validation

Add `DeviceId::parse(s: &str) -> Result<Self>` that validates UUID v4 format. Reject invalid IDs at protocol boundaries: pairing requests, discovery updates, trust operations. Keep existing `DeviceId` as-is for internal use; add `ValidDeviceId` newtype or validate on construction.

**Files:** `crates/protocol/src/lib.rs`

## [S6] Testing Strategy

- Unit tests for each changed component
- Integration tests for daemon flows
- Manual test: `cargo test --workspace` must pass
- Manual test: `cargo run -p hyprbridge-cli -- doctor` must pass

## [S7] Non-Goals

- Signature verification on DeviceIdentity (separate future work)
- Transport-level backpressure (separate future work)
- TrustStore encryption at rest (separate future work)
