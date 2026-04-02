import { useState } from "react";
import { WelcomeStep } from "./welcome-step";
import { DeviceNameStep } from "./device-name-step";
import { RoleSelectStep } from "./role-select-step";
import { PermissionsStep } from "./permissions-step";
import { DaemonStep } from "./daemon-step";
import { CompleteStep } from "./complete-step";

export interface OnboardingData {
  deviceName: string;
  role: "controller" | "target";
  permissionsGranted: boolean;
  daemonRunning: boolean;
}

const STEPS = [WelcomeStep, DeviceNameStep, RoleSelectStep, PermissionsStep, DaemonStep, CompleteStep];

export function OnboardingWizard(props: { onComplete: (data: OnboardingData) => void }) {
  const [step, setStep] = useState(0);
  const [data, setData] = useState<OnboardingData>({
    deviceName: "",
    role: "controller",
    permissionsGranted: false,
    daemonRunning: false,
  });

  function handleNext() {
    if (step < STEPS.length - 1) setStep(step + 1);
    else props.onComplete(data);
  }

  function handleBack() {
    if (step > 0) setStep(step - 1);
  }

  function handleUpdate(partial: Partial<OnboardingData>) {
    setData({ ...data, ...partial });
  }

  const StepComponent = STEPS[step];

  return (
    <div className="ib-onboarding">
      <div className="ib-onboarding__progress">
        {STEPS.map((_, i) => (
          <div key={i} className={`ib-onboarding__dot ${i <= step ? "ib-onboarding__dot--active" : ""}`} />
        ))}
      </div>
      <div className="ib-onboarding__content">
        <StepComponent data={data} onUpdate={handleUpdate} />
      </div>
      <div className="ib-onboarding__actions">
        {step > 0 ? <button className="ib-button ib-button--neutral" onClick={handleBack}>Back</button> : null}
        <button className="ib-button ib-button--accent" onClick={handleNext}>
          {step === STEPS.length - 1 ? "Get Started" : "Next"}
        </button>
      </div>
    </div>
  );
}
