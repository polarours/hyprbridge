# Peripheral Configuration

HyprBridge has two complementary jobs:

1. **Share** a keyboard/mouse across machines (the original KVM feature).
2. **Configure** the peripherals themselves — DPI, report rate, button actions,
   macros and per-application profiles. This document describes the second.

Both live in the same workspace and can run at the same time. Sharing decides
*where* input goes; peripheral configuration decides *how the device behaves*.

## Why a userspace subsystem

- Kernel `hid-logitech-hidpp` exposes devices as standard input nodes but has no
  DPI/button configuration interface. Those settings use vendor HID++ commands,
  which are best issued from userspace over `hidraw`.
- On Wayland/Hyprland, `xinput` and `xbindkeys` do nothing. Remapping has to be
  done at the `evdev` -> `uinput` layer or through compositor IPC.
- Brands do not share a configuration protocol. A pluggable backend per family
  is more honest than one pretending abstraction.

## Current scope

`crates/peripherals` currently provides:

- dependency-free discovery from `/proc/bus/input/devices` and `/sys`
- strict TOML config loading and validation (invalid config is refused)
- capability reporting that reflects what hardware actually supports
- profile resolution by focused window class + action dispatch (pure logic)
- `doctor` checks for `/dev/uinput`, receivers, devices and config
- CLI commands and a daemon read-only API

Not implemented yet: HID++ writes, the evdev/uinput remap loop, daemon-owned
profile switching. These are tracked in `docs/roadmap.md`; the CLI reports them
as unimplemented rather than claiming success.

## CLI

```bash
cargo run -p hyprbridge-cli -- peripheral list
cargo run -p hyprbridge-cli -- peripheral doctor
cargo run -p hyprbridge-cli -- peripheral check
cargo run -p hyprbridge-cli -- peripheral plan --device "Logitech G304" --window-class Firefox
cargo run -p hyprbridge-cli -- peripheral list --json
```

Unlike other subcommands, these do not require a running daemon: discovery and
validation are local and unprivileged.

## Daemon API

`GET /v1/peripherals` returns the same snapshot: devices, Lightspeed receivers,
config path, and diagnostic checks. This lets the web/desktop surfaces show
peripheral state alongside sharing state.

## Configuration

Copy `config/peripherals.example.toml` to
`~/.config/hyprbridge/peripherals.toml`, or set `XDG_CONFIG_HOME`. Set
`PFORGE_CONFIG`-style overrides with `XDG_CONFIG_HOME` only; there is no other
environment override.

### Logitech G304 / G305 caveat (measured)

| Fact | Value |
| --- | --- |
| Mouse input node | `046d:4074` (wireless PID) |
| Lightspeed receiver | `046d:c53f` |
| Onboard profiles | **1**, up to 5 DPI steps |
| HID++ features | DPI `0x2201`, report rate `0x8060`, battery `0x1000`, LED `0x8070`, profiles `0x8100` |
| `REPROG CONTROLS` (`0x1B04`) | **absent** — buttons cannot be remapped in firmware |
| Kernel requirement | >= 5.4 |

So: match by `name`, expect only one profile, and implement button actions in
userspace. Do not try to write button maps over HID++ on this device.

An Endurance mode enabled in G HUB can lock report rate at 125 Hz; Linux tooling
cannot undo that without returning to Windows.

## Direct HID++ access (DPI and report rate)

Implemented against Solaar's HID++ 2.0 feature definitions: feature discovery
through `FEATURE_SET`, `ADJUSTABLE DPI` (`0x2201`) read/write functions `0x20`/`0x30`,
and `REPORT RATE` (`0x8060`) read/write functions `0x10`/`0x20`. Logitech encodes
report rate as a period of 1..8 ms (1000 Hz down to 125 Hz).

```bash
cargo run -p hyprbridge-cli -- hidpp info                 # read-only
cargo run -p hyprbridge-cli -- hidpp dpi --set 1600
cargo run -p hyprbridge-cli -- hidpp rate --set 1000
cargo run -p hyprbridge-cli -- --json hidpp info
```

The request path is unit-tested with a scripted transport (report layout,
feature-index resolution, DPI/report-rate encoding, error codes), and read
operations have been verified against a real G304. Writing to real hardware
additionally needs raw HID access:

```bash
sudo cp udev/99-hyprbridge-peripherals.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
# replug the receiver, then:
cargo run -p hyprbridge-cli -- peripheral doctor   # receiver-hidraw must be ok
```

A Lightspeed receiver exposes several hidraw interfaces; only some accept
long reports (`hidraw0`/`hidraw1` failed with `EPIPE` here, `hidraw2`/`hidraw3`
worked). The CLI probes all of them and uses the first that answers.

The `GROUP`/`MODE` lines are a deliberate fallback: `uaccess` alone did not
create an ACL on this machine because no active logind seat was available, even
though the rule matched (`TAGS=:uaccess:`). The `input` group is deterministic.

Without that rule the CLI fails with `Permission denied (os error 13)` — this
is expected, not a protocol failure. Solaar's rule is the reference: raw HID
access can also write firmware, so install it only on your own machine.

Measured on the G304 in this workspace: DPI is at index `0x1A` (active 2000,
sensor default 800) and REPORT RATE at index `0x09` (supported mask `0x8B` =
1/2/4/8 ms, current 1 ms = 1000 Hz). `hidpp dpi --set` works. `hidpp rate
--set` is **rejected by the device with `0x02`** even with Solaar's exact
encoding; this is a device-side restriction, not a framing bug.

Caveats: an Endurance mode set in G HUB can lock the report rate at 125 Hz, and
onboard profiles may override host-side DPI changes. Neither can be repaired
from Linux; `hidpp info` reports what the device actually says.

## Coexistence rules

- `ratbagd`/`piper` may own the G304's onboard DPI and report rate. Do not have
  two tools write the same setting.
- `makima`, `input-remapper` and `keyd` grab evdev nodes. Only one grabber per
  device — disable them before running an HyprBridge remap session.
- `solaar` can read battery/DPI alongside; avoid concurrent DPI writes.

## Permissions

```text
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", GROUP="input", MODE="0660"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c53f", TAG+="uaccess"
GROUP="input"
MODE="0660"
```

Install under `/etc/udev/rules.d/`, reload with `udevadm control --reload-rules`
and replug the device. `peripheral doctor` reports whether `/dev/uinput` is
actually writable.
