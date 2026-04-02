import { useState } from "react";
import type { OnboardingData } from "./wizard";

export function DaemonStep(props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  const [starting, setStarting] = useState(false);

  function handleStart() {
    setStarting(true);
    setTimeout(() => {
      props.onUpdate({ daemonRunning: true });
      setStarting(false);
    }, 1500);
  }

  return (
    <div className="ib-onboarding__step">
      <h2>Background Service</h2>
      <p>The HyprBridge daemon runs in the background to keep your connection alive.</p>
      {props.data.daemonRunning ? (
        <p className="ib-onboarding__success">Daemon is running</p>
      ) : (
        <button className="ib-button ib-button--accent" onClick={handleStart} disabled={starting}>
          {starting ? "Starting\u2026" : "Start Daemon"}
        </button>
      )}
    </div>
  );
}
