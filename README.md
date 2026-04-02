# HyprBridge

HyprBridge is a modern cross-platform keyboard and mouse sharing tool for Windows, macOS, and Linux.

It is built as a daemon-first Rust monorepo with a polished CLI, a Tauri desktop shell, and a React-based management UI. The initial repository focuses on a strong open-source foundation: typed domain models, secure pairing primitives, explicit Linux X11 vs Wayland capability reporting, a local daemon control API, and end-to-end developer scaffolding.

## Why It Exists

HyprBridge aims to feel like a premium software KVM for modern local networks:

- Cursor ownership and keyboard focus are modeled as a deterministic state machine.
- Devices discover each other over the LAN and pair with explicit trust.
- Platform differences are surfaced honestly, especially on Linux Wayland.
- CLI, desktop, and web surfaces all sit on top of the same daemon runtime.

## Repository Layout

- `crates/core`: topology and ownership domain model
- `crates/protocol`: typed transport and control API contracts
- `crates/peripherals`: local device configuration (DPI, buttons, profiles, capability reports)
- `crates/workspace`: read-only host introspection and sandboxed file access (ported from DevBridge)
- `crates/remap`: evdev -> uinput button remapping, macros and per-app profiles
- `crates/network`: discovery, identity, pairing, and transport configuration
- `crates/daemon`: daemon runtime and local control API
- `crates/cli`: CLI for discovery, pairing, topology, diagnostics, and logs
- `crates/platform-*`: OS-specific capability and integration layers
- `apps/web`: React management UI
- `apps/desktop`: Tauri desktop shell
- `packages/ui`: shared UI components and tokens
- `docs/`: architecture, protocol, platform, security, and roadmap docs

## Current MVP Status

This repository includes:

- A structured Rust workspace
- A daemon with a localhost management API
- A CLI-first control surface
- Topology and ownership logic with tests
- Trust, identity, LAN discovery, and pairing flows
- Real mDNS advertisement and browse runtime for local-network presence
- Real QUIC session bootstrap with persisted self-signed transport certificates
- Linux X11 pointer handoff, remote pointer motion, button injection, and key injection
- Linux X11 controller-side relay loop for active remote sessions
- Hyprland-first Wayland pointer inspection, edge detection, cached `evdev`-backed controller-side key/button/wheel sampling, and target injection that prefers `uinput` with Hyprland IPC/dispatcher fallback
- Linux session and capability diagnostics
- Peripheral configuration subsystem: discovery, strict config validation, profile engine, capability reporting, CLI and daemon API
- Web and desktop application scaffolds
- Documentation and GitHub project hygiene

This repository does not yet claim universal native input capture/injection parity across all operating systems. Windows, macOS, and Wayland are modeled as first-class targets with explicit capability reporting and extension points, but the runtime remains honest about incomplete native backend coverage. The most complete end-to-end control path today is Linux X11.

## CLI Examples

```bash
cargo run -p hyprbridge-cli -- serve
cargo run -p hyprbridge-cli -- status
cargo run -p hyprbridge-cli -- discover
cargo run -p hyprbridge-cli -- transport
cargo run -p hyprbridge-cli -- doctor
cargo run -p hyprbridge-cli -- return-local
cargo run -p hyprbridge-cli -- topology sample --json
cargo run -p hyprbridge-cli -- peripheral list
cargo run -p hyprbridge-cli -- peripheral doctor
cargo run -p hyprbridge-cli -- peripheral plan --device "Logitech G304"
cargo run -p hyprbridge-cli -- hidpp info
cargo run -p hyprbridge-cli -- workspace info
cargo run -p hyprbridge-cli -- workspace files --path Projects/Personal
cargo run -p hyprbridge-cli -- remap events --device "Logitech G304"
cargo run -p hyprbridge-cli -- remap start --device "Logitech G304" --dry-run
```

## Platform Notes

- Linux:
  - Detects X11 vs Wayland at runtime
  - Reports compositor/session capabilities and limitations
  - Keeps X11 and Wayland logic in separate crates
  - Ships the current MVP control path on X11, including pointer enter, relative motion relay, and XTEST-backed button/key injection
  - Ships a Hyprland-first Wayland backend for pointer inspection, edge detection, cached `evdev`-backed controller capture, and target injection that prefers `/dev/uinput` when writable
  - Requires readable `/dev/input/event*` devices for full Hyprland controller-side keyboard and mouse-button relay
  - During active remote ownership on Wayland, the controller attempts `evdev` grabs to reduce local side effects
  - Wheel relay is implemented in the controller pipeline; Hyprland target-side wheel injection is available when `/dev/uinput` is writable
  - `status`, `doctor`, and the web/desktop management surfaces expose live controller-capture and target-injection runtime state for Hyprland sessions
  - Linux controller capture excludes HyprBridge's own virtual `uinput` device to avoid feedback loops during relay
- macOS:
  - Models accessibility/input-monitoring permission requirements
- Windows:
  - Models native desktop-session requirements and injection constraints

See [docs/platform-support.md](docs/platform-support.md) and [docs/linux-x11-wayland.md](docs/linux-x11-wayland.md).

Peripheral configuration (DPI, button actions, per-application profiles) is documented in [docs/peripherals.md](docs/peripherals.md).

## Security Model

- Each device has a persistent identity
- Pairing uses a one-time code plus explicit local approval flow
- Trusted peers are pinned in the local trust store
- Control traffic is designed around authenticated encrypted sessions

See [docs/security.md](docs/security.md) and [docs/protocol.md](docs/protocol.md).

## Screenshots

Screenshot placeholders live in `docs/assets/` and should be replaced once the first runnable desktop/web flows are captured.

## Local Development

### Rust

```bash
cargo fmt --all
cargo test --workspace
cargo run -p hyprbridge-cli -- serve
```

### Frontend

```bash
pnpm install
pnpm --filter @hyprbridge/web dev
pnpm --filter @hyprbridge/desktop tauri dev
```

## Roadmap

The near-term roadmap focuses on:

- hardening the X11 control path with better local suppression and return-local ergonomics
- building out native input backends beyond Linux X11
- evolving the topology editor into a fully interactive canvas
- improving desktop packaging and onboarding
- implementing the evdev/uinput remap engine for peripheral configuration (HID++ DPI/report-rate writer is now in place)

See [docs/roadmap.md](docs/roadmap.md).

## Contributing

Contribution guidelines live in [CONTRIBUTING.md](CONTRIBUTING.md). The project uses GPLv3, typed interfaces, and an explicitly documented architecture to keep the codebase contributor-friendly from the start.
