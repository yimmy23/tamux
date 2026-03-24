import { useEffect, useRef, useState, type CSSProperties } from "react";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useSettingsStore } from "../lib/settingsStore";
import { useAgentStore } from "../lib/agentStore";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import {
  buildDaemonAgentConfig,
  diffDaemonConfigEntries,
  getAgentBridge,
  shouldUseDaemonRuntime,
} from "../lib/agentDaemonConfig";
import { AboutTab } from "./settings-panel/AboutTab";
import { AppearanceTab } from "./settings-panel/AppearanceTab";
import { AgentTab } from "./settings-panel/AgentTab";
import { BehaviorTab } from "./settings-panel/BehaviorTab";
import { GatewayTab } from "./settings-panel/GatewayTab";
import { KeyboardTab } from "./settings-panel/KeyboardTab";
import { TerminalTab } from "./settings-panel/TerminalTab";
import { ProviderAuthTab } from "./settings-panel/ProviderAuthTab";
import { SubAgentsTab } from "./settings-panel/SubAgentsTab";
import { ConciergeSection } from "./settings-panel/ConciergeSection";
import {
  Badge,
  Button,
  Card,
  CardContent,
  ScrollArea,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  cn,
} from "./ui";

type SettingsTab =
  | "appearance"
  | "terminal"
  | "behavior"
  | "auth"
  | "agent"
  | "concierge"
  | "subagents"
  | "gateway"
  | "keyboard"
  | "about";

type SettingsPanelProps = {
  style?: CSSProperties;
  className?: string;
};

export function SettingsPanel({ style, className }: SettingsPanelProps = {}) {
  const open = useWorkspaceStore((s) => s.settingsOpen);
  const toggle = useWorkspaceStore((s) => s.toggleSettings);
  const settings = useSettingsStore((s) => s.settings);
  const updateSetting = useSettingsStore((s) => s.updateSetting);
  const resetSettings = useSettingsStore((s) => s.resetSettings);
  const profiles = useSettingsStore((s) => s.profiles);
  const addProfile = useSettingsStore((s) => s.addProfile);
  const removeProfile = useSettingsStore((s) => s.removeProfile);
  const updateProfile = useSettingsStore((s) => s.updateProfile);
  const setDefaultProfile = useSettingsStore((s) => s.setDefaultProfile);
  const agentSettings = useAgentStore((s) => s.agentSettings);
  const agentSettingsHydrated = useAgentStore((s) => s.agentSettingsHydrated);
  const updateAgentSetting = useAgentStore((s) => s.updateAgentSetting);
  const resetAgentSettings = useAgentStore((s) => s.resetAgentSettings);
  const refreshAgentSettingsFromDaemon = useAgentStore((s) => s.refreshAgentSettingsFromDaemon);
  const markAgentSettingsSynced = useAgentStore((s) => s.markAgentSettingsSynced);
  const approvals = useAgentMissionStore((s) => s.approvals);
  const snapshots = useAgentMissionStore((s) => s.snapshots);

  const [tab, setTab] = useState<SettingsTab>("appearance");
  const [systemFonts, setSystemFonts] = useState<string[]>([]);
  const [availableShells, setAvailableShells] = useState<{ name: string; path: string; args?: string }[]>([]);
  const [isFullscreen, setIsFullscreen] = useState(true);
  const lastDaemonConfigJsonRef = useRef<string | null>(null);

  useEffect(() => {
    if (open && systemFonts.length === 0) {
      if (typeof window !== "undefined" && ("tamux" in window || "amux" in window)) {
        const bridge = (window as any).tamux ?? (window as any).amux;
        bridge.getSystemFonts().then((fonts: string[]) => {
          setSystemFonts(fonts);
        });
        bridge
          .getAvailableShells?.()
          .then((shells: { name: string; path: string; args?: string }[]) => setAvailableShells(shells))
          .catch(() => {});
      }
    }
  }, [open, systemFonts.length]);

  useEffect(() => {
    if (open) {
      setIsFullscreen(true);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    void refreshAgentSettingsFromDaemon().then((ok) => {
      if (!ok) return;
      const latestAgentSettings = useAgentStore.getState().agentSettings;
      lastDaemonConfigJsonRef.current = JSON.stringify(buildDaemonAgentConfig(latestAgentSettings));
    });
  }, [open, refreshAgentSettingsFromDaemon]);

  useEffect(() => {
    if (!open) return;
    if (!agentSettingsHydrated) return;
    if (!shouldUseDaemonRuntime(agentSettings.agent_backend)) return;
    const bridge = getAgentBridge();
    if (!bridge?.agentSetConfigItem) return;
    const nextConfig = buildDaemonAgentConfig(agentSettings);
    const nextConfigJson = JSON.stringify(nextConfig);
    if (lastDaemonConfigJsonRef.current === null) return;
    if (lastDaemonConfigJsonRef.current === nextConfigJson) return;
    const previousConfig = JSON.parse(lastDaemonConfigJsonRef.current);
    const changes = diffDaemonConfigEntries(previousConfig, nextConfig);
    if (changes.length === 0) {
      lastDaemonConfigJsonRef.current = nextConfigJson;
      markAgentSettingsSynced();
      return;
    }
    void Promise.all(changes.map(({ keyPath, value }) => bridge.agentSetConfigItem?.(keyPath, value)))
      .then(() => {
        lastDaemonConfigJsonRef.current = nextConfigJson;
        markAgentSettingsSynced();
      })
      .catch(() => {});
  }, [open, settings, agentSettings, agentSettingsHydrated, markAgentSettingsSynced]);

  useEffect(() => {
    const handleOpenTab = (event: Event) => {
      const customEvent = event as CustomEvent<{ tab?: SettingsTab }>;
      const requestedTab = customEvent.detail?.tab;
      if (!requestedTab) return;
      if (["appearance", "terminal", "behavior", "auth", "agent", "concierge", "subagents", "gateway", "keyboard", "about"].includes(requestedTab)) {
        setTab(requestedTab);
      }
    };

    window.addEventListener("tamux-open-settings-tab", handleOpenTab as EventListener);
    window.addEventListener("amux-open-settings-tab", handleOpenTab as EventListener);
    return () => {
      window.removeEventListener("tamux-open-settings-tab", handleOpenTab as EventListener);
      window.removeEventListener("amux-open-settings-tab", handleOpenTab as EventListener);
    };
  }, []);

  if (!open) return null;

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: "appearance", label: "Interface" },
    { id: "terminal", label: "Execution" },
    { id: "behavior", label: "Operator" },
    { id: "auth", label: "Auth" },
    { id: "agent", label: "Agent" },
    { id: "concierge", label: "Concierge" },
    { id: "subagents", label: "Sub-Agents" },
    { id: "gateway", label: "Gateway" },
    { id: "keyboard", label: "Bindings" },
    { id: "about", label: "Runtime" },
  ];

  return (
    <div
      className={cn(isFullscreen && "backdrop-blur-[var(--panel-blur)]", className)}
      style={{
        ...(isFullscreen
          ? {
              position: "fixed",
              inset: 0,
              zIndex: 3600,
              width: "100vw",
              height: "100vh",
              padding: "var(--space-4)",
              background: "var(--overlay)",
            }
          : {
              width: 720,
              minWidth: 420,
              maxWidth: 1100,
              height: "100%",
              resize: "horizontal",
              overflow: "hidden",
            }),
        ...(style ?? {}),
      }}
    >
      <Card
        className={cn(
          "flex h-full min-h-0 flex-col overflow-hidden border-[var(--border-strong)] bg-[var(--bg-primary)]",
          isFullscreen ? "rounded-[var(--radius-2xl)] shadow-[var(--shadow-lg)]" : "rounded-[var(--radius-xl)]"
        )}
      >
        <div className="border-b border-[var(--border)] bg-[linear-gradient(180deg,var(--card),color-mix(in_srgb,var(--card)_88%,transparent))] p-[var(--space-5)]">
          <div className="flex flex-wrap items-start justify-between gap-[var(--space-4)]">
            <div className="grid gap-[var(--space-2)]">
              <div className="flex flex-wrap items-center gap-[var(--space-2)]">
                <Badge variant="mission">Operator Configuration</Badge>
                <Badge variant="accent">Redesign Surface</Badge>
              </div>
              <div className="grid gap-[var(--space-1)]">
                <h2 className="text-[clamp(1.5rem,2vw,2rem)] font-semibold tracking-[-0.02em] text-[var(--text-primary)]">
                  Mission Runtime Settings
                </h2>
                <p className="max-w-[70ch] text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
                  Tune visual hierarchy, terminal execution, providers, bindings, and runtime ergonomics from one control plane.
                </p>
              </div>
            </div>
            <div className="flex flex-wrap gap-[var(--space-2)]">
              <Button variant="outline" size="sm" onClick={() => setIsFullscreen((prev) => !prev)}>
                {isFullscreen ? "Window" : "Fullscreen"}
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  resetSettings();
                  resetAgentSettings();
                }}
              >
                Reset
              </Button>
              <Button variant="ghost" size="sm" onClick={toggle} aria-label="Close settings">
                ✕
              </Button>
            </div>
          </div>
          <div className="mt-[var(--space-4)] grid gap-[var(--space-3)] md:grid-cols-4">
            <SettingsMetric label="Theme" value={settings.themeName} tone="accent" />
            <SettingsMetric label="Provider" value={agentSettings.active_provider} tone="agent" />
            <SettingsMetric
              label="Approvals"
              value={String(approvals.filter((entry) => entry.status === "pending").length)}
              tone="warning"
            />
            <SettingsMetric label="Snapshots" value={String(snapshots.length)} tone="timeline" />
          </div>
        </div>

        <Tabs value={tab} onValueChange={(value) => setTab(value as SettingsTab)} className="flex min-h-0 flex-1 flex-col">
          <div className="border-b border-[var(--border)] px-[var(--space-4)] py-[var(--space-3)]">
            <ScrollArea className="w-full whitespace-nowrap">
              <TabsList className="w-max min-w-full justify-start rounded-[var(--radius-lg)] bg-[var(--muted)]/80">
                {tabs.map((t) => (
                  <TabsTrigger key={t.id} value={t.id} className="min-w-[7rem]">
                    {t.label}
                  </TabsTrigger>
                ))}
              </TabsList>
            </ScrollArea>
          </div>

          <ScrollArea className="flex-1">
            <CardContent className="p-[var(--space-5)]">
              <TabsContent value="appearance">
                <AppearanceTab settings={settings} updateSetting={updateSetting} systemFonts={systemFonts} />
              </TabsContent>
              <TabsContent value="terminal">
                <TerminalTab
                  settings={settings}
                  updateSetting={updateSetting}
                  availableShells={availableShells}
                  profiles={profiles}
                  addProfile={addProfile}
                  removeProfile={removeProfile}
                  updateProfile={updateProfile}
                  setDefaultProfile={setDefaultProfile}
                />
              </TabsContent>
              <TabsContent value="behavior">
                <BehaviorTab settings={settings} updateSetting={updateSetting} />
              </TabsContent>
              <TabsContent value="auth">
                <ProviderAuthTab />
              </TabsContent>
              <TabsContent value="agent">
                <AgentTab settings={agentSettings} updateSetting={updateAgentSetting} resetSettings={resetAgentSettings} />
              </TabsContent>
              <TabsContent value="concierge">
                <ConciergeSection />
              </TabsContent>
              <TabsContent value="subagents">
                <SubAgentsTab />
              </TabsContent>
              <TabsContent value="gateway">
                <GatewayTab settings={agentSettings} updateSetting={updateAgentSetting} />
              </TabsContent>
              <TabsContent value="keyboard">
                <KeyboardTab />
              </TabsContent>
              <TabsContent value="about">
                <AboutTab />
              </TabsContent>
            </CardContent>
          </ScrollArea>
        </Tabs>
      </Card>
    </div>
  );
}

function SettingsMetric({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "accent" | "agent" | "warning" | "timeline";
}) {
  return (
    <Card className="border-[var(--border)] bg-[var(--bg-secondary)]/70 shadow-none">
      <CardContent className="grid gap-[var(--space-1)] p-[var(--space-3)]">
        <div className="flex items-center justify-between gap-[var(--space-2)]">
          <span className="text-[var(--text-xs)] uppercase tracking-[0.08em] text-[var(--text-muted)]">{label}</span>
          <Badge variant={tone}>{label}</Badge>
        </div>
        <span className="truncate text-[var(--text-lg)] font-semibold text-[var(--text-primary)]">{value}</span>
      </CardContent>
    </Card>
  );
}
