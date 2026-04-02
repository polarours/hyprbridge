# Desktop Packaging Design

## [S1] Problem

HyprBridge has a Tauri 2.x skeleton but no actual packaging, auto-update, first-run wizard, or system tray. Users must run from source. This prevents adoption by non-technical users.

## [S2] Solution Overview

Complete Tauri desktop experience:
- Multi-platform packaging (Linux .deb/.rpm, macOS .dmg, Windows .msi)
- Auto-update from GitHub Releases
- First-run onboarding wizard (device name, role, permissions, daemon connection)
- System tray integration (minimize, status, quick actions)
- Daemon lifecycle management from Tauri backend

## [S3] Tauri Bundle Configuration

Update `apps/desktop/src-tauri/tauri.conf.json`:

```json
{
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.icns", "icons/icon.ico"],
    "linux": { "deb": { "depends": ["libwebkit2gtk-4.1-dev"] } },
    "macOS": { "minimumSystemVersion": "10.15" },
    "windows": { "nsis": { "installMode": "both" } }
  },
  "plugins": {
    "updater": {
      "active": true,
      "endpoints": ["https://github.com/hyprbridge/hyprbridge/releases/latest/download/latest.json"],
      "pubkey": "..."
    }
  }
}
```

**Files:** `apps/desktop/src-tauri/tauri.conf.json`

## [S4] First-Run Onboarding Wizard

New React components in `packages/ui/src/onboarding/`:

**Steps:**
1. Welcome page
2. Device name input
3. Role selection (Controller/Target)
4. Permission check (Accessibility/Input Monitoring)
5. Daemon connection (start or connect)
6. Completion → enter main UI

**Data persistence:** Tauri Store API (`~/.config/hyprbridge/config.json`)

**Files:** `packages/ui/src/onboarding/wizard.tsx`, `packages/ui/src/onboarding/*-step.tsx`

## [S5] System Tray Integration

Use Tauri's `tray` API:

**Features:**
- Minimize to tray on window close
- Tray icon shows daemon status (online/offline)
- Right-click menu: Show window, Topology editor, Relay connect/disconnect, Logs, Quit

**Rust implementation:** `TrayIconBuilder` with event handlers

**Files:** `apps/desktop/src-tauri/src/main.rs`

## [S6] Auto-Update

Use Tauri's `updater` plugin:
- Check GitHub Releases for `latest.json`
- Download and verify signature
- Prompt user to install
- Config: pubkey for ed25519 signature verification

**Files:** `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/Cargo.toml`

## [S7] Daemon Management

Tauri backend manages daemon process:
- Start/stop daemon via `std::process::Command`
- Monitor daemon status via health endpoint
- Sync status to system tray icon
- Frontend calls Tauri IPC → Rust backend → daemon

**Files:** `apps/desktop/src-tauri/src/main.rs`

## [S8] Testing Strategy

- Build packages for each platform (CI/CD)
- Test auto-update flow with local test server
- Test onboarding wizard end-to-end
- Test system tray interactions
- Test daemon lifecycle (start/stop/restart)

## [S9] Non-Goals

- Code signing (requires Apple/Windows developer accounts)
- Store distribution (Mac App Store, Microsoft Store)
- Custom installer themes
- Portable/no-install mode
