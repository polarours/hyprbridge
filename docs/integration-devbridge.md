# Integrating DevBridge into HyprBridge

> **Status:** the user did not use DevBridge. Only the read-only introspection
> and desktop-capability parts were ported (Phases 1 and 2). The DevBridge
> project directory has been removed after archiving its complete git history
> to `~/Projects/Personal/_archives/DevBridge.git.bundle` and tool configs to
> `~/Projects/Personal/_archives/DevBridge-tool-configs.tar.gz`. Restore with
> `git clone ~/Projects/Personal/_archives/DevBridge.git.bundle` if needed.

## What each project actually is

| | HyprBridge (this repo) | DevBridge (`~/Projects/Personal/DevBridge`) |
| --- | --- | --- |
| Crates | 20 | 39 |
| Daemon port | 24800 | 41741 |
| Headline feature | Cross-machine keyboard/mouse sharing + local peripheral configuration | Remote development: Desktop Mode and Workspace Mode |
| Surfaces | CLI, Axum API, Tauri/React scaffolds | CLI, Axum API, Tauri/React apps |
| Maturity | active development | single initial commit, mostly scaffold |

## Overlap

Both projects independently implement platform probes (`platform-linux`,
`platform-wayland`, `platform-x11`), a daemon with a localhost API, a CLI,
Web/Desktop shells, network/transport plumbing, and diagnostics. A literal
repository merge would produce 59 crates, two daemons, two control UIs, two
pairing models and conflicting ports, while duplicating the platform layer.
That is a large amount of churn with no new capability.

## Complementarity

- HyprBridge owns input routing (where keystrokes and pointer motion go) and
  peripheral configuration (DPI, report rate, button actions, profiles).
- DevBridge owns remote-workspace concepts: terminal sessions, files, logs,
  ports, services, and capability introspection for remote machines.

The useful combination is: **one HyprBridge daemon and one CLI, with selected
DevBridge capabilities ported in as HyprBridge subsystems**. DevBridge stays
untouched as a reference and as an independent product.

## Phased plan

### Phase 1 — read-only workspace introspection (implemented)

Add `crates/workspace` to this workspace with types owned by HyprBridge. Port
the *idea* of DevBridge's `introspection::inspect_local` and
`desktop-session::evaluate_desktop`, not its crate or types:

- `GET /v1/workspace/info`: OS/session/compositor, capabilities, tool availability
- `GET /v1/workspace/files?path=...`: sandboxed directory listing
- `hyprbridge workspace info|files` CLI commands

No cross-repo path dependency, no second daemon, no configuration changes.
This delivers the integration surface without touching existing behaviour.

Implemented in `crates/workspace` (GPL-3.0, same licence as DevBridge): detection
logic from DevBridge's `introspection`/`workspace` crates was ported with
HyprBridge-owned types. The daemon exposes `/v1/workspace/info`,
`/v1/workspace/files` and `/v1/workspace/file`; the CLI exposes
`hyprbridge workspace info|files|file`. File access is sandboxed to `$HOME`,
with a 1 MiB read limit; attempts to escape the root return HTTP 400.

Verified on this machine: the CLI reports `session: wayland`, `desktop:
Hyprland`, `backend: wayland-hyprland (partial)`, the files endpoint lists
`$HOME/Projects/Personal`, and `path=/etc` is rejected.

### Phase 2 — desktop-mode capability reporting (in progress)

Port DevBridge's `DesktopCapability` evaluation into the existing HyprBridge
diagnostics: a `desktop-mode` doctor check now reports the detected backend
(`wayland-hyprland` here) and its caveats. This directly strengthens the
Hyprland-first story: it explains what remote desktop control can and cannot do
on X11 versus Wayland, using the same honesty principle already applied to
input capture and injection.

### Phase 3 — remote workspace operations

Only if needed: terminal sessions, log tailing, port forwarding and service
introspection. Each needs explicit sandboxing and an opt-in flag; this is the
highest-risk phase and should follow Phases 1 and 2.

## Alternative: CLI delegation (lowest code cost)

Keep both repositories completely separate and let `hyprbridge workspace ...`
invoke the `devbridge` CLI over its local API. No duplicated code, but it
requires both binaries and both daemons to be installed and running, and the
user experience is split across two configuration models.

## Decision rule

Prefer porting small, read-only capabilities into HyprBridge's existing daemon
and CLI. Do not merge repositories or add a path dependency on DevBridge
crates: its domain model and daemon lifecycle are different, and it is still
at a single initial commit.
