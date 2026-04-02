//! Core domain models for topology and ownership.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use hyprbridge_protocol::{
    DeviceId, EdgeMapping, OwnershipSnapshot, OwnershipState, ScreenEdge, TopologySnapshot,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors returned by topology operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TopologyError {
    /// Multiple devices were bound to the same local edge.
    #[error("duplicate binding for edge {0:?}")]
    DuplicateEdge(ScreenEdge),
    /// No mapping was found for an edge.
    #[error("no mapping for edge {0:?}")]
    UnmappedEdge(ScreenEdge),
}

/// Configurable topology of local to remote screen edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyLayout {
    /// Local edge bindings.
    pub bindings: HashMap<ScreenEdge, EdgeMapping>,
    /// Number of pixels treated as a dead zone.
    pub dead_zone_px: u16,
}

impl Default for TopologyLayout {
    fn default() -> Self {
        Self {
            bindings: HashMap::new(),
            dead_zone_px: 12,
        }
    }
}

impl TopologyLayout {
    /// Build a topology from a flat list of mappings.
    pub fn try_from_mappings(
        mappings: Vec<EdgeMapping>,
        dead_zone_px: u16,
    ) -> Result<Self, TopologyError> {
        let mut bindings = HashMap::new();
        for mapping in mappings {
            if bindings
                .insert(mapping.local_edge, mapping.clone())
                .is_some()
            {
                return Err(TopologyError::DuplicateEdge(mapping.local_edge));
            }
        }

        Ok(Self {
            bindings,
            dead_zone_px,
        })
    }

    /// Resolve the mapping for a local edge.
    pub fn route_for(&self, edge: ScreenEdge) -> Result<&EdgeMapping, TopologyError> {
        self.bindings
            .get(&edge)
            .ok_or(TopologyError::UnmappedEdge(edge))
    }

    /// Convert the topology into a protocol snapshot.
    #[must_use]
    pub fn snapshot(&self) -> TopologySnapshot {
        let mut edges = self.bindings.values().cloned().collect::<Vec<_>>();
        edges.sort_by_key(|mapping| match mapping.local_edge {
            ScreenEdge::Left => 0,
            ScreenEdge::Right => 1,
            ScreenEdge::Top => 2,
            ScreenEdge::Bottom => 3,
        });

        TopologySnapshot {
            edges,
            dead_zone_px: self.dead_zone_px,
        }
    }
}

/// State transitions for ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipEvent {
    /// Local pointer hit a mapped edge.
    EdgeCrossed(ScreenEdge),
    /// Remote device accepted ownership.
    RemoteAccepted(DeviceId),
    /// User or runtime requested a local return.
    ReturnLocal,
    /// Current remote session disconnected.
    RemoteDisconnected(DeviceId),
    /// Suspend ownership handling.
    Suspend,
    /// Resume ownership handling.
    Resume,
}

/// Deterministic ownership state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipMachine {
    state: OwnershipState,
    active_remote: Option<DeviceId>,
    crossing_timeout: Duration,
    entered_crossing_at: Option<Instant>,
}

impl Default for OwnershipMachine {
    fn default() -> Self {
        Self::with_timeout(Duration::from_secs(5))
    }
}

impl OwnershipMachine {
    /// Create a new ownership machine with a custom crossing timeout.
    pub fn with_timeout(crossing_timeout: Duration) -> Self {
        Self {
            state: OwnershipState::LocalActive,
            active_remote: None,
            crossing_timeout,
            entered_crossing_at: None,
        }
    }

    /// Access the current state.
    #[must_use]
    pub fn snapshot(&self) -> OwnershipSnapshot {
        OwnershipSnapshot {
            state: self.state,
            active_remote: self.active_remote.clone(),
        }
    }

    /// Apply a state transition.
    pub fn apply(&mut self, event: OwnershipEvent) -> OwnershipSnapshot {
        match event {
            OwnershipEvent::EdgeCrossed(_) if self.state == OwnershipState::LocalActive => {
                self.state = OwnershipState::CrossingEdge;
                self.entered_crossing_at = Some(Instant::now());
            }
            OwnershipEvent::RemoteAccepted(device)
                if self.state == OwnershipState::CrossingEdge =>
            {
                self.state = OwnershipState::RemoteActive;
                self.active_remote = Some(device);
            }
            OwnershipEvent::ReturnLocal if self.state == OwnershipState::RemoteActive => {
                self.state = OwnershipState::ReturningLocal;
            }
            OwnershipEvent::RemoteDisconnected(device)
                if self.active_remote.as_ref() == Some(&device) =>
            {
                self.state = OwnershipState::LocalActive;
                self.active_remote = None;
            }
            OwnershipEvent::Suspend => {
                self.state = OwnershipState::Suspended;
            }
            OwnershipEvent::Resume if self.state == OwnershipState::Suspended => {
                self.state = if self.active_remote.is_some() {
                    OwnershipState::RemoteActive
                } else {
                    OwnershipState::LocalActive
                };
            }
            OwnershipEvent::ReturnLocal if self.state == OwnershipState::ReturningLocal => {
                self.state = OwnershipState::LocalActive;
                self.active_remote = None;
            }
            _ => {}
        }

        self.snapshot()
    }

    /// Check for crossing timeout and auto-transition if needed.
    pub fn tick(&mut self) -> OwnershipSnapshot {
        if self.state == OwnershipState::CrossingEdge
            && let Some(entered) = self.entered_crossing_at
            && entered.elapsed() > self.crossing_timeout
        {
            self.state = OwnershipState::ReturningLocal;
            self.entered_crossing_at = None;
        }
        self.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use hyprbridge_protocol::{DeviceId, EdgeMapping, OwnershipState, ScreenEdge};

    use super::{OwnershipEvent, OwnershipMachine, TopologyLayout};

    #[test]
    fn topology_rejects_duplicate_edges() {
        let result = TopologyLayout::try_from_mappings(
            vec![
                EdgeMapping {
                    local_edge: ScreenEdge::Left,
                    remote_device: DeviceId("a".into()),
                    remote_edge: ScreenEdge::Right,
                },
                EdgeMapping {
                    local_edge: ScreenEdge::Left,
                    remote_device: DeviceId("b".into()),
                    remote_edge: ScreenEdge::Right,
                },
            ],
            8,
        );

        assert!(result.is_err());
    }

    #[test]
    fn topology_snapshot_is_stable() {
        let layout = TopologyLayout::try_from_mappings(
            vec![
                EdgeMapping {
                    local_edge: ScreenEdge::Bottom,
                    remote_device: DeviceId("b".into()),
                    remote_edge: ScreenEdge::Top,
                },
                EdgeMapping {
                    local_edge: ScreenEdge::Left,
                    remote_device: DeviceId("a".into()),
                    remote_edge: ScreenEdge::Right,
                },
            ],
            14,
        )
        .expect("topology should be valid");

        let snapshot = layout.snapshot();

        assert_eq!(snapshot.edges[0].local_edge, ScreenEdge::Left);
        assert_eq!(snapshot.edges[1].local_edge, ScreenEdge::Bottom);
        assert_eq!(snapshot.dead_zone_px, 14);
    }

    #[test]
    fn ownership_transitions_are_deterministic() {
        let mut machine = OwnershipMachine::default();
        let remote = DeviceId("remote".into());

        assert_eq!(machine.snapshot().state, OwnershipState::LocalActive);
        assert_eq!(
            machine
                .apply(OwnershipEvent::EdgeCrossed(ScreenEdge::Right))
                .state,
            OwnershipState::CrossingEdge
        );
        assert_eq!(
            machine
                .apply(OwnershipEvent::RemoteAccepted(remote.clone()))
                .state,
            OwnershipState::RemoteActive
        );
        assert_eq!(
            machine.apply(OwnershipEvent::ReturnLocal).state,
            OwnershipState::ReturningLocal
        );
        assert_eq!(
            machine.apply(OwnershipEvent::ReturnLocal).state,
            OwnershipState::LocalActive
        );
        assert_eq!(machine.snapshot().active_remote, None);
    }

    #[test]
    fn crossing_edge_times_out_to_returning_local() {
        use std::time::Duration;

        let mut machine = OwnershipMachine::with_timeout(Duration::from_millis(100));
        machine.apply(OwnershipEvent::EdgeCrossed(ScreenEdge::Right));
        assert_eq!(machine.snapshot().state, OwnershipState::CrossingEdge);

        std::thread::sleep(Duration::from_millis(150));

        let snapshot = machine.tick();
        assert_eq!(snapshot.state, OwnershipState::ReturningLocal);
    }

    #[test]
    fn suspend_and_resume_preserve_remote_context() {
        let mut machine = OwnershipMachine::default();
        let remote = DeviceId("remote".into());

        machine.apply(OwnershipEvent::EdgeCrossed(ScreenEdge::Left));
        machine.apply(OwnershipEvent::RemoteAccepted(remote.clone()));
        machine.apply(OwnershipEvent::Suspend);

        assert_eq!(machine.snapshot().state, OwnershipState::Suspended);

        machine.apply(OwnershipEvent::Resume);

        assert_eq!(machine.snapshot().state, OwnershipState::RemoteActive);
        assert_eq!(machine.snapshot().active_remote, Some(remote));
    }
}
