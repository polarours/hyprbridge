import type { OnboardingData } from "./wizard";

export function RoleSelectStep(props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  return (
    <div className="ib-onboarding__step">
      <h2>Choose a Role</h2>
      <p>How will this device participate in HyprBridge?</p>
      <div className="ib-onboarding__roles">
        <button
          className={`ib-onboarding__role ${props.data.role === "controller" ? "ib-onboarding__role--selected" : ""}`}
          onClick={() => props.onUpdate({ role: "controller" })}
        >
          <strong>Controller</strong>
          <span>Send input to other devices</span>
        </button>
        <button
          className={`ib-onboarding__role ${props.data.role === "target" ? "ib-onboarding__role--selected" : ""}`}
          onClick={() => props.onUpdate({ role: "target" })}
        >
          <strong>Target</strong>
          <span>Receive input from controllers</span>
        </button>
      </div>
    </div>
  );
}
