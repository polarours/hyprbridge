import { ManagementConsole } from "@hyprbridge/ui";

export function App() {
  return (
    <ManagementConsole
      title="Modern Input Sharing, Built Like Infrastructure"
      subtitle="A daemon-first control plane for keyboard and mouse roaming across your LAN. HyprBridge surfaces topology, trust, session state, and Linux capability diagnostics without hiding platform constraints."
      shellBadge="Daemon online"
    />
  );
}
