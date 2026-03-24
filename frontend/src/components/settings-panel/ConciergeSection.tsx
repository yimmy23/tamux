import { useEffect } from "react";
import { useAgentStore } from "../../lib/agentStore";
import type { AgentProviderId } from "../../lib/agentStore";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, fieldClassName } from "../ui";
import { ModelSelector } from "./shared";

const DETAIL_LEVELS = [
  { value: "minimal", label: "Quick Hello", desc: "Session title and date with action buttons. No AI call — instant." },
  { value: "context_summary", label: "Session Recap", desc: "AI-generated 1-2 sentence summary of your last session." },
  { value: "proactive_triage", label: "Smart Triage", desc: "Session summary plus pending tasks, alerts, and unfinished work." },
  { value: "daily_briefing", label: "Full Briefing", desc: "Complete operational briefing: sessions, tasks, health, gateways, snapshots." },
];

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-[var(--space-2)] border-t border-[var(--border-subtle)] pt-[var(--space-3)] first:border-t-0 first:pt-0 md:grid-cols-[minmax(0,12rem)_minmax(0,1fr)] md:items-center">
      <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{label}</span>
      {children}
    </div>
  );
}

export function ConciergeSection() {
  const config = useAgentStore((s) => s.conciergeConfig);
  const refresh = useAgentStore((s) => s.refreshConciergeConfig);
  const update = useAgentStore((s) => s.updateConciergeConfig);
  const providerAuthStates = useAgentStore((s) => s.providerAuthStates);
  const refreshProviderAuthStates = useAgentStore((s) => s.refreshProviderAuthStates);

  useEffect(() => {
    refresh();
    refreshProviderAuthStates();
  }, [refresh, refreshProviderAuthStates]);

  const selectedLevel = DETAIL_LEVELS.find((l) => l.value === config.detail_level) || DETAIL_LEVELS[2];

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <CardTitle>Concierge</CardTitle>
          <Badge variant={config.enabled ? "success" : "default"}>{config.enabled ? "Enabled" : "Disabled"}</Badge>
        </div>
        <CardDescription>Concierge detail level, provider selection, and model inheritance now sit on redesign cards without changing daemon config flow.</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-[var(--space-3)]">
        <Field label="Enabled">
          <Button variant={config.enabled ? "primary" : "outline"} size="sm" onClick={() => update({ ...config, enabled: !config.enabled })}>
            {config.enabled ? "On" : "Off"}
          </Button>
        </Field>
        <Field label="Detail Level">
          <select value={config.detail_level} onChange={(e) => update({ ...config, detail_level: e.target.value })} className={fieldClassName}>
            {DETAIL_LEVELS.map((l) => (
              <option key={l.value} value={l.value}>{l.label}</option>
            ))}
          </select>
        </Field>
        <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--muted)]/50 px-[var(--space-3)] py-[var(--space-2)] text-[var(--text-sm)] text-[var(--text-secondary)]">{selectedLevel.desc}</div>
        <Field label="Provider">
          <select value={config.provider || ""} onChange={(e) => update({ ...config, provider: e.target.value || undefined, model: undefined })} className={fieldClassName}>
            <option value="">Use main agent</option>
            {providerAuthStates.map((p) => (
              <option key={p.provider_id} value={p.provider_id}>{p.provider_name}{p.authenticated ? "" : " (no key)"}</option>
            ))}
          </select>
        </Field>
        {config.provider ? (
          <Field label="Model">
            <ModelSelector providerId={config.provider as AgentProviderId} value={config.model || ""} onChange={(model) => update({ ...config, model: model || undefined })} />
          </Field>
        ) : (
          <div className="text-[var(--text-sm)] text-[var(--text-secondary)]">Model inherited from the main agent when no provider is selected.</div>
        )}
      </CardContent>
    </Card>
  );
}
