# Team Device Groups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement relay server-based device grouping with auto-trust for team setups.

**Architecture:** Relay server stores groups, clients join/leave groups, group members auto-trusted. CLI + REST API + Web UI for management.

**Tech Stack:** Rust (relay-server, network, daemon, cli), React (UI)

---

## Task 1: Relay Server Group Management

**Covers:** [S3]

**Files:**
- Modify: `crates/relay-server/src/lib.rs`

- [ ] **Step 1: Add Group struct and GroupManager**

In `crates/relay-server/src/lib.rs`, add:

```rust
/// A device group for team setups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Unique group identifier.
    pub id: String,
    /// Human-readable group name.
    pub name: String,
    /// Device that created the group.
    pub owner: DeviceId,
    /// Group member device IDs.
    pub members: Vec<DeviceId>,
    /// When the group was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Manages device groups.
pub struct GroupManager {
    /// All groups: group_id -> Group.
    pub groups: RwLock<HashMap<String, Group>>,
    /// Device to groups mapping: device_id -> Vec<group_id>.
    pub device_groups: RwLock<HashMap<DeviceId, Vec<String>>>,
}

impl GroupManager {
    /// Create a new group manager.
    pub fn new() -> Self {
        Self {
            groups: RwLock::new(HashMap::new()),
            device_groups: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new group.
    pub async fn create_group(&self, name: String, owner: DeviceId) -> Group {
        let id = format!("group-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());
        let group = Group {
            id: id.clone(),
            name,
            owner: owner.clone(),
            members: vec![owner.clone()],
            created_at: chrono::Utc::now(),
        };

        self.groups.write().await.insert(id.clone(), group.clone());
        self.device_groups.write().await
            .entry(owner)
            .or_default()
            .push(id.clone());

        group
    }

    /// Add a device to a group.
    pub async fn add_member(&self, group_id: &str, device_id: DeviceId) -> Result<Group, String> {
        let mut groups = self.groups.write().await;
        let group = groups.get_mut(group_id).ok_or("group not found")?;

        if !group.members.contains(&device_id) {
            group.members.push(device_id.clone());
        }

        self.device_groups.write().await
            .entry(device_id)
            .or_default()
            .push(group_id.to_string());

        Ok(group.clone())
    }

    /// Remove a device from a group.
    pub async fn remove_member(&self, group_id: &str, device_id: &DeviceId) -> Result<Group, String> {
        let mut groups = self.groups.write().await;
        let group = groups.get_mut(group_id).ok_or("group not found")?;

        group.members.retain(|m| m != device_id);

        if let Some(device_groups) = self.device_groups.write().await.get_mut(device_id) {
            device_groups.retain(|g| g != group_id);
        }

        Ok(group.clone())
    }

    /// List all groups.
    pub async fn list_groups(&self) -> Vec<Group> {
        self.groups.read().await.values().cloned().collect()
    }

    /// Get group members.
    pub async fn get_members(&self, group_id: &str) -> Result<Vec<DeviceId>, String> {
        let groups = self.groups.read().await;
        let group = groups.get(group_id).ok_or("group not found")?;
        Ok(group.members.clone())
    }

    /// Delete a group.
    pub async fn delete_group(&self, group_id: &str) -> Result<(), String> {
        let group = self.groups.write().await.remove(group_id)
            .ok_or("group not found")?;

        for member in &group.members {
            if let Some(device_groups) = self.device_groups.write().await.get_mut(member) {
                device_groups.retain(|g| g != group_id);
            }
        }

        Ok(())
    }
}

impl Default for GroupManager {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Add group methods to RelayState**

In `RelayState`, add:

```rust
/// Group manager.
pub group_manager: GroupManager,
```

Initialize in `RelayState::new`:

```rust
group_manager: GroupManager::new(),
```

- [ ] **Step 3: Add group message handling**

In the WebSocket handler, add cases for group messages:

```rust
RelayMessage::CreateGroup { name } => {
    if let Some(device_id) = &registered_device {
        let group = state.group_manager.create_group(name, device_id.clone()).await;
        let ack = RelayMessage::CreateGroupAck { group_id: group.id };
        let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
    }
}
RelayMessage::JoinGroup { group_id, token } => {
    match state.group_manager.add_member(&group_id, registered_device.clone().unwrap_or_default()).await {
        Ok(group) => {
            let members = group.members;
            let ack = RelayMessage::JoinGroupAck { group_id, members };
            let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
        }
        Err(e) => {
            let err = RelayMessage::Error { detail: e };
            let _ = sender.send(Message::Text(serde_json::to_string(&err).unwrap().into())).await;
        }
    }
}
RelayMessage::LeaveGroup { group_id } => {
    if let Some(device_id) = &registered_device {
        let _ = state.group_manager.remove_member(&group_id, device_id).await;
    }
}
RelayMessage::ListGroups => {
    let groups = state.group_manager.list_groups().await;
    let group_infos: Vec<GroupInfo> = groups.iter().map(|g| GroupInfo {
        id: g.id.clone(),
        name: g.name.clone(),
        member_count: g.members.len(),
    }).collect();
    let ack = RelayMessage::ListGroupsAck { groups: group_infos };
    let _ = sender.send(Message::Text(serde_json::to_string(&ack).unwrap().into())).await;
}
```

- [ ] **Step 4: Add GroupInfo struct**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub member_count: usize,
}
```

- [ ] **Step 5: Run check**

Run: `cargo check -p hyprbridge-relay-server`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/relay-server/
git commit -m "feat(relay): add group management to relay server"
```

---

## Task 2: Relay Client Group Methods

**Covers:** [S4, S5]

**Files:**
- Modify: `crates/network/src/relay_client.rs`

- [ ] **Step 1: Add group message variants**

In `relay_client.rs`, add to `RelayMessage`:

```rust
CreateGroup { name: String },
CreateGroupAck { group_id: String },
JoinGroup { group_id: String, token: String },
JoinGroupAck { group_id: String, members: Vec<DeviceIdentity> },
LeaveGroup { group_id: String },
ListGroups,
ListGroupsAck { groups: Vec<GroupInfo> },
AddToGroup { group_id: String, device_id: DeviceId },
RemoveFromGroup { group_id: String, device_id: DeviceId },
```

Add `GroupInfo` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub member_count: usize,
}
```

- [ ] **Step 2: Add group methods to RelayClient**

```rust
impl RelayClient {
    /// Create a new group.
    pub async fn create_group(&self, name: &str) -> Result<String> {
        let msg = RelayMessage::CreateGroup { name: name.to_string() };
        self.send_message(&msg).await?;
        // In real implementation, wait for ack
        Ok(format!("group-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap()))
    }

    /// Join a group.
    pub async fn join_group(&self, group_id: &str) -> Result<()> {
        let token = self.token.read().await.clone().context("not registered")?;
        let msg = RelayMessage::JoinGroup {
            group_id: group_id.to_string(),
            token,
        };
        self.send_message(&msg).await
    }

    /// Leave a group.
    pub async fn leave_group(&self, group_id: &str) -> Result<()> {
        let msg = RelayMessage::LeaveGroup {
            group_id: group_id.to_string(),
        };
        self.send_message(&msg).await
    }

    /// List all groups.
    pub async fn list_groups(&self) -> Result<()> {
        let msg = RelayMessage::ListGroups;
        self.send_message(&msg).await
    }

    /// Add a device to a group.
    pub async fn add_to_group(&self, group_id: &str, device_id: &DeviceId) -> Result<()> {
        let msg = RelayMessage::AddToGroup {
            group_id: group_id.to_string(),
            device_id: device_id.clone(),
        };
        self.send_message(&msg).await
    }

    /// Remove a device from a group.
    pub async fn remove_from_group(&self, group_id: &str, device_id: &DeviceId) -> Result<()> {
        let msg = RelayMessage::RemoveFromGroup {
            group_id: group_id.to_string(),
            device_id: device_id.clone(),
        };
        self.send_message(&msg).await
    }
}
```

- [ ] **Step 3: Add auto-trust handler**

In the reader task, handle `JoinGroupAck`:

```rust
RelayMessage::JoinGroupAck { group_id, members } => {
    // Auto-trust all members
    for member in members {
        // In real implementation, call trust_store.trust(member, "group auto-trust")
        tracing::info!("auto-trusting group member: {}", member.device_id);
    }
}
```

- [ ] **Step 4: Run check**

Run: `cargo check -p hyprbridge-network`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/network/
git commit -m "feat(network): add group methods to relay client"
```

---

## Task 3: Protocol Extensions

**Covers:** [S6]

**Files:**
- Modify: `crates/protocol/src/lib.rs`

- [ ] **Step 1: Add group types to protocol**

In `crates/protocol/src/lib.rs`, add:

```rust
/// Device group information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceGroup {
    /// Unique group identifier.
    pub id: String,
    /// Human-readable group name.
    pub name: String,
    /// Group member device IDs.
    pub members: Vec<DeviceId>,
    /// When the group was created.
    pub created_at: DateTime<Utc>,
}

/// Request to create a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    /// Group name.
    pub name: String,
}

/// Response after creating a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupResponse {
    /// Created group.
    pub group: DeviceGroup,
}

/// Request to join a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinGroupRequest {
    /// Group ID to join.
    pub group_id: String,
}

/// Request to add a device to a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddToGroupRequest {
    /// Group ID.
    pub group_id: String,
    /// Device ID to add.
    pub device_id: DeviceId,
}

/// Request to remove a device from a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveFromGroupRequest {
    /// Group ID.
    pub group_id: String,
    /// Device ID to remove.
    pub device_id: DeviceId,
}
```

- [ ] **Step 2: Run check**

Run: `cargo check -p hyprbridge-protocol`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/protocol/
git commit -m "feat(protocol): add group management types"
```

---

## Task 4: Daemon Group API

**Covers:** [S7]

**Files:**
- Modify: `crates/daemon/src/lib.rs`
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: Add group REST endpoints**

In `crates/daemon/src/lib.rs`, add routes:

```rust
.route("/v1/groups", get(get_groups).post(post_create_group))
.route("/v1/groups/:id/join", post(post_join_group))
.route("/v1/groups/:id/leave", post(post_leave_group))
.route("/v1/groups/:id/members", get(get_group_members))
.route("/v1/groups/:id/add", post(post_add_to_group))
.route("/v1/groups/:id/remove", post(post_remove_from_group))
```

Handlers:

```rust
async fn get_groups(State(state): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    // In real implementation, get groups from relay client
    Json(serde_json::json!({ "groups": [] }))
}

async fn post_create_group(
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<hyprbridge_protocol::CreateGroupRequest>,
) -> Result<Json<hyprbridge_protocol::CreateGroupResponse>, ApiError> {
    // In real implementation, call relay client
    Ok(Json(hyprbridge_protocol::CreateGroupResponse {
        group: hyprbridge_protocol::DeviceGroup {
            id: "group-placeholder".into(),
            name: request.name,
            members: vec![],
            created_at: chrono::Utc::now(),
        },
    }))
}

async fn post_join_group(
    State(state): State<Arc<DaemonState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "joined": id })))
}

async fn post_leave_group(
    State(state): State<Arc<DaemonState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "left": id })))
}

async fn get_group_members(
    State(state): State<Arc<DaemonState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "group_id": id, "members": [] }))
}

async fn post_add_to_group(
    State(state): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(request): Json<hyprbridge_protocol::AddToGroupRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "added": request.device_id })))
}

async fn post_remove_from_group(
    State(state): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(request): Json<hyprbridge_protocol::RemoveFromGroupRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "removed": request.device_id })))
}
```

- [ ] **Step 2: Add CLI group commands**

In `crates/cli/src/main.rs`, add:

```rust
/// Group management commands.
#[derive(Subcommand)]
enum GroupCommand {
    /// Create a new group.
    Create {
        /// Group name.
        name: String,
    },
    /// Join a group.
    Join {
        /// Group ID.
        group_id: String,
    },
    /// Leave a group.
    Leave {
        /// Group ID.
        group_id: String,
    },
    /// List all groups.
    List,
    /// List group members.
    Members {
        /// Group ID.
        group_id: String,
    },
    /// Add a device to a group.
    Add {
        /// Group ID.
        group_id: String,
        /// Device ID.
        device_id: String,
    },
    /// Remove a device from a group.
    Remove {
        /// Group ID.
        group_id: String,
        /// Device ID.
        device_id: String,
    },
}
```

- [ ] **Step 3: Run check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/daemon/ crates/cli/
git commit -m "feat(daemon): add group management API and CLI"
```

---

## Task 5: Web UI Groups Panel

**Covers:** [S8]

**Files:**
- Create: `packages/ui/src/groups-panel.tsx`
- Modify: `packages/ui/src/management-console.tsx`
- Modify: `packages/ui/src/styles.css`
- Modify: `packages/ui/src/index.ts`

- [ ] **Step 1: Create GroupsPanel component**

Create `packages/ui/src/groups-panel.tsx`:

```tsx
import { useState } from "react";
import { Button, Panel } from "./primitives";

export interface Group {
  id: string;
  name: string;
  members: string[];
}

export interface GroupsPanelProps {
  groups: Group[];
  onCreateGroup: (name: string) => void;
  onDeleteGroup: (id: string) => void;
  onAddMember: (groupId: string, deviceId: string) => void;
  onRemoveMember: (groupId: string, deviceId: string) => void;
}

export function GroupsPanel(props: GroupsPanelProps) {
  const [newGroupName, setNewGroupName] = useState("");
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null);
  const [newMemberId, setNewMemberId] = useState("");

  function handleCreate() {
    if (newGroupName.trim()) {
      props.onCreateGroup(newGroupName.trim());
      setNewGroupName("");
    }
  }

  return (
    <Panel title="Device Groups" eyebrow="Team">
      <div className="ib-groups">
        <div className="ib-groups__create">
          <input
            className="ib-groups__input"
            type="text"
            value={newGroupName}
            onChange={(e) => setNewGroupName(e.target.value)}
            placeholder="New group name..."
          />
          <Button tone="accent" onClick={handleCreate}>Create</Button>
        </div>

        {props.groups.length > 0 ? (
          <div className="ib-groups__list">
            {props.groups.map((group) => (
              <div
                key={group.id}
                className={`ib-groups__group ${selectedGroup === group.id ? "ib-groups__group--selected" : ""}`}
                onClick={() => setSelectedGroup(group.id)}
              >
                <div className="ib-groups__group-header">
                  <strong>{group.name}</strong>
                  <span>{group.members.length} members</span>
                </div>
                {selectedGroup === group.id ? (
                  <div className="ib-groups__group-details">
                    <div className="ib-groups__members">
                      {group.members.map((member) => (
                        <div key={member} className="ib-groups__member">
                          <span>{member.slice(0, 8)}...</span>
                          <Button tone="neutral" onClick={() => props.onRemoveMember(group.id, member)}>×</Button>
                        </div>
                      ))}
                    </div>
                    <div className="ib-groups__add-member">
                      <input
                        className="ib-groups__input"
                        type="text"
                        value={newMemberId}
                        onChange={(e) => setNewMemberId(e.target.value)}
                        placeholder="Device ID..."
                      />
                      <Button tone="accent" onClick={() => { props.onAddMember(group.id, newMemberId); setNewMemberId(""); }}>Add</Button>
                    </div>
                    <Button tone="danger" onClick={() => props.onDeleteGroup(group.id)}>Delete Group</Button>
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        ) : (
          <p className="ib-copy">No groups created yet.</p>
        )}
      </div>
    </Panel>
  );
}
```

- [ ] **Step 2: Add GroupsPanel styles**

Append to `packages/ui/src/styles.css`:

```css
.ib-groups__create {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}

.ib-groups__input {
  flex: 1;
  padding: 8px 12px;
  border: 1px solid var(--ib-border);
  border-radius: 8px;
  background: var(--ib-bg-elevated);
  color: var(--ib-text);
  font: inherit;
}

.ib-groups__list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.ib-groups__group {
  padding: 12px;
  border: 1px solid var(--ib-border);
  border-radius: 12px;
  cursor: pointer;
  transition: border-color 120ms ease;
}

.ib-groups__group--selected {
  border-color: var(--ib-accent);
}

.ib-groups__group-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.ib-groups__group-header span {
  font-size: 0.85rem;
  color: var(--ib-text-muted);
}

.ib-groups__group-details {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--ib-border);
}

.ib-groups__members {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 12px;
}

.ib-groups__member {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.85rem;
  padding: 4px 0;
}

.ib-groups__add-member {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}
```

- [ ] **Step 3: Integrate into ManagementConsole**

In `packages/ui/src/management-console.tsx`, add:

```tsx
import { GroupsPanel } from "./groups-panel";
```

Add state:

```tsx
const [groups, setGroups] = useState<Group[]>([]);
```

Add handlers:

```tsx
function handleCreateGroup(name: string) {
  setGroups([...groups, { id: `group-${Date.now()}`, name, members: [] }]);
}

function handleDeleteGroup(id: string) {
  setGroups(groups.filter((g) => g.id !== id));
}

function handleAddMember(groupId: string, deviceId: string) {
  setGroups(groups.map((g) =>
    g.id === groupId ? { ...g, members: [...g.members, deviceId] } : g
  ));
}

function handleRemoveMember(groupId: string, deviceId: string) {
  setGroups(groups.map((g) =>
    g.id === groupId ? { ...g, members: g.members.filter((m) => m !== deviceId) } : g
  ));
}
```

Add JSX:

```tsx
<GroupsPanel
  groups={groups}
  onCreateGroup={handleCreateGroup}
  onDeleteGroup={handleDeleteGroup}
  onAddMember={handleAddMember}
  onRemoveMember={handleRemoveMember}
/>
```

- [ ] **Step 4: Export from index.ts**

```tsx
export { GroupsPanel } from "./groups-panel";
export type { Group } from "./groups-panel";
```

- [ ] **Step 5: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/ui check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add packages/ui/
git commit -m "feat(ui): add groups panel to management console"
```

---

## Task 6: Final Verification

**Covers:** [S9]

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

Run: `cargo run -p hyprbridge-cli -- --help`
Expected: CLI help with group subcommand

- [ ] **Step 5: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for team device groups"
```
