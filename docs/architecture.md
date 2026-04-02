# Architecture

HyprBridge is a daemon-centered system.

## Runtime Model

- The daemon owns discovery, trust, topology, ownership state, diagnostics, and local control APIs.
- The daemon also owns the QUIC transport endpoint and session registry.
- The daemon exposes live runtime snapshots over HTTP and WebSocket so every control surface sees the same ownership, discovery, transport, and native backend state.
- The CLI, desktop app, and web UI are control surfaces over the daemon.
- Platform crates implement capability detection and eventually native input capture/injection behind stable traits.

## Domain Boundaries

- `core`: topology graph, ownership state machine, device/domain events
- `protocol`: typed messages and API DTOs
- `network`: discovery, identity, pairing, and transport setup
- `network`: mDNS advertisement/browse, persisted QUIC identity, and trusted session bootstrap
- `input`: stable platform abstraction traits
- `diagnostics`: capability and doctor reporting
- `daemon`: orchestration and API host
- `platform-linux`: session routing, `evdev` controller capture, and Linux runtime snapshot assembly
- `platform-x11`: X11 capture and XTEST-based target injection
- `platform-wayland`: Hyprland-first Wayland inspection and target injection with `/dev/uinput` preference and compositor fallback

## Key Decisions

- QUIC is the preferred transport for authenticated encrypted peer sessions.
- Pairing uses explicit one-time codes and local approval.
- Linux X11 and Wayland support remain separate all the way down to capability reporting and backend implementation.
- Linux controller capture and target injection are modeled as separate runtime concerns so diagnostics can explain exactly which half of a session is missing.
- Hyprland support is implemented as a real runtime backend, not a generic Wayland placeholder: pointer inspection comes from compositor IPC, controller capture comes from readable `evdev` devices, and target injection prefers `/dev/uinput` while preserving Hyprland-specific fallback behavior.
