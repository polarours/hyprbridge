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
    props.onUpdate({ ...props.topology, edges: newEdges });
    setDialogEdge(null);
  }

  function handleDelete() {
    if (!dialogEdge) return;
    const newEdges = props.topology.edges.filter(
      (e) => e.local_edge !== dialogEdge
    );
    props.onUpdate({ ...props.topology, edges: newEdges });
    setDialogEdge(null);
  }

  function handleDrop(e: React.DragEvent, targetEdge: ScreenEdge) {
    e.preventDefault();
    const sourceEdge = e.dataTransfer.getData("text/plain") as ScreenEdge;
    if (sourceEdge && sourceEdge !== targetEdge) {
      const sourceMapping = getMappingForEdge(sourceEdge);
      const targetMapping = getMappingForEdge(targetEdge);
      const newEdges = props.topology.edges.filter(
        (ed) => ed.local_edge !== sourceEdge && ed.local_edge !== targetEdge
      );
      if (targetMapping) newEdges.push({ ...targetMapping, local_edge: sourceEdge });
      if (sourceMapping) newEdges.push({ ...sourceMapping, local_edge: targetEdge });
      props.onUpdate({ ...props.topology, edges: newEdges });
    }
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
