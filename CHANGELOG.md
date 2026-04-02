# Changelog

All notable changes to HyprBridge are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-19

### Added

- Cross-machine keyboard and mouse sharing with a daemon, local control API,
  CLI, and desktop/web scaffolds. X11 uses the native XTEST path; Wayland is
  Hyprland-first with compositor IPC inspection and `uinput` injection.
- Peripheral configuration subsystem: dependency-free device discovery, strict
  TOML validation, capability reporting, profile resolution by focused window
  class, and `doctor` diagnostics.
- Logitech HID++ 2.0 transport: feature discovery through ROOT, DPI read/write,
  report-rate read, error-code handling, and receiver-interface probing.
  Real G304 readings: DPI 2000 (default 800) at index `0x1A`, report rate
  1000 Hz at index `0x09`. The G304 rejects report-rate writes with `0x02`.
- evdev to uinput remapping engine: key, macro, exec, DPI, profile, pass and
  disable actions; unbound events are passed through; per-application profiles
  switch on the Hyprland active window class; raw event viewer for discovering
  button codes, including onboard-mapped buttons such as `Ctrl+C` / `Ctrl+V`.
- Workspace introspection and sandboxed read-only file access ported from
  DevBridge, including host/session detection and desktop-mode capability
  assessment in the doctor report.
- udev rule for `uinput` and the G304/G305 Lightspeed receiver, with an
  `input`-group fallback when no active logind seat is available.

### Changed

- Project renamed from InputBridge to HyprBridge; all crates, commands,
  configuration paths and documentation use the new name.
- Git history rewritten to a single `init` commit. The previous history is
  preserved in a local bundle under `~/Projects/Personal/_archives/`.
- Remote changed from the old InputBridge repository to the new HyprBridge
  repository.

### Notes

- DevBridge was archived and removed after its read-only introspection and
  desktop-capability behaviour were ported into HyprBridge.
- The grab-and-inject remap path has not yet been exercised with a physical
  button press; all other listed behaviour was verified on the local machine.
