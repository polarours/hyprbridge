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
