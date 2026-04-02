use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use hyprbridge_protocol::DeviceId;
use serde::{Deserialize, Serialize};

use crate::{RecordedEvent, RecordingSession};

/// File format version.
pub const FILE_VERSION: u32 = 1;

/// Header metadata for a recording file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingFileHeader {
    /// Format version for forward/backward compatibility.
    pub version: u32,
    /// Device that recorded the events.
    pub device_id: DeviceId,
    /// Platform identifier (e.g. "linux", "macos").
    pub platform: String,
    /// When the recording was created.
    pub created_at: DateTime<Utc>,
    /// Number of events in the file.
    pub event_count: usize,
    /// Free-form metadata key-value pairs.
    pub metadata: HashMap<String, String>,
}

/// A persisted recording with header and events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingFile {
    /// File header with metadata.
    pub header: RecordingFileHeader,
    /// All recorded events.
    pub events: Vec<RecordedEvent>,
}

impl RecordingFile {
    /// Build a `RecordingFile` from a completed recording session.
    #[must_use]
    pub fn from_session(session: &RecordingSession, platform: &str) -> Self {
        let header = RecordingFileHeader {
            version: FILE_VERSION,
            device_id: session
                .events
                .first()
                .map(|e| e.device.clone())
                .unwrap_or_else(DeviceId::random),
            platform: platform.to_string(),
            created_at: session.started_at,
            event_count: session.events.len(),
            metadata: HashMap::new(),
        };
        Self {
            header,
            events: session.events.clone(),
        }
    }

    /// Serialize and write the recording to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or file writing fails.
    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let json = serde_json::to_vec_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Read and deserialize a recording from a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or deserialized.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let data = std::fs::read(path)?;
        let file: Self = serde_json::from_slice(&data)?;
        if file.header.version > FILE_VERSION {
            anyhow::bail!(
                "unsupported file version {} (max supported: {})",
                file.header.version,
                FILE_VERSION
            );
        }
        Ok(file)
    }

    /// Get a reference to the recorded events.
    #[must_use]
    pub fn events(&self) -> &[RecordedEvent] {
        &self.events
    }

    /// Compute the duration of the recording in milliseconds.
    ///
    /// Returns 0 if there are fewer than two events.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        if self.events.len() < 2 {
            return 0;
        }
        let first = self.events.first().expect("checked len >= 2");
        let last = self.events.last().expect("checked len >= 2");
        (last.timestamp - first.timestamp).num_milliseconds().max(0) as u64
    }

    /// Number of events in the recording.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InputEventType, RecordedEventData};
    use chrono::Duration;

    fn make_event(ts: DateTime<Utc>, keycode: u8) -> RecordedEvent {
        RecordedEvent {
            timestamp: ts,
            event_type: InputEventType::KeyDown,
            device: DeviceId::random(),
            data: RecordedEventData::Key {
                keycode,
                pressed: true,
            },
        }
    }

    fn make_session(events: Vec<RecordedEvent>) -> RecordingSession {
        RecordingSession {
            id: uuid::Uuid::new_v4().to_string(),
            name: "test".into(),
            started_at: events.first().map(|e| e.timestamp).unwrap_or_else(Utc::now),
            events,
            duration_ms: 0,
        }
    }

    #[test]
    fn from_session_copies_events() {
        let now = Utc::now();
        let session = make_session(vec![make_event(now, 1), make_event(now, 2)]);
        let file = RecordingFile::from_session(&session, "linux");
        assert_eq!(file.event_count(), 2);
        assert_eq!(file.header.platform, "linux");
        assert_eq!(file.header.version, FILE_VERSION);
    }

    #[test]
    fn roundtrip_save_load() {
        let now = Utc::now();
        let session = make_session(vec![
            make_event(now, 10),
            make_event(now + Duration::milliseconds(50), 20),
        ]);
        let file = RecordingFile::from_session(&session, "macos");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ibr");
        file.save(&path).unwrap();

        let loaded = RecordingFile::load(&path).unwrap();
        assert_eq!(loaded.event_count(), 2);
        assert_eq!(loaded.header.platform, "macos");
        assert_eq!(loaded.events[0].data, file.events[0].data);
        assert_eq!(loaded.events[1].data, file.events[1].data);
    }

    #[test]
    fn duration_ms_empty() {
        let file = RecordingFile {
            header: RecordingFileHeader {
                version: FILE_VERSION,
                device_id: DeviceId::random(),
                platform: "test".into(),
                created_at: Utc::now(),
                event_count: 0,
                metadata: HashMap::new(),
            },
            events: vec![],
        };
        assert_eq!(file.duration_ms(), 0);
    }

    #[test]
    fn duration_ms_single_event() {
        let file = RecordingFile {
            header: RecordingFileHeader {
                version: FILE_VERSION,
                device_id: DeviceId::random(),
                platform: "test".into(),
                created_at: Utc::now(),
                event_count: 1,
                metadata: HashMap::new(),
            },
            events: vec![make_event(Utc::now(), 1)],
        };
        assert_eq!(file.duration_ms(), 0);
    }

    #[test]
    fn duration_ms_calculation() {
        let t0 = Utc::now();
        let session = make_session(vec![
            make_event(t0, 1),
            make_event(t0 + Duration::milliseconds(100), 2),
            make_event(t0 + Duration::milliseconds(250), 3),
        ]);
        let file = RecordingFile::from_session(&session, "linux");
        assert_eq!(file.duration_ms(), 250);
    }

    #[test]
    fn load_rejects_unsupported_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.ibr");
        let file = RecordingFile {
            header: RecordingFileHeader {
                version: u32::MAX,
                device_id: DeviceId::random(),
                platform: "test".into(),
                created_at: Utc::now(),
                event_count: 0,
                metadata: HashMap::new(),
            },
            events: vec![],
        };
        file.save(&path).unwrap();
        let err = RecordingFile::load(&path).unwrap_err();
        assert!(err.to_string().contains("unsupported file version"));
    }

    #[test]
    fn metadata_roundtrip() {
        let mut header = RecordingFileHeader {
            version: FILE_VERSION,
            device_id: DeviceId::random(),
            platform: "linux".into(),
            created_at: Utc::now(),
            event_count: 0,
            metadata: HashMap::new(),
        };
        header
            .metadata
            .insert("source".into(), "test-harness".into());
        header.metadata.insert("app_version".into(), "0.1.0".into());

        let file = RecordingFile {
            header,
            events: vec![],
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.ibr");
        file.save(&path).unwrap();
        let loaded = RecordingFile::load(&path).unwrap();
        assert_eq!(
            loaded.header.metadata.get("source").unwrap(),
            "test-harness"
        );
        assert_eq!(loaded.header.metadata.get("app_version").unwrap(), "0.1.0");
    }
}
