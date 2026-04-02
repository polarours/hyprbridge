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
