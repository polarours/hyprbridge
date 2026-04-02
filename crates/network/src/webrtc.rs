use std::collections::HashMap;

use chrono::{DateTime, Utc};
use hyprbridge_protocol::{DeviceId, DeviceIdentity};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast};

/// A unique identifier for a WebRTC session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WebRtcSessionId(pub String);

impl WebRtcSessionId {
    pub fn random() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Represents a WebRTC session used for screen sharing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcSession {
    pub id: WebRtcSessionId,
    pub local_device: DeviceIdentity,
    pub remote_device: Option<DeviceIdentity>,
    pub state: WebRtcSessionState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub detail: String,
}

/// WebRTC-specific session state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebRtcSessionState {
    Creating,
    Signaling,
    Connecting,
    Connected,
    Disconnecting,
    Disconnected,
    Failed,
}

/// A signaling message exchanged between peers to establish a WebRTC session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcSignal {
    pub session_id: WebRtcSessionId,
    pub sender: DeviceId,
    pub payload: WebRtcSignalPayload,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebRtcSignalPayload {
    Offer(String),
    Answer(String),
    IceCandidate(String),
    Hangup,
}

/// Manages WebRTC sessions for screen sharing.
#[derive(Debug)]
pub struct WebRtcManager {
    sessions: Mutex<HashMap<String, WebRtcSession>>,
    signal_tx: broadcast::Sender<WebRtcSignal>,
}

impl WebRtcManager {
    /// Create a new WebRTC manager with a signal broadcast channel.
    pub fn new() -> Self {
        let (signal_tx, _) = broadcast::channel(256);
        Self {
            sessions: Mutex::new(HashMap::new()),
            signal_tx,
        }
    }

    /// Create a new session for screen sharing.
    pub async fn create_session(
        &self,
        local_device: DeviceIdentity,
    ) -> WebRtcSession {
        let id = WebRtcSessionId::random();
        let now = Utc::now();
        let session = WebRtcSession {
            id: id.clone(),
            local_device,
            remote_device: None,
            state: WebRtcSessionState::Creating,
            created_at: now,
            updated_at: now,
            detail: "session created".into(),
        };
        self.sessions
            .lock()
            .await
            .insert(id.0.clone(), session.clone());
        session
    }

    /// Update the state of a session.
    pub async fn update_state(
        &self,
        session_id: &WebRtcSessionId,
        new_state: WebRtcSessionState,
        detail: &str,
    ) -> Result<WebRtcSession, &'static str> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(&session_id.0)
            .ok_or("session not found")?;
        session.state = new_state;
        session.detail = detail.into();
        session.updated_at = Utc::now();
        Ok(session.clone())
    }

    /// Get a session by id.
    pub async fn get_session(&self, session_id: &WebRtcSessionId) -> Option<WebRtcSession> {
        self.sessions.lock().await.get(&session_id.0).cloned()
    }

    /// List all active sessions.
    pub async fn list_sessions(&self) -> Vec<WebRtcSession> {
        let mut sessions: Vec<WebRtcSession> =
            self.sessions.lock().await.values().cloned().collect();
        sessions.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        sessions
    }

    /// Remove a session by id. Returns true if the session existed.
    pub async fn remove_session(&self, session_id: &WebRtcSessionId) -> bool {
        self.sessions.lock().await.remove(&session_id.0).is_some()
    }

    /// Broadcast a signaling message to all subscribers.
    pub fn send_signal(&self, signal: WebRtcSignal) {
        let _ = self.signal_tx.send(signal);
    }

    /// Subscribe to incoming signaling messages.
    pub fn subscribe_signals(&self) -> broadcast::Receiver<WebRtcSignal> {
        self.signal_tx.subscribe()
    }
}

impl Default for WebRtcManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use hyprbridge_protocol::{DeviceId, DeviceRole};

    use super::*;

    fn test_identity(name: &str) -> DeviceIdentity {
        DeviceIdentity {
            device_id: DeviceId(name.into()),
            name: name.into(),
            role: DeviceRole::Target,
            public_key: format!("key-{name}"),
        }
    }

    #[tokio::test]
    async fn create_session_returns_new_session() {
        let manager = WebRtcManager::new();
        let identity = test_identity("local");
        let session = manager.create_session(identity.clone()).await;

        assert_eq!(session.state, WebRtcSessionState::Creating);
        assert_eq!(session.local_device.device_id.0, "local");
        assert!(session.remote_device.is_none());
    }

    #[tokio::test]
    async fn list_sessions_returns_all() {
        let manager = WebRtcManager::new();
        manager.create_session(test_identity("a")).await;
        manager.create_session(test_identity("b")).await;

        let sessions = manager.list_sessions().await;
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn remove_session_removes_correctly() {
        let manager = WebRtcManager::new();
        let session = manager.create_session(test_identity("a")).await;

        assert!(manager.remove_session(&session.id).await);
        assert!(manager.get_session(&session.id).await.is_none());
        assert!(!manager.remove_session(&session.id).await);
    }

    #[tokio::test]
    async fn update_session_state() {
        let manager = WebRtcManager::new();
        let session = manager.create_session(test_identity("a")).await;

        let updated = manager
            .update_state(&session.id, WebRtcSessionState::Connected, "connected")
            .await
            .unwrap();
        assert_eq!(updated.state, WebRtcSessionState::Connected);
        assert_eq!(updated.detail, "connected");
    }

    #[tokio::test]
    async fn update_nonexistent_session_fails() {
        let manager = WebRtcManager::new();
        let fake_id = WebRtcSessionId("nonexistent".into());

        let result = manager
            .update_state(&fake_id, WebRtcSessionState::Failed, "nope")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn signal_broadcast_works() {
        let manager = WebRtcManager::new();
        let mut rx = manager.subscribe_signals();

        let session = manager.create_session(test_identity("a")).await;
        let signal = WebRtcSignal {
            session_id: session.id.clone(),
            sender: DeviceId("a".into()),
            payload: WebRtcSignalPayload::Offer("sdp".into()),
            timestamp: Utc::now(),
        };

        manager.send_signal(signal.clone());
        let received = rx.recv().await.unwrap();
        assert_eq!(received.session_id, session.id);
        assert_eq!(received.sender.0, "a");
    }
}
