import type { OnboardingData } from "./wizard";

export function DeviceNameStep(props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  return (
    <div className="ib-onboarding__step">
      <h2>Name Your Device</h2>
      <p>Give this device a friendly name so you can identify it later.</p>
      <input
        className="ib-onboarding__input"
        type="text"
        placeholder="My Device"
        value={props.data.deviceName}
        onChange={(e) => props.onUpdate({ deviceName: e.target.value })}
      />
    </div>
  );
}
