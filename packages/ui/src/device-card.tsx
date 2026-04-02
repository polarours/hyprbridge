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