# Roadmap

## Near Term

- Harden discovery and trust persistence
- Implement richer remote-session orchestration
- Expand topology editing UX
- Fill in native platform backends

## DevBridge Integration

- Ship read-only workspace introspection (done: `crates/workspace`)
- Add desktop-mode capability reporting to the shared capability report
- Evaluate remote workspace operations with explicit sandboxing and opt-in flags

## Peripheral Configuration

- Ship the evdev -> uinput remap engine with per-window profile switching (done: `crates/remap`)
- Verify the grab/inject loop with physical button presses on the G304

- Ship the discovery/config/profile engine (in progress)
- Implement the evdev -> uinput remap loop with per-window profile switching
- Implement HID++ DPI/report-rate writes, starting with G304/G305
- Integrate peripheral state into the web/desktop surfaces via `/v1/peripherals`

## Later

- Clipboard sync
- File handoff
- Relay mode beyond LAN
- Team device groups
