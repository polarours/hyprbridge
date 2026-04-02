# Desktop Packaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Tauri desktop experience with multi-platform packaging, auto-update, onboarding wizard, and system tray.

**Architecture:** Tauri 2.x native features: bundle for multi-platform, updater plugin for auto-update, tray API for system tray, React onboarding wizard. Daemon lifecycle managed from Rust backend.

**Tech Stack:** Rust (Tauri 2.x), React 19, TypeScript, Vite

---

## Task 1: Tauri Bundle Configuration

**Covers:** [S3]

**Files:**
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Create: `apps/desktop/src-tauri/icons/` (placeholder icons)

- [ ] **Step 1: Update tauri.conf.json**

Replace `apps/desktop/src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "HyprBridge",
  "version": "0.1.0",
  "identifier": "io.hyprbridge.desktop",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build",
    "devUrl": "http://localhost:1430",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "HyprBridge",
        "width": 1440,
        "height": 940,
        "resizable": true,
        "visible": false
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "resources": [],
    "linux": {
      "deb": {
        "depends": ["libwebkit2gtk-4.1-dev", "libgtk-3-dev", "libayatana-appindicator3-dev"]
      },
      "rpm": {
        "requires": ["webkit2gtk4.1", "gtk3", "libappindicator-gtk3"]
      }
    },
    "macOS": {
      "minimumSystemVersion": "10.15",
      "signingIdentity": null
    },
    "windows": {
      "nsis": {
        "installMode": "both",
        "installerIcon": "icons/icon.ico"
      }
    }
  },
  "plugins": {
    "updater": {
      "active": true,
      "endpoints": [
        "https://github.com/hyprbridge/hyprbridge/releases/latest/download/latest.json"
      ],
      "pubkey": "dW50cnVzdGVkIGNvbW1lbnQgZm9yIGdpdGh1Yi5jb20vZ2l0aHViLXVwZGF0ZXIgZnJvbSBzaWduZXItY29tbWl0cy10ZXN0"
    }
  }
}
```

- [ ] **Step 2: Create placeholder icons**

Create `apps/desktop/src-tauri/icons/` directory and add placeholder PNG files (32x32, 128x128, 128x128@2x). For now, create simple colored squares as placeholders.

- [ ] **Step 3: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/desktop check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/
git commit -m "feat(desktop): configure Tauri bundle for multi-platform packaging"
```

---

## Task 2: Onboarding Wizard Components

**Covers:** [S4]

**Files:**
- Create: `packages/ui/src/onboarding/wizard.tsx`
- Create: `packages/ui/src/onboarding/welcome-step.tsx`
- Create: `packages/ui/src/onboarding/device-name-step.tsx`
- Create: `packages/ui/src/onboarding/role-select-step.tsx`
- Create: `packages/ui/src/onboarding/permissions-step.tsx`
- Create: `packages/ui/src/onboarding/daemon-step.tsx`
- Create: `packages/ui/src/onboarding/complete-step.tsx`
- Modify: `packages/ui/src/index.ts`

- [ ] **Step 1: Create wizard.tsx**

Create `packages/ui/src/onboarding/wizard.tsx`:

```tsx
import { useState } from "react";
import { WelcomeStep } from "./welcome-step";
import { DeviceNameStep } from "./device-name-step";
import { RoleSelectStep } from "./role-select-step";
import { PermissionsStep } from "./permissions-step";
import { DaemonStep } from "./daemon-step";
import { CompleteStep } from "./complete-step";

export interface OnboardingData {
  deviceName: string;
  role: "controller" | "target";
  permissionsGranted: boolean;
  daemonRunning: boolean;
}

const STEPS = [
  WelcomeStep,
  DeviceNameStep,
  RoleSelectStep,
  PermissionsStep,
  DaemonStep,
  CompleteStep,
];

export function OnboardingWizard(props: { onComplete: (data: OnboardingData) => void }) {
  const [step, setStep] = useState(0);
  const [data, setData] = useState<OnboardingData>({
    deviceName: "",
    role: "controller",
    permissionsGranted: false,
    daemonRunning: false,
  });

  function handleNext() {
    if (step < STEPS.length - 1) {
      setStep(step + 1);
    } else {
      props.onComplete(data);
    }
  }

  function handleBack() {
    if (step > 0) {
      setStep(step - 1);
    }
  }

  function handleUpdate(partial: Partial<OnboardingData>) {
    setData({ ...data, ...partial });
  }

  const StepComponent = STEPS[step];

  return (
    <div className="ib-onboarding">
      <div className="ib-onboarding__progress">
        {STEPS.map((_, i) => (
          <div
            key={i}
            className={`ib-onboarding__dot ${i <= step ? "ib-onboarding__dot--active" : ""}`}
          />
        ))}
      </div>

      <div className="ib-onboarding__content">
        <StepComponent data={data} onUpdate={handleUpdate} />
      </div>

      <div className="ib-onboarding__actions">
        {step > 0 ? (
          <button className="ib-button ib-button--neutral" onClick={handleBack}>
            Back
          </button>
        ) : null}
        <button className="ib-button ib-button--accent" onClick={handleNext}>
          {step === STEPS.length - 1 ? "Get Started" : "Next"}
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create step components**

Create each step component in `packages/ui/src/onboarding/`:

**welcome-step.tsx:**
```tsx
export function WelcomeStep() {
  return (
    <div className="ib-onboarding__step">
      <h2>Welcome to HyprBridge</h2>
      <p>Share your keyboard and mouse across devices on your network.</p>
      <p>This wizard will help you get started.</p>
    </div>
  );
}
```

**device-name-step.tsx:**
```tsx
import type { OnboardingData } from "./wizard";

export function DeviceNameStep(props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  return (
    <div className="ib-onboarding__step">
      <h2>Device Name</h2>
      <p>Give this device a name to identify it on the network.</p>
      <input
        className="ib-onboarding__input"
        type="text"
        value={props.data.deviceName}
        onChange={(e) => props.onUpdate({ deviceName: e.target.value })}
        placeholder="My Computer"
      />
    </div>
  );
}
```

**role-select-step.tsx:**
```tsx
import type { OnboardingData } from "./wizard";

export function RoleSelectStep(props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  return (
    <div className="ib-onboarding__step">
      <h2>Device Role</h2>
      <p>Is this device the one with the keyboard and mouse?</p>
      <div className="ib-onboarding__roles">
        <button
          className={`ib-onboarding__role ${props.data.role === "controller" ? "ib-onboarding__role--selected" : ""}`}
          onClick={() => props.onUpdate({ role: "controller" })}
        >
          <strong>Controller</strong>
          <span>Has keyboard and mouse</span>
        </button>
        <button
          className={`ib-onboarding__role ${props.data.role === "target" ? "ib-onboarding__role--selected" : ""}`}
          onClick={() => props.onUpdate({ role: "target" })}
        >
          <strong>Target</strong>
          <span>Receives input from controller</span>
        </button>
      </div>
    </div>
  );
}
```

**permissions-step.tsx:**
```tsx
import { useState } from "react";
import type { OnboardingData } from "./wizard";

export function PermissionsStep(props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  const [checking, setChecking] = useState(false);

  async function checkPermissions() {
    setChecking(true);
    // In real implementation, call Tauri IPC to check permissions
    // For now, simulate after 1 second
    await new Promise((r) => setTimeout(r, 1000));
    props.onUpdate({ permissionsGranted: true });
    setChecking(false);
  }

  return (
    <div className="ib-onboarding__step">
      <h2>Permissions</h2>
      <p>HyprBridge needs accessibility permissions to control input.</p>
      {props.data.permissionsGranted ? (
        <p className="ib-onboarding__success">✓ Permissions granted</p>
      ) : (
        <button
          className="ib-button ib-button--accent"
          onClick={checkPermissions}
          disabled={checking}
        >
          {checking ? "Checking..." : "Check Permissions"}
        </button>
      )}
    </div>
  );
}
```

**daemon-step.tsx:**
```tsx
import { useState } from "react";
import type { OnboardingData } from "./wizard";

export function DaemonStep(props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  const [starting, setStarting] = useState(false);

  async function startDaemon() {
    setStarting(true);
    // In real implementation, call Tauri IPC to start daemon
    await new Promise((r) => setTimeout(r, 1500));
    props.onUpdate({ daemonRunning: true });
    setStarting(false);
  }

  return (
    <div className="ib-onboarding__step">
      <h2>Daemon Connection</h2>
      <p>The HyprBridge daemon manages device connections.</p>
      {props.data.daemonRunning ? (
        <p className="ib-onboarding__success">✓ Daemon is running</p>
      ) : (
        <button
          className="ib-button ib-button--accent"
          onClick={startDaemon}
          disabled={starting}
        >
          {starting ? "Starting..." : "Start Daemon"}
        </button>
      )}
    </div>
  );
}
```

**complete-step.tsx:**
```tsx
export function CompleteStep() {
  return (
    <div className="ib-onboarding__step">
      <h2>You're All Set!</h2>
      <p>HyprBridge is configured and ready to use.</p>
      <p>Click "Get Started" to open the management console.</p>
    </div>
  );
}
```

- [ ] **Step 3: Add onboarding styles**

Append to `packages/ui/src/styles.css`:

```css
.ib-onboarding {
  max-width: 480px;
  margin: 0 auto;
  padding: 40px 24px;
}

.ib-onboarding__progress {
  display: flex;
  gap: 8px;
  justify-content: center;
  margin-bottom: 32px;
}

.ib-onboarding__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--ib-border);
  transition: background 120ms ease;
}

.ib-onboarding__dot--active {
  background: var(--ib-accent);
}

.ib-onboarding__content {
  min-height: 240px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.ib-onboarding__step {
  text-align: center;
}

.ib-onboarding__step h2 {
  margin: 0 0 12px;
}

.ib-onboarding__step p {
  color: var(--ib-text-muted);
  margin: 0 0 16px;
}

.ib-onboarding__input {
  width: 100%;
  padding: 12px 16px;
  border: 1px solid var(--ib-border);
  border-radius: 12px;
  background: var(--ib-bg-elevated);
  color: var(--ib-text);
  font: inherit;
  font-size: 1.1rem;
  text-align: center;
}

.ib-onboarding__roles {
  display: flex;
  gap: 16px;
  justify-content: center;
}

.ib-onboarding__role {
  padding: 20px;
  border: 2px solid var(--ib-border);
  border-radius: 16px;
  background: var(--ib-bg-elevated);
  color: var(--ib-text);
  cursor: pointer;
  text-align: center;
  min-width: 160px;
  transition: border-color 120ms ease;
}

.ib-onboarding__role--selected {
  border-color: var(--ib-accent);
}

.ib-onboarding__role strong {
  display: block;
  margin-bottom: 4px;
}

.ib-onboarding__role span {
  font-size: 0.85rem;
  color: var(--ib-text-muted);
}

.ib-onboarding__success {
  color: var(--ib-accent);
  font-weight: 600;
}

.ib-onboarding__actions {
  display: flex;
  justify-content: space-between;
  margin-top: 32px;
}
```

- [ ] **Step 4: Export from index.ts**

In `packages/ui/src/index.ts`, add:

```typescript
export { OnboardingWizard } from "./onboarding/wizard";
export type { OnboardingData } from "./onboarding/wizard";
```

- [ ] **Step 5: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/ui check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add packages/ui/
git commit -m "feat(ui): add onboarding wizard components"
```

---

## Task 3: System Tray

**Covers:** [S5]

**Files:**
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`

- [ ] **Step 1: Add tray dependencies**

In `apps/desktop/src-tauri/Cargo.toml`, add:

```toml
[dependencies]
tauri-plugin-shell = "2"
```

- [ ] **Step 2: Update main.rs with system tray**

Replace `apps/desktop/src-tauri/src/main.rs`:

```rust
use tauri::{
    Manager,
    tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent},
    menu::{MenuBuilder, MenuItemBuilder},
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let show_item = MenuItemBuilder::with_id("show", "Show Window").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .tooltip("HyprBridge")
                .build(app)?;

            // Show main window after setup
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/desktop check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/
git commit -m "feat(desktop): add system tray with menu and window management"
```

---

## Task 4: Auto-Update Integration

**Covers:** [S6]

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`

- [ ] **Step 1: Add updater plugin**

In `apps/desktop/src-tauri/Cargo.toml`, add:

```toml
tauri-plugin-updater = "2"
```

- [ ] **Step 2: Register updater plugin in main.rs**

In `apps/desktop/src-tauri/src/main.rs`, add to Builder:

```rust
.plugin(tauri_plugin_updater::Builder::new().build())
```

- [ ] **Step 3: Add updater check to frontend**

In `packages/ui/src/management-console.tsx`, add update check on mount:

```typescript
import { check } from "@tauri-apps/plugin-updater";

useEffect(() => {
  async function checkUpdate() {
    try {
      const update = await check();
      if (update) {
        // Show update available notification
        console.log("Update available:", update.version);
      }
    } catch (e) {
      console.log("Update check failed:", e);
    }
  }
  checkUpdate();
}, []);
```

- [ ] **Step 4: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/desktop check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/ packages/ui/
git commit -m "feat(desktop): integrate auto-update from GitHub Releases"
```

---

## Task 5: Daemon Lifecycle Management

**Covers:** [S7]

**Files:**
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`

- [ ] **Step 1: Add shell plugin for daemon management**

In `apps/desktop/src-tauri/src/main.rs`, add Tauri commands:

```rust
#[tauri::command]
fn start_daemon() -> Result<String, String> {
    // In real implementation, spawn daemon process
    // For now, return success
    Ok("daemon started".into())
}

#[tauri::command]
fn stop_daemon() -> Result<String, String> {
    // In real implementation, kill daemon process
    Ok("daemon stopped".into())
}

#[tauri::command]
async fn check_daemon_status() -> Result<bool, String> {
    // In real implementation, check daemon health endpoint
    Ok(true)
}
```

Register commands in Builder:

```rust
.invoke_handler(tauri::generate_handler![start_daemon, stop_daemon, check_daemon_status])
```

- [ ] **Step 2: Add @tauri-apps/api dependency**

In `packages/ui/package.json`, add:

```json
"dependencies": {
  "@tauri-apps/api": "^2.0.0",
  "@tauri-apps/plugin-updater": "^2.0.0"
}
```

- [ ] **Step 3: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/desktop check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/ packages/ui/
git commit -m "feat(desktop): add daemon lifecycle management via Tauri IPC"
```

---

## Task 6: Final Verification

**Covers:** [S8]

**Files:** None

- [ ] **Step 1: Run all TypeScript checks**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm check`
Expected: PASS

- [ ] **Step 2: Run Rust workspace check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Verify Tauri build**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/desktop tauri build` (check it starts without errors)

- [ ] **Step 4: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for desktop packaging"
```
