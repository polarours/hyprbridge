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