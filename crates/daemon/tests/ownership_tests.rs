use std::time::Duration;
use hyprbridge_core::{OwnershipEvent, OwnershipMachine};
use hyprbridge_protocol::{DeviceId, OwnershipState, ScreenEdge};

#[test]
fn full_ownership_lifecycle() {
    let mut machine = OwnershipMachine::default();
    let remote = DeviceId("test-remote".into());
    assert_eq!(machine.snapshot().state, OwnershipState::LocalActive);
    machine.apply(OwnershipEvent::EdgeCrossed(ScreenEdge::Right));
    assert_eq!(machine.snapshot().state, OwnershipState::CrossingEdge);
    machine.apply(OwnershipEvent::RemoteAccepted(remote.clone()));
    assert_eq!(machine.snapshot().state, OwnershipState::RemoteActive);
    assert_eq!(machine.snapshot().active_remote, Some(remote.clone()));
    machine.apply(OwnershipEvent::ReturnLocal);
    assert_eq!(machine.snapshot().state, OwnershipState::ReturningLocal);
    machine.apply(OwnershipEvent::ReturnLocal);
    assert_eq!(machine.snapshot().state, OwnershipState::LocalActive);
    assert_eq!(machine.snapshot().active_remote, None);
}

#[test]
fn timeout_transitions_to_returning_local() {
    let mut machine = OwnershipMachine::with_timeout(Duration::from_millis(50));
    machine.apply(OwnershipEvent::EdgeCrossed(ScreenEdge::Right));
    assert_eq!(machine.snapshot().state, OwnershipState::CrossingEdge);
    std::thread::sleep(Duration::from_millis(100));
    let snapshot = machine.tick();
    assert_eq!(snapshot.state, OwnershipState::ReturningLocal);
}

#[test]
fn remote_disconnect_returns_local() {
    let mut machine = OwnershipMachine::default();
    let remote = DeviceId("test-remote".into());
    machine.apply(OwnershipEvent::EdgeCrossed(ScreenEdge::Left));
    machine.apply(OwnershipEvent::RemoteAccepted(remote.clone()));
    assert_eq!(machine.snapshot().state, OwnershipState::RemoteActive);
    machine.apply(OwnershipEvent::RemoteDisconnected(remote.clone()));
    assert_eq!(machine.snapshot().state, OwnershipState::LocalActive);
    assert_eq!(machine.snapshot().active_remote, None);
}

#[test]
fn suspend_and_resume() {
    let mut machine = OwnershipMachine::default();
    let remote = DeviceId("test-remote".into());
    machine.apply(OwnershipEvent::EdgeCrossed(ScreenEdge::Top));
    machine.apply(OwnershipEvent::RemoteAccepted(remote.clone()));
    machine.apply(OwnershipEvent::Suspend);
    assert_eq!(machine.snapshot().state, OwnershipState::Suspended);
    machine.apply(OwnershipEvent::Resume);
    assert_eq!(machine.snapshot().state, OwnershipState::RemoteActive);
}

#[test]
fn invalid_transitions_are_ignored() {
    let mut machine = OwnershipMachine::default();
    let remote = DeviceId("test-remote".into());
    machine.apply(OwnershipEvent::RemoteAccepted(remote));
    assert_eq!(machine.snapshot().state, OwnershipState::LocalActive);
    machine.apply(OwnershipEvent::ReturnLocal);
    assert_eq!(machine.snapshot().state, OwnershipState::LocalActive);
}
