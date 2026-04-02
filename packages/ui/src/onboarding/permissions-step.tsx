import { useState } from "react";
import type { OnboardingData } from "./wizard";

export function PermissionsStep(props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  const [checking, setChecking] = useState(false);

  function handleCheck() {
    setChecking(true);
    setTimeout(() => {
      props.onUpdate({ permissionsGranted: true });
      setChecking(false);
    }, 1200);
  }

  return (
    <div className="ib-onboarding__step">
      <h2>Permissions</h2>
      <p>HyprBridge needs accessibility permissions to read and send input events.</p>
      {props.data.permissionsGranted ? (
        <p className="ib-onboarding__success">Permissions granted</p>
      ) : (
        <button className="ib-button ib-button--accent" onClick={handleCheck} disabled={checking}>
          {checking ? "Checking\u2026" : "Grant Permissions"}
        </button>
      )}
    </div>
  );
}
