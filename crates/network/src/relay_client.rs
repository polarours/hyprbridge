//! Relay client for non-LAN connections.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use hyprbridge_protocol::{ControlFrame, DeviceId, DeviceIdentity};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RelayMessage {
    Register { device: DeviceIdentity },
    RegisterAck { token: String, room_id: String },
    JoinRoom { room_id: String, token: String },
    JoinRoomAck { room_id: String, devices: Vec<DeviceIdentity> },
    Forward { target: DeviceId, frame: ControlFrame },
    Received { source: DeviceIdentity, frame: ControlFrame },
    Error { detail: String },
    CreateGroup { name: String },
    CreateGroupAck { group_id: String },
    JoinGroup { group_id: String, token: String },
    JoinGroupAck { group_id: String, members: Vec<DeviceIdentity> },
    LeaveGroup { group_id: String },
    ListGroups,
    ListGroupsAck { groups: Vec<GroupInfo> },
    AddToGroup { group_id: String, device_id: DeviceId },
    RemoveFromGroup { group_id: String, device_id: DeviceId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub member_count: usize,
}

pub struct RelayClient {
    server_url: String,
    token: RwLock<Option<String>>,
    room_id: RwLock<Option<String>>,
    local_device: DeviceIdentity,
    ws_writer: RwLock<
        Option<
            futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                Message,
            >,
        >,
    >,
    frame_sender: broadcast::Sender<(DeviceIdentity, ControlFrame)>,
}

impl RelayClient {
    pub fn new(server_url: String, local_device: DeviceIdentity) -> Self {
        let (frame_sender, _) = broadcast::channel(64);
        Self {
            server_url,
            token: RwLock::new(None),
            room_id: RwLock::new(None),
            local_device,
            ws_writer: RwLock::new(None),
            frame_sender,
        }
    }

    pub async fn connect(&self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.server_url)
            .await
            .context("failed to connect to relay server")?;
        let (writer, mut reader) = ws_stream.split();
        *self.ws_writer.write().await = Some(writer);
        let frame_sender = self.frame_sender.clone();
        tokio::spawn(async move {
            while let Some(msg) = reader.next().await {
                if let Ok(Message::Text(text)) = msg
                    && let Ok(msg) = serde_json::from_str::<RelayMessage>(&text)
                {
                    match msg {
                        RelayMessage::Received { source, frame } => {
                            let _ = frame_sender.send((source, frame));
                        }
                        RelayMessage::JoinGroupAck { group_id: _, members } => {
                            for member in members {
                                tracing::info!(
                                    "auto-trusting group member: {:?}",
                                    member.device_id
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        });
        Ok(())
    }

    pub async fn register(&self) -> Result<()> {
        let msg = RelayMessage::Register {
            device: self.local_device.clone(),
        };
        self.send_message(&msg).await
    }

    pub async fn join_room(&self, room_id: &str) -> Result<()> {
        let token = self
            .token
            .read()
            .await
            .clone()
            .context("not registered")?;
        let msg = RelayMessage::JoinRoom {
            room_id: room_id.to_string(),
            token,
        };
        self.send_message(&msg).await?;
        *self.room_id.write().await = Some(room_id.to_string());
        Ok(())
    }

    pub async fn send_frame(&self, target: &DeviceId, frame: ControlFrame) -> Result<()> {
        let msg = RelayMessage::Forward {
            target: target.clone(),
            frame,
        };
        self.send_message(&msg).await
    }

    pub async fn create_group(&self, name: &str) -> Result<String> {
        let msg = RelayMessage::CreateGroup {
            name: name.to_string(),
        };
        self.send_message(&msg).await?;
        Ok(format!(
            "group-{}",
            uuid::Uuid::new_v4().to_string().split('-').next().context("invalid uuid")?
        ))
    }

    pub async fn join_group(&self, group_id: &str) -> Result<()> {
        let token = self.token.read().await.clone().context("not registered")?;
        let msg = RelayMessage::JoinGroup {
            group_id: group_id.to_string(),
            token,
        };
        self.send_message(&msg).await
    }

    pub async fn leave_group(&self, group_id: &str) -> Result<()> {
        let msg = RelayMessage::LeaveGroup {
            group_id: group_id.to_string(),
        };
        self.send_message(&msg).await
    }

    pub async fn list_groups(&self) -> Result<()> {
        let msg = RelayMessage::ListGroups;
        self.send_message(&msg).await
    }

    pub async fn add_to_group(&self, group_id: &str, device_id: &DeviceId) -> Result<()> {
        let msg = RelayMessage::AddToGroup {
            group_id: group_id.to_string(),
            device_id: device_id.clone(),
        };
        self.send_message(&msg).await
    }

    pub async fn remove_from_group(&self, group_id: &str, device_id: &DeviceId) -> Result<()> {
        let msg = RelayMessage::RemoveFromGroup {
            group_id: group_id.to_string(),
            device_id: device_id.clone(),
        };
        self.send_message(&msg).await
    }

    pub fn subscribe_frames(&self) -> broadcast::Receiver<(DeviceIdentity, ControlFrame)> {
        self.frame_sender.subscribe()
    }

    pub async fn room_id(&self) -> Option<String> {
        self.room_id.read().await.clone()
    }

    pub async fn is_connected(&self) -> bool {
        self.ws_writer.read().await.is_some()
    }

    async fn send_message(&self, msg: &RelayMessage) -> Result<()> {
        let mut writer = self.ws_writer.write().await;
        let writer = writer.as_mut().context("not connected")?;
        let text = serde_json::to_string(msg).context("failed to serialize message")?;
        writer
            .send(Message::Text(text))
            .await
            .context("failed to send message")?;
        Ok(())
    }
}

impl std::fmt::Debug for RelayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayClient")
            .field("server_url", &self.server_url)
            .finish()
    }
}
