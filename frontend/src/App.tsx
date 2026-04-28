import { useEffect } from "react";
import { startAutoSave } from "./lib/sessionPersistence";
import { applyAppShellTheme, getAppShellTheme } from "./lib/themes";
import { useSettingsStore } from "./lib/settingsStore";
import { useWorkspaceStore } from "./lib/workspaceStore";
import { ZoraiApp } from "./zorai/ZoraiApp";

export default function App() {
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const workspaces = useWorkspaceStore((state) => state.workspaces);
  const settings = useSettingsStore((state) => state.settings);

  useEffect(() => {
    if (workspaces.length === 0) {
      createWorkspace("Default");
    }
  }, [createWorkspace, workspaces.length]);

  useEffect(() => startAutoSave(30_000), []);

  useEffect(() => {
    applyAppShellTheme(
      getAppShellTheme(
        settings.themeName,
        settings.useCustomTerminalColors,
        settings.customTerminalBackground,
        settings.customTerminalForeground,
        settings.customTerminalCursor,
        settings.customTerminalSelection,
      ),
    );
  }, [
    settings.themeName,
    settings.useCustomTerminalColors,
    settings.customTerminalBackground,
    settings.customTerminalForeground,
    settings.customTerminalCursor,
    settings.customTerminalSelection,
  ]);

  return <ZoraiApp />;
}
