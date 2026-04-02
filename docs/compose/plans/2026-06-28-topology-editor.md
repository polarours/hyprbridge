# Topology Editor UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an interactive topology editor UI for visualizing and editing device edge mappings.

**Architecture:** Canvas-based visualization with device cards and edge connectors. Click-to-select and drag-to-connect interactions. Real-time sync with daemon via REST API. Dark theme matching existing UI.

**Tech Stack:** React 19, TypeScript, CSS (existing design system), Vite

---

## Task 1: API Client Extensions

**Covers:** [S6]

**Files:**
- Modify: `packages/api-client/src/index.ts`

- [ ] **Step 1: Add topology API methods to HyprBridgeClient**

In `packages/api-client/src/index.ts`, add to the `HyprBridgeClient` class:

```typescript
getTopology(): Promise<TopologySnapshot> {
  return this.get("/v1/topology");
}

updateTopology(topology: TopologySnapshot): Promise<{ topology: TopologySnapshot }> {
  return this.put("/v1/topology", { topology });
}
```

Also add the `put` method:

```typescript
private async put<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${this.baseUrl}${path}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(`HyprBridge API request failed: ${response.status}`);
  }
  return response.json() as Promise<T>;
}
```

- [ ] **Step 2: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/api-client check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add packages/api-client/
git commit -m "feat(api-client): add topology get/update methods"
```

---

## Task 2: DeviceCard Component

**Covers:** [S4]

**Files:**
- Create: `packages/ui/src/device-card.tsx`

- [ ] **Step 1: Create DeviceCard component**

Create `packages/ui/src/device-card.tsx`:

```tsx
import type { ReactNode } from "react";
import { Badge } from "./primitives";

export interface DeviceCardProps {
  name: string;
  role: "controller" | "target";
  isLocal: boolean;
  isConnected: boolean;
  children?: ReactNode;
}

export function DeviceCard(props: DeviceCardProps) {
  return (
    <div
      className={`ib-device-card ${props.isLocal ? "ib-device-card--local" : ""}`}
    >
      <div className="ib-device-card__header">
        <strong>{props.name}</strong>
        <Badge tone={props.isConnected ? "good" : "neutral"}>
          {props.role}
        </Badge>
      </div>
      {props.children}
    </div>
  );
}
```

- [ ] **Step 2: Add DeviceCard styles**

In `packages/ui/src/styles.css`, add:

```css
.ib-device-card {
  padding: 16px;
  border: 1px solid var(--ib-border);
  border-radius: 16px;
  background: var(--ib-bg-elevated);
  min-width: 200px;
  transition: border-color 120ms ease;
}

.ib-device-card--local {
  border-color: var(--ib-accent);
  min-width: 280px;
}

.ib-device-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}
```

- [ ] **Step 3: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/ui check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add packages/ui/
git commit -m "feat(ui): add DeviceCard component"
```

---

## Task 3: EdgeConnector Component

**Covers:** [S4]

**Files:**
- Create: `packages/ui/src/edge-connector.tsx`
- Modify: `packages/ui/src/styles.css`

- [ ] **Step 1: Create EdgeConnector component**

Create `packages/ui/src/edge-connector.tsx`:

```tsx
import type { ScreenEdge } from "@hyprbridge/api-client";

export interface EdgeConnectorProps {
  edge: ScreenEdge;
  isMapped: boolean;
  remoteDeviceName?: string;
  onClick: (edge: ScreenEdge) => void;
  onDragStart?: (edge: ScreenEdge) => void;
}

const EDGE_LABELS: Record<ScreenEdge, string> = {
  left: "Left",
  right: "Right",
  top: "Top",
  bottom: "Bottom",
};

const EDGE_POSITIONS: Record<ScreenEdge, React.CSSProperties> = {
  left: { left: -12, top: "50%", transform: "translateY(-50%)" },
  right: { right: -12, top: "50%", transform: "translateY(-50%)" },
  top: { top: -12, left: "50%", transform: "translateX(-50%)" },
  bottom: { bottom: -12, left: "50%", transform: "translateX(-50%)" },
};

export function EdgeConnector(props: EdgeConnectorProps) {
  return (
    <button
      type="button"
      className={`ib-edge-connector ${props.isMapped ? "ib-edge-connector--mapped" : ""}`}
      style={EDGE_POSITIONS[props.edge]}
      onClick={() => props.onClick(props.edge)}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData("text/plain", props.edge);
        props.onDragStart?.(props.edge);
      }}
      title={props.isMapped ? `${EDGE_LABELS[props.edge]} → ${props.remoteDeviceName}` : `Map ${EDGE_LABELS[props.edge]} edge`}
    >
      {props.isMapped ? "●" : "+"}
    </button>
  );
}
```

- [ ] **Step 2: Add EdgeConnector styles**

In `packages/ui/src/styles.css`, add:

```css
.ib-edge-connector {
  position: absolute;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  border: 2px solid var(--ib-border);
  background: var(--ib-bg);
  color: var(--ib-text-muted);
  font-size: 14px;
  font-weight: 700;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 120ms ease;
  z-index: 10;
}

.ib-edge-connector:hover {
  border-color: var(--ib-accent);
  color: var(--ib-accent);
  transform: translateY(-50%) scale(1.1);
}

.ib-edge-connector--mapped {
  border-color: var(--ib-accent);
  background: var(--ib-accent);
  color: #041b13;
}
```

- [ ] **Step 3: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/ui check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add packages/ui/
git commit -m "feat(ui): add EdgeConnector component"
```

---

## Task 4: EdgeMappingDialog Component

**Covers:** [S4]

**Files:**
- Create: `packages/ui/src/edge-mapping-dialog.tsx`
- Modify: `packages/ui/src/styles.css`

- [ ] **Step 1: Create EdgeMappingDialog component**

Create `packages/ui/src/edge-mapping-dialog.tsx`:

```tsx
import { useState } from "react";
import type { ScreenEdge, DiscoveredDevice } from "@hyprbridge/api-client";
import { Button } from "./primitives";

export interface EdgeMappingDialogProps {
  localEdge: ScreenEdge;
  devices: DiscoveredDevice[];
  existingRemoteDevice?: string;
  existingRemoteEdge?: ScreenEdge;
  onConfirm: (remoteDeviceId: string, remoteEdge: ScreenEdge) => void;
  onDelete?: () => void;
  onCancel: () => void;
}

const EDGES: ScreenEdge[] = ["left", "right", "top", "bottom"];

export function EdgeMappingDialog(props: EdgeMappingDialogProps) {
  const [selectedDevice, setSelectedDevice] = useState(
    props.existingRemoteDevice ?? ""
  );
  const [selectedEdge, setSelectedEdge] = useState<ScreenEdge>(
    props.existingRemoteEdge ?? "left"
  );

  return (
    <div className="ib-dialog-overlay" onClick={props.onCancel}>
      <div className="ib-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>Map {props.localEdge} edge</h3>

        <label className="ib-dialog__label">
          Remote device
          <select
            className="ib-dialog__select"
            value={selectedDevice}
            onChange={(e) => setSelectedDevice(e.target.value)}
          >
            <option value="">Select a device...</option>
            {props.devices.map((d) => (
              <option key={d.identity.device_id} value={d.identity.device_id}>
                {d.identity.name} ({d.identity.role})
              </option>
            ))}
          </select>
        </label>

        <label className="ib-dialog__label">
          Remote edge
          <select
            className="ib-dialog__select"
            value={selectedEdge}
            onChange={(e) => setSelectedEdge(e.target.value as ScreenEdge)}
          >
            {EDGES.map((edge) => (
              <option key={edge} value={edge}>
                {edge}
              </option>
            ))}
          </select>
        </label>

        <div className="ib-dialog__actions">
          {props.existingRemoteDevice && props.onDelete ? (
            <Button tone="danger" onClick={props.onDelete}>
              Remove
            </Button>
          ) : null}
          <Button tone="neutral" onClick={props.onCancel}>
            Cancel
          </Button>
          <Button
            tone="accent"
            disabled={!selectedDevice}
            onClick={() => props.onConfirm(selectedDevice, selectedEdge)}
          >
            Confirm
          </Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Add dialog styles**

In `packages/ui/src/styles.css`, add:

```css
.ib-dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.ib-dialog {
  padding: 24px;
  border: 1px solid var(--ib-border);
  border-radius: 16px;
  background: var(--ib-bg);
  min-width: 320px;
  box-shadow: var(--ib-shadow);
}

.ib-dialog h3 {
  margin: 0 0 16px;
}

.ib-dialog__label {
  display: block;
  margin-bottom: 12px;
  color: var(--ib-text-muted);
  font-size: 0.85rem;
}

.ib-dialog__select {
  display: block;
  width: 100%;
  margin-top: 6px;
  padding: 8px 12px;
  border: 1px solid var(--ib-border);
  border-radius: 8px;
  background: var(--ib-bg-elevated);
  color: var(--ib-text);
  font: inherit;
}

.ib-dialog__actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  margin-top: 20px;
}
```

- [ ] **Step 3: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/ui check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add packages/ui/
git commit -m "feat(ui): add EdgeMappingDialog component"
```

---

## Task 5: TopologyCanvas Component

**Covers:** [S3, S7]

**Files:**
- Create: `packages/ui/src/topology-canvas.tsx`
- Modify: `packages/ui/src/styles.css`
- Modify: `packages/ui/src/index.ts`

- [ ] **Step 1: Create TopologyCanvas component**

Create `packages/ui/src/topology-canvas.tsx`:

```tsx
import { useState } from "react";
import type {
  TopologySnapshot,
  EdgeMapping,
  ScreenEdge,
  DiscoveredDevice,
} from "@hyprbridge/api-client";
import { DeviceCard } from "./device-card";
import { EdgeConnector } from "./edge-connector";
import { EdgeMappingDialog } from "./edge-mapping-dialog";

export interface TopologyCanvasProps {
  topology: TopologySnapshot;
  localDeviceName: string;
  discoveredDevices: DiscoveredDevice[];
  onUpdate: (topology: TopologySnapshot) => void;
}

const EDGES: ScreenEdge[] = ["left", "right", "top", "bottom"];

export function TopologyCanvas(props: TopologyCanvasProps) {
  const [dialogEdge, setDialogEdge] = useState<ScreenEdge | null>(null);
  const [dragEdge, setDragEdge] = useState<ScreenEdge | null>(null);

  function getMappingForEdge(edge: ScreenEdge): EdgeMapping | undefined {
    return props.topology.edges.find((e) => e.local_edge === edge);
  }

  function getDeviceName(deviceId: string): string {
    const device = props.discoveredDevices.find(
      (d) => d.identity.device_id === deviceId
    );
    return device?.identity.name ?? deviceId.slice(0, 8);
  }

  function handleEdgeClick(edge: ScreenEdge) {
    setDialogEdge(edge);
  }

  function handleConfirm(remoteDeviceId: string, remoteEdge: ScreenEdge) {
    if (!dialogEdge) return;

    const newEdges = props.topology.edges.filter(
      (e) => e.local_edge !== dialogEdge
    );
    newEdges.push({
      local_edge: dialogEdge,
      remote_device: remoteDeviceId,
      remote_edge: remoteEdge,
    });

    props.onUpdate({
      ...props.topology,
      edges: newEdges,
    });
    setDialogEdge(null);
  }

  function handleDelete() {
    if (!dialogEdge) return;

    const newEdges = props.topology.edges.filter(
      (e) => e.local_edge !== dialogEdge
    );
    props.onUpdate({
      ...props.topology,
      edges: newEdges,
    });
    setDialogEdge(null);
  }

  function handleDrop(e: React.DragEvent, targetEdge: ScreenEdge) {
    e.preventDefault();
    const sourceEdge = e.dataTransfer.getData("text/plain") as ScreenEdge;
    if (sourceEdge && sourceEdge !== targetEdge) {
      // Swap mappings
      const sourceMapping = getMappingForEdge(sourceEdge);
      const targetMapping = getMappingForEdge(targetEdge);

      const newEdges = props.topology.edges.filter(
        (e) => e.local_edge !== sourceEdge && e.local_edge !== targetEdge
      );

      if (targetMapping) {
        newEdges.push({ ...targetMapping, local_edge: sourceEdge });
      }
      if (sourceMapping) {
        newEdges.push({ ...sourceMapping, local_edge: targetEdge });
      }

      props.onUpdate({ ...props.topology, edges: newEdges });
    }
    setDragEdge(null);
  }

  const dialogMapping = dialogEdge ? getMappingForEdge(dialogEdge) : undefined;

  return (
    <div className="ib-topology-canvas">
      <div className="ib-topology-canvas__local">
        <DeviceCard
          name={props.localDeviceName}
          role="controller"
          isLocal={true}
          isConnected={true}
        >
          <div className="ib-topology-canvas__edges">
            {EDGES.map((edge) => {
              const mapping = getMappingForEdge(edge);
              return (
                <div
                  key={edge}
                  className={`ib-topology-canvas__edge-zone ib-topology-canvas__edge-zone--${edge}`}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => handleDrop(e, edge)}
                >
                  <EdgeConnector
                    edge={edge}
                    isMapped={!!mapping}
                    remoteDeviceName={mapping ? getDeviceName(mapping.remote_device) : undefined}
                    onClick={handleEdgeClick}
                    onDragStart={setDragEdge}
                  />
                  {mapping ? (
                    <span className="ib-topology-canvas__edge-label">
                      → {getDeviceName(mapping.remote_device)} ({mapping.remote_edge})
                    </span>
                  ) : null}
                </div>
              );
            })}
          </div>
        </DeviceCard>
      </div>

      {dialogEdge ? (
        <EdgeMappingDialog
          localEdge={dialogEdge}
          devices={props.discoveredDevices}
          existingRemoteDevice={dialogMapping?.remote_device}
          existingRemoteEdge={dialogMapping?.remote_edge}
          onConfirm={handleConfirm}
          onDelete={dialogMapping ? handleDelete : undefined}
          onCancel={() => setDialogEdge(null)}
        />
      ) : null}
    </div>
  );
}
```

- [ ] **Step 2: Add TopologyCanvas styles**

In `packages/ui/src/styles.css`, add:

```css
.ib-topology-canvas {
  position: relative;
  padding: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 400px;
}

.ib-topology-canvas__local {
  position: relative;
}

.ib-topology-canvas__edges {
  position: relative;
  height: 20px;
}

.ib-topology-canvas__edge-zone {
  position: absolute;
  display: flex;
  align-items: center;
  gap: 8px;
}

.ib-topology-canvas__edge-zone--left {
  left: -40px;
  top: 50%;
  transform: translateY(-50%);
  flex-direction: row-reverse;
}

.ib-topology-canvas__edge-zone--right {
  right: -40px;
  top: 50%;
  transform: translateY(-50%);
}

.ib-topology-canvas__edge-zone--top {
  top: -40px;
  left: 50%;
  transform: translateX(-50%);
  flex-direction: column;
}

.ib-topology-canvas__edge-zone--bottom {
  bottom: -40px;
  left: 50%;
  transform: translateX(-50%);
  flex-direction: column;
}

.ib-topology-canvas__edge-label {
  font-size: 0.75rem;
  color: var(--ib-text-muted);
  white-space: nowrap;
}
```

- [ ] **Step 3: Export from index.ts**

In `packages/ui/src/index.ts`, add:

```typescript
export { TopologyCanvas } from "./topology-canvas";
export { DeviceCard } from "./device-card";
export { EdgeConnector } from "./edge-connector";
export { EdgeMappingDialog } from "./edge-mapping-dialog";
```

- [ ] **Step 4: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/ui check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add packages/ui/
git commit -m "feat(ui): add TopologyCanvas component"
```

---

## Task 6: TopologySettings Component

**Covers:** [S5]

**Files:**
- Create: `packages/ui/src/topology-settings.tsx`
- Modify: `packages/ui/src/styles.css`

- [ ] **Step 1: Create TopologySettings component**

Create `packages/ui/src/topology-settings.tsx`:

```tsx
import type { TopologySnapshot, ScreenEdge } from "@hyprbridge/api-client";
import { Button, Panel } from "./primitives";

export interface TopologySettingsProps {
  topology: TopologySnapshot;
  onUpdate: (topology: TopologySnapshot) => void;
  onReset: () => void;
}

export function TopologySettings(props: TopologySettingsProps) {
  function handleDeadZoneChange(value: number) {
    props.onUpdate({ ...props.topology, dead_zone_px: value });
  }

  function handleRemoveEdge(edge: ScreenEdge) {
    const newEdges = props.topology.edges.filter((e) => e.local_edge !== edge);
    props.onUpdate({ ...props.topology, edges: newEdges });
  }

  return (
    <Panel title="Topology Settings" eyebrow="Configuration">
      <div className="ib-topology-settings">
        <label className="ib-topology-settings__label">
          Dead zone: {props.topology.dead_zone_px}px
          <input
            type="range"
            min={0}
            max={50}
            value={props.topology.dead_zone_px}
            onChange={(e) => handleDeadZoneChange(Number(e.target.value))}
            className="ib-topology-settings__slider"
          />
        </label>

        {props.topology.edges.length > 0 ? (
          <div className="ib-topology-settings__mappings">
            <p className="ib-copy">Current mappings:</p>
            {props.topology.edges.map((edge) => (
              <div key={edge.local_edge} className="ib-topology-settings__mapping">
                <span>
                  {edge.local_edge} → {edge.remote_device.slice(0, 8)} ({edge.remote_edge})
                </span>
                <Button
                  tone="neutral"
                  onClick={() => handleRemoveEdge(edge.local_edge)}
                >
                  ×
                </Button>
              </div>
            ))}
          </div>
        ) : (
          <p className="ib-copy">No edge mappings configured.</p>
        )}

        <Button tone="danger" onClick={props.onReset}>
          Reset Topology
        </Button>
      </div>
    </Panel>
  );
}
```

- [ ] **Step 2: Add settings styles**

In `packages/ui/src/styles.css`, add:

```css
.ib-topology-settings__label {
  display: block;
  margin-bottom: 16px;
  color: var(--ib-text-muted);
  font-size: 0.85rem;
}

.ib-topology-settings__slider {
  display: block;
  width: 100%;
  margin-top: 8px;
  accent-color: var(--ib-accent);
}

.ib-topology-settings__mappings {
  margin-bottom: 16px;
}

.ib-topology-settings__mapping {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 0;
  border-bottom: 1px solid rgba(139, 161, 199, 0.12);
  font-size: 0.85rem;
}
```

- [ ] **Step 3: Export from index.ts**

In `packages/ui/src/index.ts`, add:

```typescript
export { TopologySettings } from "./topology-settings";
```

- [ ] **Step 4: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/ui check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add packages/ui/
git commit -m "feat(ui): add TopologySettings component"
```

---

## Task 7: Integrate into ManagementConsole

**Covers:** [S7]

**Files:**
- Modify: `packages/ui/src/management-console.tsx`

- [ ] **Step 1: Add topology state and API calls**

In `packages/ui/src/management-console.tsx`, add imports:

```typescript
import { TopologyCanvas } from "./topology-canvas";
import { TopologySettings } from "./topology-settings";
```

Add state:

```typescript
const [topology, setTopology] = useState<TopologySnapshot>({ edges: [], dead_zone_px: 12 });
```

Update `refresh()` to load topology:

```typescript
const topologySnapshot = await client.getTopology();
setTopology(topologySnapshot);
```

Add update handler:

```typescript
async function handleTopologyUpdate(newTopology: TopologySnapshot) {
  try {
    const response = await client.updateTopology(newTopology);
    setTopology(response.topology);
  } catch (updateError) {
    setError(
      updateError instanceof Error ? updateError.message : "Failed to update topology"
    );
  }
}

async function handleTopologyReset() {
  const empty: TopologySnapshot = { edges: [], dead_zone_px: 12 };
  await handleTopologyUpdate(empty);
}
```

- [ ] **Step 2: Add topology section to the UI**

In the JSX, add a new section after the existing panels:

```tsx
<div className="ib-grid">
  <Panel title="Topology Editor" eyebrow="Layout">
    <TopologyCanvas
      topology={topology}
      localDeviceName={status?.identity.name ?? "Local"}
      discoveredDevices={discovery}
      onUpdate={handleTopologyUpdate}
    />
  </Panel>

  <TopologySettings
    topology={topology}
    onUpdate={handleTopologyUpdate}
    onReset={handleTopologyReset}
  />
</div>
```

- [ ] **Step 3: Run check**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/ui check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add packages/ui/
git commit -m "feat(ui): integrate topology editor into management console"
```

---

## Task 8: Final Verification

**Covers:** [S8]

**Files:** None

- [ ] **Step 1: Run all TypeScript checks**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm check`
Expected: PASS

- [ ] **Step 2: Run Rust workspace check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Verify web dev server starts**

Run: `cd /home/polarours/Projects/Personal/HyprBridge && pnpm --filter @hyprbridge/web dev` (check it starts without errors)

- [ ] **Step 4: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification for topology editor"
```
