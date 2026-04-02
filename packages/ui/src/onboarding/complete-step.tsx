import type { OnboardingData } from "./wizard";

export function CompleteStep(_props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  return (
    <div className="ib-onboarding__step">
      <h2>You're All Set</h2>
      <p className="ib-onboarding__success">HyprBridge is configured and ready to go.</p>
    </div>
  );
}
