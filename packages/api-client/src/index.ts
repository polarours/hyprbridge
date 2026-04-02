export type DeviceRole = "controller" | "target";
export type LinuxSessionKind = "x11" | "wayland" | "unknown";
export type WaylandSupportLevel = "unsupported" | "partial" | "supported";
export type SupportLevel = "supported" | "unsupported" | "planned";
export type PermissionState = "granted" | "missing" | "unknown";
export type OwnershipState =
  | "local_active"
  | "crossing_edge"
  | "remote_active"
  | "returning_local"
  | "suspended";
export type ScreenEdge = "left" | "right" | "top" | "bottom";
export type SessionDirection = "inbound" | "outbound";
export type SessionState = "connected" | "rejected" | "failed" | "closed";

export interface DeviceIdentity {
  device_id: string;
  name: string;
  role: DeviceRole;
  public_key: string;
}

export interface CapabilityReport {
  platform: string;
  linux_session: LinuxSessionKind;
  wayland_support: WaylandSupportLevel;
  input_capture: SupportLevel;
  input_injection: SupportLevel;
  cursor_handoff: SupportLevel;
  permission_state: PermissionState;
  notes: string[];
}

export interface TransportProfile {
  service_type: string;
  quic_addr: string;
  certificate_sha256: string;
  certificate_der_b64: string;
}

export interface TransportSession {
  peer: DeviceIdentity | null;
  direction: SessionDirection;
  state: SessionState;
  remote_addr: string;
  updated_at: string;
  detail: string;
}

export interface TransportSnapshot {
  profile: TransportProfile;
  sessions: TransportSession[];
}

export interface EdgeMapping {
  local_edge: ScreenEdge;
  remote_device: string;
  remote_edge: ScreenEdge;
}

export interface TopologySnapshot {
  edges: EdgeMapping[];
  dead_zone_px: number;
}

export interface PointerSnapshot {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface InputBackendSnapshot {
  backend: string;
  available: boolean;
  pointer: PointerSnapshot | null;
  edge_candidate: ScreenEdge | null;
  controller_capture: ControllerCaptureSnapshot | null;
  target_injection: TargetInjectionSnapshot | null;
  target_wheel_injection: SupportLevel;
  detail: string;
}

export interface ControllerCaptureSnapshot {
  readable_devices: number;
  grabbed_devices: number;
  exclusive_capture_active: boolean;
}

export interface TargetInjectionSnapshot {
  primary_backend: string;
  fallback_backend: string | null;
  uinput_writable: boolean;
  last_error: string | null;
}

export interface OwnershipSnapshot {
  state: OwnershipState;
  active_remote: string | null;
}

export interface ReturnLocalResponse {
  ownership: OwnershipSnapshot;
}

export interface ServiceStatus {
  identity: DeviceIdentity;
  capability: CapabilityReport;
  trusted_peers: number;
  discovered_peers: number;
  ownership: OwnershipSnapshot;
  topology: TopologySnapshot;
  input_backend: InputBackendSnapshot | null;
  transport: TransportSnapshot;
  observed_at: string;
}

export interface DiscoveredDevice {
  identity: DeviceIdentity;
  endpoint: string;
  transport: TransportProfile;
  capability: CapabilityReport;
  pairable: boolean;
}

export interface LogEntry {
  at: string;
  level: string;
  target: string;
  message: string;
}

export interface DoctorCheck {
  id: string;
  title: string;
  status: "ok" | "warning" | "error";
  detail: string;
}

export interface DoctorReport {
  checks: DoctorCheck[];
}

export type EventEnvelope =
  | { type: "status"; payload: ServiceStatus }
  | { type: "discovery"; payload: DiscoveredDevice[] }
  | { type: "pairing"; payload: unknown }
  | { type: "log"; payload: LogEntry };

export class HyprBridgeClient {
  constructor(private readonly baseUrl = "http://127.0.0.1:24800") {}

  status(): Promise<ServiceStatus> {
    return this.get("/v1/status");
  }

  discovery(): Promise<DiscoveredDevice[]> {
    return this.get("/v1/discovery");
  }

  diagnostics(): Promise<DoctorReport> {
    return this.get("/v1/diagnostics");
  }

  transport(): Promise<TransportSnapshot> {
    return this.get("/v1/transport");
  }

  logs(): Promise<LogEntry[]> {
    return this.get("/v1/logs");
  }

  returnLocal(): Promise<ReturnLocalResponse> {
    return this.post("/v1/ownership/return");
  }

  getTopology(): Promise<TopologySnapshot> {
    return this.get("/v1/topology");
  }

  updateTopology(topology: TopologySnapshot): Promise<{ topology: TopologySnapshot }> {
    return this.put("/v1/topology", { topology });
  }

  eventsUrl(): string {
    const url = new URL(this.baseUrl);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    url.pathname = "/v1/events";
    url.search = "";
    url.hash = "";
    return url.toString();
  }

  private async get<T>(path: string): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`);
    if (!response.ok) {
      throw new Error(`HyprBridge API request failed: ${response.status}`);
    }

    return response.json() as Promise<T>;
  }

  private async post<T>(path: string): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, { method: "POST" });
    if (!response.ok) {
      throw new Error(`HyprBridge API request failed: ${response.status}`);
    }

    return response.json() as Promise<T>;
  }

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
}
