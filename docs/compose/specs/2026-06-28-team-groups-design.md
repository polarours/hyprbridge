# Team Device Groups Design

## [S1] Problem

HyprBridge has no device grouping. Users with multiple devices must manually manage trust for each device. This scales poorly for teams and multi-device setups.

## [S2] Solution Overview

Relay server-based device grouping with auto-trust:
- Groups stored on relay server, synced across devices
- Group members auto-trusted (no manual pairing needed)
- CLI + REST API + Web UI for group management
- Create/delete groups, add/remove members

## [S3] Relay Server Group Extension

Extend relay server with group management:

**Data model:**
```rust
struct Group {
    id: String,
    name: String,
    owner: DeviceId,
    members: Vec<DeviceId>,
    created_at: DateTime<Utc>,
}
```

**New message types:**
- `CreateGroup { name }` → `CreateGroupAck { group_id }`
- `JoinGroup { group_id, token }` → `JoinGroupAck { group_id, members }`
- `LeaveGroup { group_id }`
- `ListGroups` → `ListGroupsAck { groups }`
- `AddToGroup { group_id, device_id }`
- `RemoveFromGroup { group_id, device_id }`

**Files:** `crates/relay-server/src/lib.rs`

## [S4] Auto-Trust Mechanism

When device joins group, auto-add to local TrustStore:
1. Listen for `JoinGroupAck` message
2. Check each member against TrustStore
3. Auto-trust unknown members with note "group auto-trust"

**Files:** `crates/network/src/relay_client.rs`

## [S5] Client Extension

Extend `RelayClient` with group management:
- `create_group(name)` → `join_group(id)` → `leave_group(id)`
- `list_groups()` → `members(group_id)`
- `add_to_group(group_id, device_id)` → `remove_from_group(group_id, device_id)`
- Group event listener → trigger auto-trust

**Files:** `crates/network/src/relay_client.rs`

## [S6] CLI Commands

```bash
hyprbridge-cli group create <name>
hyprbridge-cli group join <id>
hyprbridge-cli group leave <id>
hyprbridge-cli group list
hyprbridge-cli group members <id>
hyprbridge-cli group add <id> <device>
hyprbridge-cli group remove <id> <device>
```

**Files:** `crates/cli/src/main.rs`

## [S7] REST API

```bash
POST   /v1/groups
GET    /v1/groups
POST   /v1/groups/{id}/join
POST   /v1/groups/{id}/leave
GET    /v1/groups/{id}/members
POST   /v1/groups/{id}/add
POST   /v1/groups/{id}/remove
```

**Files:** `crates/daemon/src/lib.rs`

## [S8] Web UI

New "Groups" panel in management console:
- Group list with member counts
- Create/delete group buttons
- Add/remove member interface
- Auto-trust indicator

**Files:** `packages/ui/src/groups-panel.tsx`

## [S9] Testing Strategy

- Unit tests for group management logic
- Integration test: create group, add members, verify auto-trust
- Manual test: CLI group operations
- Load test: multiple concurrent group operations

## [S10] Non-Goals

- Group permissions (admin/member roles)
- Group-to-group relationships
- Group search/discovery
- Group encryption
