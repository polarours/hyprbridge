# Topology Editor UI Design

## [S1] Problem

HyprBridge has no visual topology editor. Users must configure device edge mappings via CLI or raw JSON. This is a core UX gap that prevents non-technical users from setting up the tool.

## [S2] Solution Overview

Interactive topology editor in the web/desktop UI:
- Canvas-based visualization with device cards and edge connectors
- Click-to-select and drag-to-connect for creating mappings
- Real-time sync with daemon via REST API and WebSocket
- Dead zone configuration slider
- Dark theme matching existing UI guidelines

## [S3] TopologyCanvas Component

Main canvas component displaying device layout:

**Layout:**
- Local device: large rectangle (screen aspect ratio), centered
- Remote devices: smaller rectangles, positioned around local device based on active mappings
- Mapped edges: colored connecting lines
- Unmapped edges: dashed/gray indicators

**Interaction:**
- Click edge → popup menu to select remote device + remote edge
- Drag from edge to remote device → auto-create mapping
- Click existing line → delete mapping
- Dead zone slider → adjust pixel threshold

**Data flow:**
```
Daemon GET /v1/topology → TopologySnapshot → render canvas
User edit → PUT /v1/topology → update topology
```

**Files:** `packages/ui/src/topology-canvas.tsx`

## [S4] DeviceCard and EdgeConnector

**DeviceCard:**
- Shows device name, role (Controller/Target), connection status
- Local device: special style (larger, highlighted border)
- Remote devices: standard style

**EdgeConnector:**
- Four clickable edge zones on local device
- Hover highlight
- Mapped: show remote device name + colored line
- Unmapped: show "+" button or dashed line

**EdgeMappingDialog:**
- Appears on click or drag-release
- Dropdown: select remote device (from discovered peers)
- Dropdown: select remote edge (Left/Right/Top/Bottom)
- Confirm/Cancel buttons

**Files:** `packages/ui/src/device-card.tsx`, `packages/ui/src/edge-connector.tsx`, `packages/ui/src/edge-mapping-dialog.tsx`

## [S5] TopologySettings Panel

- Dead zone slider (0-50px, default 12px)
- Current mappings list (individual delete)
- "Reset Topology" button

**Files:** `packages/ui/src/topology-settings.tsx`

## [S6] API Integration

- `GET /v1/topology` → load current topology
- `PUT /v1/topology` → save topology changes
- `GET /v1/discovery` → get available devices (for dropdown)
- WebSocket events → real-time device online status

**Files:** `packages/api-client/src/index.ts` (add `getTopology()`, `updateTopology()`)

## [S7] Integration into ManagementConsole

Add topology editor as a new tab/section in the existing management console. Replace the current static topology display with the interactive canvas.

**Files:** `packages/ui/src/management-console.tsx`

## [S8] Testing Strategy

- Visual verification: start web dev server, verify canvas renders
- Interaction testing: create/delete mappings via UI
- API integration: verify topology saves and loads correctly
- Cross-browser: test in Chrome, Firefox, Safari

## [S9] Non-Goals

- Multi-monitor topology (v2)
- Animated transitions (v2)
- Undo/redo (v2)
- Import/export topology files (v2)
