//! Input recording engine for capturing, storing, and replaying input events.

/// Hybrid file format for recording persistence.
pub mod file;

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use hyprbridge_protocol::DeviceId;
use serde::{Deserialize, Serialize};

pub use file::{RecordingFile, RecordingFileHeader};

/// Types of input events that can be recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputEventType {
    /// A key was pressed down.
    KeyDown,
    /// A key was released.
    KeyUp,
    /// The mouse pointer moved.
    MouseMove,
    /// A mouse button state changed.
    MouseButton,
    /// The mouse scroll wheel was used.
    MouseScroll,
}

/// Data payload for a recorded event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RecordedEventData {
    /// Keyboard key event with keycode and pressed state.
    Key {
        /// Native keycode.
        keycode: u8,
        /// Whether the key was pressed or released.
        pressed: bool,
    },
    /// Mouse movement event.
    MouseMove {
        /// X delta from previous position.
        delta_x: i16,
        /// Y delta from previous position.
        delta_y: i16,
    },
    /// Mouse button event.
    MouseButton {
        /// Button identifier (0=left, 1=middle, 2=right).
        button: u8,
        /// Whether the button was pressed or released.
        pressed: bool,
    },
    /// Mouse scroll event.
    MouseScroll {
        /// Horizontal scroll steps.
        horizontal_steps: i16,
        /// Vertical scroll steps.
        vertical_steps: i16,
    },
}

/// A single recorded input event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedEvent {
    /// Timestamp when the event was recorded.
    pub timestamp: DateTime<Utc>,
    /// Type of the input event.
    pub event_type: InputEventType,
    /// Device that generated the event.
    pub device: DeviceId,
    /// Event-specific data.
    pub data: RecordedEventData,
}

/// A collection of recorded events forming a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSession {
    /// Unique session identifier.
    pub id: String,
    /// Human-readable session name.
    pub name: String,
    /// When the recording started.
    pub started_at: DateTime<Utc>,
    /// All recorded events in chronological order.
    pub events: Vec<RecordedEvent>,
    /// Total duration of the recording in milliseconds.
    pub duration_ms: u64,
}

/// Playback state of a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    /// Playback is not active.
    Stopped,
    /// Playback is actively running.
    Playing,
    /// Playback is paused at the current position.
    Paused,
}

/// Controls playback of a recording session.
pub struct PlaybackController {
    /// Current playback state.
    state: PlaybackState,
    /// Current position in the event list (index).
    position: usize,
    /// Playback speed multiplier (1.0 = normal, 2.0 = 2x faster).
    speed: f64,
    /// Timestamp of when playback started or resumed.
    start_time: Option<Instant>,
    /// Elapsed time accounting for pauses.
    elapsed: Duration,
}

impl PlaybackController {
    /// Create a new playback controller in stopped state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: PlaybackState::Stopped,
            position: 0,
            speed: 1.0,
            start_time: None,
            elapsed: Duration::ZERO,
        }
    }

    /// Start or resume playback.
    pub fn play(&mut self) {
        if self.state != PlaybackState::Playing {
            self.state = PlaybackState::Playing;
            self.start_time = Some(Instant::now());
        }
    }

    /// Pause playback at the current position.
    pub fn pause(&mut self) {
        if self.state == PlaybackState::Playing {
            self.state = PlaybackState::Paused;
            if let Some(start) = self.start_time.take() {
                self.elapsed += start.elapsed();
            }
        }
    }

    /// Stop playback and reset position.
    pub fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        self.position = 0;
        self.start_time = None;
        self.elapsed = Duration::ZERO;
    }

    /// Set the playback speed multiplier.
    ///
    /// # Panics
    ///
    /// Panics if `speed` is not positive.
    pub fn set_speed(&mut self, speed: f64) {
        assert!(speed > 0.0, "speed must be positive");
        self.speed = speed;
    }

    /// Advance playback position, returning the next event index if available.
    ///
    /// Returns `None` if playback is at the end or not playing.
    pub fn advance(&mut self, total_events: usize) -> Option<usize> {
        if self.state != PlaybackState::Playing {
            return None;
        }

        if self.position >= total_events {
            self.state = PlaybackState::Stopped;
            return None;
        }

        let current = self.position;
        self.position += 1;
        Some(current)
    }

    /// Get the current playback state.
    #[must_use]
    pub fn state(&self) -> PlaybackState {
        self.state
    }

    /// Get the current position in the event list.
    #[must_use]
    pub fn position(&self) -> usize {
        self.position
    }

    /// Get the current playback speed.
    #[must_use]
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Get the elapsed playback time.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

impl Default for PlaybackController {
    fn default() -> Self {
        Self::new()
    }
}

/// Records input events into a session.
pub struct InputRecorder {
    /// Current recording state.
    is_recording: bool,
    /// Current session being built.
    session: Option<RecordingSession>,
    /// When recording started.
    start_time: Option<Instant>,
}

impl InputRecorder {
    /// Create a new recorder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            is_recording: false,
            session: None,
            start_time: None,
        }
    }

    /// Start a new recording session with the given name.
    pub fn start(&mut self, name: &str) {
        if self.is_recording {
            tracing::warn!("Recording already in progress, ignoring start");
            return;
        }

        self.session = Some(RecordingSession {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            started_at: Utc::now(),
            events: Vec::new(),
            duration_ms: 0,
        });
        self.start_time = Some(Instant::now());
        self.is_recording = true;
        tracing::info!("Started recording session: {name}");
    }

    /// Record an input event.
    pub fn record_event(&mut self, event_type: InputEventType, device: &DeviceId, data: RecordedEventData) {
        if !self.is_recording {
            tracing::warn!("Not recording, ignoring event");
            return;
        }

        let event = RecordedEvent {
            timestamp: Utc::now(),
            event_type,
            device: device.clone(),
            data,
        };

        if let Some(session) = &mut self.session {
            session.events.push(event);
        }
    }

    /// Stop the current recording and return the session.
    pub fn stop(&mut self) -> Option<RecordingSession> {
        if !self.is_recording {
            return None;
        }

        self.is_recording = false;
        let duration_ms = self
            .start_time
            .take()
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let mut session = self.session.take()?;
        session.duration_ms = duration_ms;
        tracing::info!(
            "Stopped recording session: {} ({} events, {duration_ms}ms)",
            session.name,
            session.events.len()
        );
        Some(session)
    }

    /// Check if the recorder is currently recording.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.is_recording
    }
}

impl Default for InputRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_device() -> DeviceId {
        DeviceId::random()
    }

    #[test]
    fn recorder_lifecycle() {
        let mut recorder = InputRecorder::new();
        assert!(!recorder.is_recording());

        recorder.start("test");
        assert!(recorder.is_recording());

        let device = test_device();
        recorder.record_event(
            InputEventType::KeyDown,
            &device,
            RecordedEventData::Key {
                keycode: 42,
                pressed: true,
            },
        );
        recorder.record_event(
            InputEventType::KeyUp,
            &device,
            RecordedEventData::Key {
                keycode: 42,
                pressed: false,
            },
        );

        let session = recorder.stop();
        assert!(session.is_some());
        assert!(!recorder.is_recording());

        let session = session.unwrap();
        assert_eq!(session.events.len(), 2);
        assert_eq!(session.name, "test");
    }

    #[test]
    fn recorder_ignores_events_when_stopped() {
        let mut recorder = InputRecorder::new();
        let device = test_device();
        recorder.record_event(
            InputEventType::KeyDown,
            &device,
            RecordedEventData::Key {
                keycode: 42,
                pressed: true,
            },
        );
        assert!(recorder.stop().is_none());
    }

    #[test]
    fn recorder_multiple_sessions() {
        let mut recorder = InputRecorder::new();

        recorder.start("session1");
        let session1 = recorder.stop().unwrap();

        recorder.start("session2");
        let session2 = recorder.stop().unwrap();

        assert_ne!(session1.id, session2.id);
        assert_eq!(session1.name, "session1");
        assert_eq!(session2.name, "session2");
    }

    #[test]
    fn playback_controller_lifecycle() {
        let mut controller = PlaybackController::new();
        assert_eq!(controller.state(), PlaybackState::Stopped);
        assert_eq!(controller.position(), 0);

        controller.play();
        assert_eq!(controller.state(), PlaybackState::Playing);

        let pos = controller.advance(10);
        assert_eq!(pos, Some(0));
        assert_eq!(controller.position(), 1);

        controller.pause();
        assert_eq!(controller.state(), PlaybackState::Paused);

        controller.stop();
        assert_eq!(controller.state(), PlaybackState::Stopped);
        assert_eq!(controller.position(), 0);
    }

    #[test]
    fn playback_controller_speed_limits() {
        let mut controller = PlaybackController::new();
        assert_eq!(controller.speed(), 1.0);

        controller.set_speed(2.0);
        assert_eq!(controller.speed(), 2.0);

        controller.set_speed(0.5);
        assert_eq!(controller.speed(), 0.5);
    }

    #[test]
    #[should_panic(expected = "speed must be positive")]
    fn playback_controller_speed_zero_panics() {
        let mut controller = PlaybackController::new();
        controller.set_speed(0.0);
    }

    #[test]
    #[should_panic(expected = "speed must be positive")]
    fn playback_controller_speed_negative_panics() {
        let mut controller = PlaybackController::new();
        controller.set_speed(-1.0);
    }

    #[test]
    fn playback_controller_advance_past_end() {
        let mut controller = PlaybackController::new();
        controller.play();

        // Exhaust all events
        assert_eq!(controller.advance(3), Some(0));
        assert_eq!(controller.advance(3), Some(1));
        assert_eq!(controller.advance(3), Some(2));
        // Past the end
        assert_eq!(controller.advance(3), None);
        assert_eq!(controller.state(), PlaybackState::Stopped);
    }

    #[test]
    fn playback_controller_advance_when_paused() {
        let mut controller = PlaybackController::new();
        controller.play();
        assert_eq!(controller.advance(10), Some(0));
        controller.pause();
        assert_eq!(controller.advance(10), None);
    }

    #[test]
    fn recorded_event_serialization() {
        let event = RecordedEvent {
            timestamp: Utc::now(),
            event_type: InputEventType::KeyDown,
            device: test_device(),
            data: RecordedEventData::Key {
                keycode: 42,
                pressed: true,
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: RecordedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event.event_type, deserialized.event_type);
        assert_eq!(event.data, deserialized.data);
    }

    #[test]
    fn recording_session_serialization() {
        let mut recorder = InputRecorder::new();
        recorder.start("serialize test");

        let device = test_device();
        recorder.record_event(
            InputEventType::MouseMove,
            &device,
            RecordedEventData::MouseMove {
                delta_x: 10,
                delta_y: 20,
            },
        );

        let session = recorder.stop().unwrap();
        let json = serde_json::to_string(&session).unwrap();
        let deserialized: RecordingSession = serde_json::from_str(&json).unwrap();
        assert_eq!(session.events.len(), deserialized.events.len());
        assert_eq!(session.name, deserialized.name);
    }
}
