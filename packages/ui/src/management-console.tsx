import { useEffect, useState } from "react";

import {
  HyprBridgeClient,
  type DiscoveredDevice,
  type DoctorReport,
  type EventEnvelope,
  type LogEntry,
  type ServiceStatus,
  type TopologySnapshot,
  type TransportSnapshot
} from "@hyprbridge/api-client";

import { check } from "@tauri-apps/plugin-updater";
import { Badge, Button, Metric, Panel, Shell } from "./primitives";
import { ClipboardHistoryPanel } from "./clipboard-history-panel";
import type { ClipboardHistoryEntry } from "./clipboard-history-panel";
import { GroupsPanel } from "./groups-panel";
import type { Group } from "./groups-panel";
import { TopologyCanvas } from "./topology-canvas";
import { TopologySettings } from "./topology-settings";

const client = new HyprBridgeClient();

export function ManagementConsole(props: { title: string; subtitle: string; shellBadge?: string }) {
  const [status, setStatus] = useState<ServiceStatus | null>(null);
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [discovery, setDiscovery] = useState<DiscoveredDevice[]>([]);
  const [transport, setTransport] = useState<TransportSnapshot | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [topology, setTopology] = useState<TopologySnapshot>({ edges: [], dead_zone_px: 12 });
  const [groups, setGroups] = useState<Group[]>([]);
  const [clipboardEntries, setClipboardEntries] = useState<ClipboardHistoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    async function checkUpdate() {
      try {
        const update = await check();
        if (update) {
          console.log("Update available:", update.version);
        }
      } catch (e) {
        console.log("Update check failed:", e);
      }
    }
    checkUpdate();
  }, []);

  useEffect(() => {
    const socket = new WebSocket(client.eventsUrl());

    socket.onmessage = (event) => {
      try {
        const message = JSON.parse(event.data as string) as EventEnvelope;
        if (message.type === "status") {
          setStatus(message.payload);
        } else if (message.type === "discovery") {
          setDiscovery(message.payload);
        } else if (message.type === "log") {
          setLogs((current) => [message.payload, ...current].slice(0, 5));
        }
      } catch {
        setError("Received an invalid daemon event payload");
      }
    };

    socket.onerror = () => {
      setError((current) => current ?? "Daemon event stream is unavailable");
    };

    return () => {
      socket.close();
    };
  }, []);

  async function refresh() {
    try {
      const [serviceStatus, doctorReport, discovered, transportSnapshot, daemonLogs, topologySnapshot] =
        await Promise.all([
          client.status(),
          client.diagnostics(),
          client.discovery(),
          client.transport(),
          client.logs(),
          client.getTopology()
        ]);
      setStatus(serviceStatus);
      setDoctor(doctorReport);
      setDiscovery(discovered);
      setTransport(transportSnapshot);
      setLogs(daemonLogs.slice(-5).reverse());
      setTopology(topologySnapshot);
      setError(null);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "Unknown API error");
    }
  }

  async function handleReturnLocal() {
    try {
      await client.returnLocal();
      await refresh();
    } catch (returnError) {
      setError(
        returnError instanceof Error
          ? returnError.message
          : "Failed to return local ownership"
      );
    }
  }

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

  function handleCreateGroup(name: string) {
    setGroups((current) => [...current, { id: crypto.randomUUID(), name, members: [] }]);
  }

  function handleDeleteGroup(id: string) {
    setGroups((current) => current.filter((g) => g.id !== id));
  }

  function handleAddMember(groupId: string, deviceId: string) {
    if (!deviceId.trim()) return;
    setGroups((current) =>
      current.map((g) =>
        g.id === groupId && !g.members.includes(deviceId.trim())
          ? { ...g, members: [...g.members, deviceId.trim()] }
          : g
      )
    );
  }

  function handleRemoveMember(groupId: string, deviceId: string) {
    setGroups((current) =>
      current.map((g) =>
        g.id === groupId ? { ...g, members: g.members.filter((m) => m !== deviceId) } : g
      )
    );
  }

  function handlePinClipboard(id: string) {
    setClipboardEntries((current) =>
      current.map((entry) => (entry.id === id ? { ...entry, pinned: true } : entry))
    );
  }

  function handleUnpinClipboard(id: string) {
    setClipboardEntries((current) =>
      current.map((entry) => (entry.id === id ? { ...entry, pinned: false } : entry))
    );
  }

  function handleDeleteClipboard(id: string) {
    setClipboardEntries((current) => current.filter((entry) => entry.id !== id));
  }

  function handleClearClipboard() {
    setClipboardEntries([]);
  }

  return (
    <Shell
      title={props.title}
      subtitle={props.subtitle}
      actions={
        <Badge tone={status ? "good" : error ? "bad" : "warn"}>
          {status ? props.shellBadge ?? "Daemon online" : error ? "API offline" : "Loading"}
        </Badge>
      }
    >
      <div className="ib-grid">
        <Panel title="Runtime Snapshot" eyebrow="Status">
          {status ? (
            <>
              <Metric label="Device" value={status.identity.name} />
              <Metric label="Role" value={status.identity.role} />
              <Metric label="Ownership" value={status.ownership.state} />
              <Metric label="Active remote" value={status.ownership.active_remote ?? "none"} />
              <Metric label="Trusted peers" value={status.trusted_peers} />
              <Metric label="Discovered peers" value={status.discovered_peers} />
              {status.ownership.state === "remote_active" ? (
                <Button tone="danger" onClick={handleReturnLocal}>
                  Return Local
                </Button>
              ) : null}
            </>
          ) : (
            <p className="ib-copy">{error ?? "Waiting for the daemon to respond."}</p>
          )}
        </Panel>

        <Panel title="Linux Capability Matrix" eyebrow="Platform">
          {status ? (
            <>
              <Metric label="Platform" value={status.capability.platform} />
              <Metric label="Session" value={status.capability.linux_session} />
              <Metric label="Capture" value={status.capability.input_capture} />
              <Metric label="Injection" value={status.capability.input_injection} />
              <Metric label="Cursor handoff" value={status.capability.cursor_handoff} />
            </>
          ) : (
            <p className="ib-copy">
              Capability data appears here once the local daemon is available.
            </p>
          )}
        </Panel>
      </div>

      <div className="ib-grid">
        <Panel title="Native Input Backend" eyebrow="Platform runtime">
          {status?.input_backend ? (
            <>
              <Metric label="Backend" value={status.input_backend.backend} />
              <Metric label="Available" value={String(status.input_backend.available)} />
              <Metric
                label="Edge candidate"
                value={status.input_backend.edge_candidate ?? "none"}
              />
              <Metric
                label="Pointer"
                value={
                  status.input_backend.pointer
                    ? `${status.input_backend.pointer.x}, ${status.input_backend.pointer.y}`
                    : "unavailable"
                }
              />
              <Metric
                label="Wheel injection"
                value={status.input_backend.target_wheel_injection}
              />
              {status.input_backend.target_injection ? (
                <>
                  <Metric
                    label="Target injection"
                    value={status.input_backend.target_injection.primary_backend}
                  />
                  <Metric
                    label="uinput writable"
                    value={String(status.input_backend.target_injection.uinput_writable)}
                  />
                </>
              ) : null}
              <p className="ib-copy">{status.input_backend.detail}</p>
              {status.input_backend.target_injection?.last_error ? (
                <p className="ib-copy">
                  Last target injection error: {status.input_backend.target_injection.last_error}
                </p>
              ) : null}
            </>
          ) : (
            <p className="ib-copy">
              No live native backend snapshot is available for this session. On Wayland this is
              expected until a compositor-specific backend exists.
            </p>
          )}
        </Panel>

        <Panel title="Controller Capture" eyebrow="Hyprland / Wayland">
          {status?.input_backend?.controller_capture ? (
            <>
              <Metric
                label="Readable evdev devices"
                value={status.input_backend.controller_capture.readable_devices}
              />
              <Metric
                label="Grabbed devices"
                value={status.input_backend.controller_capture.grabbed_devices}
              />
              <Metric
                label="Exclusive capture"
                value={
                  status.input_backend.controller_capture.exclusive_capture_active
                    ? "active"
                    : "inactive"
                }
              />
              <p className="ib-copy">
                Controller-side keyboard, button, and wheel sampling uses cached Linux evdev
                devices. HyprBridge attempts exclusive grabs only during remote ownership.
              </p>
            </>
          ) : (
            <p className="ib-copy">
              No controller capture runtime is exposed for this backend. This is expected on X11
              and on Wayland sessions without readable evdev devices.
            </p>
          )}
        </Panel>
      </div>

      <div className="ib-grid">
        <Panel title="Discovery" eyebrow="LAN">
          {discovery.length > 0 ? (
            <div className="ib-stack">
              {discovery.map((device) => (
                <div key={device.identity.device_id} className="ib-callout">
                  <div className="ib-callout__header">
                    <strong>{device.identity.name}</strong>
                    <Badge tone={device.pairable ? "good" : "warn"}>
                      {device.identity.role}
                    </Badge>
                  </div>
                  <p>{device.transport.quic_addr}</p>
                  <p className="ib-mono">
                    {device.transport.certificate_sha256.slice(0, 16)}...
                  </p>
                </div>
              ))}
            </div>
          ) : (
            <p className="ib-copy">
              No remote HyprBridge devices are currently visible on the LAN.
            </p>
          )}
        </Panel>

        <Panel title="Transport" eyebrow="QUIC">
          {transport ? (
            <>
              <Metric label="Listen address" value={transport.profile.quic_addr} />
              <Metric label="Service type" value={transport.profile.service_type} />
              <Metric label="Session records" value={transport.sessions.length} />
              <div className="ib-stack">
                {transport.sessions.slice(0, 4).map((session) => (
                  <div
                    key={`${session.remote_addr}-${session.updated_at}`}
                    className="ib-callout"
                  >
                    <div className="ib-callout__header">
                      <strong>{session.peer?.name ?? session.remote_addr}</strong>
                      <Badge
                        tone={
                          session.state === "connected"
                            ? "good"
                            : session.state === "closed"
                              ? "neutral"
                              : "bad"
                        }
                      >
                        {session.direction} {session.state}
                      </Badge>
                    </div>
                    <p>{session.detail}</p>
                  </div>
                ))}
              </div>
            </>
          ) : (
            <p className="ib-copy">
              Transport runtime details appear here once the daemon starts.
            </p>
          )}
        </Panel>
      </div>

      <div className="ib-grid">
        <Panel title="Doctor" eyebrow="Diagnostics">
          {doctor ? (
            <div className="ib-stack">
              {doctor.checks.map((check) => (
                <div key={check.id} className="ib-callout">
                  <div className="ib-callout__header">
                    <strong>{check.title}</strong>
                    <Badge
                      tone={
                        check.status === "ok"
                          ? "good"
                          : check.status === "warning"
                            ? "warn"
                            : "bad"
                      }
                    >
                      {check.status}
                    </Badge>
                  </div>
                  <p>{check.detail}</p>
                </div>
              ))}
            </div>
          ) : (
            <p className="ib-copy">
              Doctor results will explain session restrictions and missing permissions.
            </p>
          )}
        </Panel>

        <Panel title="Recent Logs" eyebrow="Observability">
          {logs.length > 0 ? (
            <div className="ib-stack">
              {logs.map((log) => (
                <div key={`${log.at}-${log.message}`} className="ib-log">
                  <span>{new Date(log.at).toLocaleTimeString()}</span>
                  <strong>{log.target}</strong>
                  <p>{log.message}</p>
                </div>
              ))}
            </div>
          ) : (
            <p className="ib-copy">
              Structured daemon logs are streamed here for quick diagnostics.
            </p>
          )}
        </Panel>
      </div>

      <GroupsPanel
        groups={groups}
        onCreateGroup={handleCreateGroup}
        onDeleteGroup={handleDeleteGroup}
        onAddMember={handleAddMember}
        onRemoveMember={handleRemoveMember}
      />

      <ClipboardHistoryPanel
        entries={clipboardEntries}
        onPin={handlePinClipboard}
        onUnpin={handleUnpinClipboard}
        onDelete={handleDeleteClipboard}
        onClear={handleClearClipboard}
      />

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
    </Shell>
  );
}
