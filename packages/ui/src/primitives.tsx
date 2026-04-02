import type { PropsWithChildren, ReactNode } from "react";

import "./styles.css";

export function Shell(props: PropsWithChildren<{ title: string; subtitle: string; actions?: ReactNode }>) {
  return (
    <div className="ib-shell">
      <header className="ib-shell__header">
        <div>
          <p className="ib-shell__eyebrow">HyprBridge</p>
          <h1>{props.title}</h1>
          <p className="ib-shell__subtitle">{props.subtitle}</p>
        </div>
        <div className="ib-shell__actions">{props.actions}</div>
      </header>
      <main className="ib-shell__content">{props.children}</main>
    </div>
  );
}

export function Panel(props: PropsWithChildren<{ title: string; eyebrow?: string; aside?: ReactNode }>) {
  return (
    <section className="ib-panel">
      <header className="ib-panel__header">
        <div>
          {props.eyebrow ? <p className="ib-panel__eyebrow">{props.eyebrow}</p> : null}
          <h2>{props.title}</h2>
        </div>
        {props.aside}
      </header>
      {props.children}
    </section>
  );
}

export function Badge(props: { tone: "neutral" | "good" | "warn" | "bad"; children: ReactNode }) {
  return <span className={`ib-badge ib-badge--${props.tone}`}>{props.children}</span>;
}

export function Metric(props: { label: string; value: ReactNode }) {
  return (
    <div className="ib-metric">
      <span>{props.label}</span>
      <strong>{props.value}</strong>
    </div>
  );
}

export function Button(props: {
  tone?: "accent" | "neutral" | "danger";
  disabled?: boolean;
  onClick?: () => void | Promise<void>;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={`ib-button ib-button--${props.tone ?? "accent"}`}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}
