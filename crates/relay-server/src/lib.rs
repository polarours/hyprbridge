//! Relay server for non-LAN HyprBridge connections.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use hyprbridge_protocol::{DeviceId, DeviceIdentity, DeviceRole};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RelayMessage {
    Register { device: DeviceIdentity },
    RegisterAck { token: String, room_id: String },
    JoinRoom { room_id: String, token: String },
    JoinRoomAck { room_id: String, devices: Vec<DeviceIdentity> },
    Forward { target: DeviceId, frame: hyprbridge_protocol::ControlFrame },
    Received { source: DeviceIdentity, frame: hyprbridge_protocol::ControlFrame },
    CreateGroup { name: String },
    CreateGroupAck { group: GroupInfo },
    JoinGroup { group_id: String },
    JoinGroupAck { group: GroupInfo },
    LeaveGroup { group_id: String },
    LeaveGroupAck { group_id: String },
    ListGroups,
    ListGroupsAck { groups: Vec<GroupInfo> },
    Error { detail: String },
}

#[derive(Debug, Clone)]
pub struct Room {
    pub id: String,
    pub devices: HashMap<DeviceId, DeviceSender>,
}

#[derive(Debug, Clone)]
pub struct DeviceSender {
    pub identity: DeviceIdentity,
    pub sender: broadcast::Sender<String>,
}

/// A device group for team setups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub owner: DeviceId,
    pub members: Vec<DeviceId>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Manages device groups.
pub struct GroupManager {
    pub groups: RwLock<HashMap<String, Group>>,
    pub device_groups: RwLock<HashMap<DeviceId, Vec<String>>>,
}

impl GroupManager {
    pub fn new() -> Self {
        Self {
            groups: RwLock::new(HashMap::new()),
            device_groups: RwLock::new(HashMap::new()),
        }
    }

    pub async fn create_group(&self, name: String, owner: DeviceId) -> Group {
        let id = format!(
            "group-{}",
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        let group = Group {
            id: id.clone(),
            name,
            owner: owner.clone(),
            members: vec![owner.clone()],
            created_at: chrono::Utc::now(),
        };
        self.groups.write().await.insert(id.clone(), group.clone());
        self.device_groups
            .write()
            .await
            .entry(owner)
            .or_default()
            .push(id.clone());
        group
    }

    pub async fn add_member(&self, group_id: &str, device_id: DeviceId) -> Result<Group, String> {
        let mut groups = self.groups.write().await;
        let group = groups.get_mut(group_id).ok_or("group not found")?;
        if !group.members.contains(&device_id) {
            group.members.push(device_id.clone());
        }
        self.device_groups
            .write()
            .await
            .entry(device_id)
            .or_default()
            .push(group_id.to_string());
        Ok(group.clone())
    }

    pub async fn remove_member(&self, group_id: &str, device_id: &DeviceId) -> Result<Group, String> {
        let mut groups = self.groups.write().await;
        let group = groups.get_mut(group_id).ok_or("group not found")?;
        group.members.retain(|m| m != device_id);
        if let Some(device_groups) = self.device_groups.write().await.get_mut(device_id) {
            device_groups.retain(|g| g != group_id);
        }
        Ok(group.clone())
    }

    pub async fn list_groups(&self) -> Vec<Group> {
        self.groups.read().await.values().cloned().collect()
    }

    pub async fn get_members(&self, group_id: &str) -> Result<Vec<DeviceId>, String> {
        let groups = self.groups.read().await;
        let group = groups.get(group_id).ok_or("group not found")?;
        Ok(group.members.clone())
    }

    pub async fn delete_group(&self, group_id: &str) -> Result<(), String> {
        let group = self
            .groups
            .write()
            .await
            .remove(group_id)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub member_count: usize,
}

pub struct RelayState {
    pub rooms: RwLock<HashMap<String, Room>>,
    pub device_rooms: RwLock<HashMap<DeviceId, String>>,
    pub tokens: RwLock<HashMap<String, DeviceId>>,
    pub group_manager: GroupManager,
}

impl RelayState {
    pub fn new() -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            device_rooms: RwLock::new(HashMap::new()),
            tokens: RwLock::new(HashMap::new()),
            group_manager: GroupManager::new(),
        }
    }

    pub async fn register(&self, device: DeviceIdentity) -> (String, String) {
        let token = uuid::Uuid::new_v4().to_string();
        let room_id = format!("room-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());
        self.tokens.write().await.insert(token.clone(), device.device_id.clone());
        self.device_rooms.write().await.insert(device.device_id.clone(), room_id.clone());
        let mut rooms = self.rooms.write().await;
        let room = rooms.entry(room_id.clone()).or_insert_with(|| Room {
            id: room_id.clone(),
            devices: HashMap::new(),
        });
        let (sender, _) = broadcast::channel(64);
        room.devices.insert(device.device_id.clone(), DeviceSender {
            identity: device,
            sender,
        });
        (token, room_id)
    }

    pub async fn join_room(&self, room_id: &str, token: &str) -> Result<Vec<DeviceIdentity>, String> {
        let device_id = self.tokens.read().await.get(token).cloned()
            .ok_or("invalid token")?;
        let device_rooms = self.device_rooms.write().await;
        if device_rooms.contains_key(&device_id) {
            return Err("device already in a room".into());
        }
        drop(device_rooms);
        let mut rooms = self.rooms.write().await;
        let room = rooms.get_mut(room_id).ok_or("room not found")?;
        let (sender, _) = broadcast::channel(64);
        let identity = DeviceIdentity {
            device_id: device_id.clone(),
            name: "relay-client".into(),
            role: DeviceRole::Target,
            public_key: String::new(),
        };
        room.devices.insert(device_id.clone(), DeviceSender {
            identity: identity.clone(),
            sender,
        });
        self.device_rooms.write().await.insert(device_id, room_id.to_string());
        let devices: Vec<DeviceIdentity> = room.devices.values().map(|d| d.identity.clone()).collect();
        Ok(devices)
    }

    pub async fn get_sender(&self, device_id: &DeviceId) -> Option<broadcast::Sender<String>> {
        let room_id = self.device_rooms.read().await.get(device_id)?.clone();
        let rooms = self.rooms.read().await;
        rooms.get(&room_id)?.devices.get(device_id).map(|d| d.sender.clone())
    }
}

impl Default for RelayState {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn handle_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RelayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<RelayState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut registered_device: Option<DeviceId> = None;

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text,
            Ok(_) => continue,
            Err(_) => break,
        };

        let relay_msg: RelayMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(_) => {
                let err = RelayMessage::Error {
                    detail: "invalid message format".into(),
                };
                let _ = sender
                    .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                    .await;
                continue;
            }
        };

        match relay_msg {
            RelayMessage::Register { device } => {
                let (token, room_id) = state.register(device.clone()).await;
                registered_device = Some(device.device_id.clone());
                let ack = RelayMessage::RegisterAck { token, room_id };
                let _ = sender
                    .send(Message::Text(serde_json::to_string(&ack).unwrap().into()))
                    .await;
            }
            RelayMessage::JoinRoom { room_id, token } => match state.join_room(&room_id, &token).await {
                Ok(devices) => {
                    let ack = RelayMessage::JoinRoomAck { room_id, devices };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&ack).unwrap().into()))
                        .await;
                }
                Err(e) => {
                    let err = RelayMessage::Error { detail: e };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                }
            },
            RelayMessage::Forward { target, frame } => {
                if let Some(device_id) = &registered_device {
                    let rooms = state.rooms.read().await;
                    let source_identity = rooms.values().find_map(|room| {
                        room.devices
                            .get(device_id)
                            .map(|d| d.identity.clone())
                    });
                    if let Some(source) = source_identity {
                        for room in rooms.values() {
                            if let Some(target_sender) = room.devices.get(&target) {
                                let msg = RelayMessage::Received { source, frame };
                                let _ = target_sender
                                    .sender
                                    .send(serde_json::to_string(&msg).unwrap());
                                break;
                            }
                        }
                    }
                }
            }
            RelayMessage::CreateGroup { name } => {
                if let Some(device_id) = &registered_device {
                    let group = state
                        .group_manager
                        .create_group(name, device_id.clone())
                        .await;
                    let info = GroupInfo {
                        id: group.id,
                        name: group.name,
                        member_count: group.members.len(),
                    };
                    let ack = RelayMessage::CreateGroupAck { group: info };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&ack).unwrap().into()))
                        .await;
                } else {
                    let err = RelayMessage::Error {
                        detail: "not registered".into(),
                    };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                }
            }
            RelayMessage::JoinGroup { group_id } => {
                if let Some(device_id) = &registered_device {
                    match state
                        .group_manager
                        .add_member(&group_id, device_id.clone())
                        .await
                    {
                        Ok(group) => {
                            let info = GroupInfo {
                                id: group.id,
                                name: group.name,
                                member_count: group.members.len(),
                            };
                            let ack = RelayMessage::JoinGroupAck { group: info };
                            let _ = sender
                                .send(Message::Text(serde_json::to_string(&ack).unwrap().into()))
                                .await;
                        }
                        Err(e) => {
                            let err = RelayMessage::Error { detail: e };
                            let _ = sender
                                .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                                .await;
                        }
                    }
                } else {
                    let err = RelayMessage::Error {
                        detail: "not registered".into(),
                    };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                }
            }
            RelayMessage::LeaveGroup { group_id } => {
                if let Some(device_id) = &registered_device {
                    match state
                        .group_manager
                        .remove_member(&group_id, device_id)
                        .await
                    {
                        Ok(_) => {
                            let ack = RelayMessage::LeaveGroupAck {
                                group_id: group_id.clone(),
                            };
                            let _ = sender
                                .send(Message::Text(serde_json::to_string(&ack).unwrap().into()))
                                .await;
                        }
                        Err(e) => {
                            let err = RelayMessage::Error { detail: e };
                            let _ = sender
                                .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                                .await;
                        }
                    }
                } else {
                    let err = RelayMessage::Error {
                        detail: "not registered".into(),
                    };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                }
            }
            RelayMessage::ListGroups => {
                let groups = state.group_manager.list_groups().await;
                let infos: Vec<GroupInfo> = groups
                    .into_iter()
                    .map(|g| GroupInfo {
                        id: g.id,
                        name: g.name,
                        member_count: g.members.len(),
                    })
                    .collect();
                let ack = RelayMessage::ListGroupsAck { groups: infos };
                let _ = sender
                    .send(Message::Text(serde_json::to_string(&ack).unwrap().into()))
                    .await;
            }
            _ => {
                let err = RelayMessage::Error {
                    detail: "unexpected message type".into(),
                };
                let _ = sender
                    .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                    .await;
            }
        }
    }

    if let Some(device_id) = registered_device {
        let mut device_rooms = state.device_rooms.write().await;
        if let Some(room_id) = device_rooms.remove(&device_id) {
            let mut rooms = state.rooms.write().await;
            if let Some(room) = rooms.get_mut(&room_id) {
                room.devices.remove(&device_id);
                if room.devices.is_empty() {
                    rooms.remove(&room_id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_returns_token_and_room() {
        let state = RelayState::new();
        let device = DeviceIdentity {
            device_id: DeviceId("test-device".into()),
            name: "Test".into(),
            role: DeviceRole::Target,
            public_key: "key".into(),
        };
        let (token, room_id) = state.register(device).await;
        assert!(!token.is_empty());
        assert!(!room_id.is_empty());
    }

    #[tokio::test]
    async fn join_room_requires_valid_token() {
        let state = RelayState::new();
        let result = state.join_room("room-123", "invalid-token").await;
        assert!(result.is_err());
    }
}