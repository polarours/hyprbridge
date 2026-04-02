import type { OnboardingData } from "./wizard";

export function WelcomeStep(_props: { data: OnboardingData; onUpdate: (d: Partial<OnboardingData>) => void }) {
  return (
    <div className="ib-onboarding__step">
      <h2>Welcome to HyprBridge</h2>
      <p>Let's get your device set up in a few quick steps.</p>
    </div>
  );
}
