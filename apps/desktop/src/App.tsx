import { ManagementConsole } from "@hyprbridge/ui";

export function DesktopApp() {
  return (
    <ManagementConsole
      title="HyprBridge Desktop"
      subtitle="The desktop shell now runs the same daemon-backed management console as the web surface, while staying ready for tray integration, permission helpers, startup wiring, and native packaging."
      shellBadge="Desktop shell"
    />
  );
}
