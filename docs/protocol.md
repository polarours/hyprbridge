# Protocol

## Control API

The local daemon exposes a localhost-only HTTP API with typed JSON responses. A WebSocket event stream carries live state snapshots and ownership updates.

The current control API includes status, discovery, transport, topology, pairing, connection handoff, and explicit return-local ownership endpoints.

`GET /v1/status` now returns a live `input_backend` snapshot when a native backend is active. That snapshot carries:

- pointer coordinates and current edge candidate
- controller-side capture runtime state
- target-side injection runtime state
- target wheel injection support level

For Linux Wayland and Hyprland sessions this is how the CLI, doctor command, web UI, and desktop UI surface whether controller `evdev` capture is readable, whether exclusive grabs are active, and whether target injection is running through `/dev/uinput` or compositor-specific fallback paths.

## Peer Protocol

Peer discovery and sessions are modeled around:

- mDNS presence advertisements
- one-time-code pairing handshake
- pinned device identities
- authenticated encrypted control channels

The current repository implements:

- mDNS service advertisement with typed TXT metadata
- QUIC endpoint bootstrap with persisted self-signed transport certificates
- session handshake records exposed through the daemon control API
- trusted control streams carrying typed `ControlFrame` messages
- Linux X11 remote pointer entry, relative pointer motion, wheel events, button events, and key events over those streams
- Hyprland Wayland pointer inspection via compositor IPC, controller capture via readable `evdev` devices, and target injection that prefers a virtual `/dev/uinput` device before falling back to Hyprland IPC and dispatcher paths
- best-effort remote input release and `return_local` signaling when the controller restores local ownership

The current `ControlFrame` set includes:

- `pointer_enter`
- `pointer_move`
- `pointer_scroll`
- `pointer_button`
- `keyboard_key`
- `return_local`

Transport authorization is currently enforced against the local trust store at session acceptance time. Native input forwarding now exists for Linux X11 and for Hyprland sessions that satisfy the required `/dev/input` and `/dev/uinput` access constraints, while other platforms remain capability-first until their native backends are implemented.
