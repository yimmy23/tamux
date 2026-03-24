import { useEffect, useState } from "react";
import { useAgentStore } from "../../lib/agentStore";
import type { AgentProviderId, SubAgentDefinition } from "../../lib/agentStore";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, Input, TextArea, fieldClassName } from "../ui";
import { ModelSelector } from "./shared";

type SubAgentForm = {
  name: string;
  provider: string;
  model: string;
  role: string;
  system_prompt: string;
  enabled: boolean;
  showAdvanced: boolean;
  tool_whitelist: string;
  tool_blacklist: string;
  context_budget_tokens: string;
  max_duration_secs: string;
};

const ROLE_PRESETS = [
  { id: "code_review", label: "Code Review", system_prompt: "You are a code review specialist. Focus on correctness, regressions, security, edge cases, missing tests, and actionable fixes. Be concise and precise." },
  { id: "research", label: "Research", system_prompt: "You are a research specialist. Gather relevant code and runtime context, compare options, identify constraints, and return clear conclusions with supporting evidence." },
  { id: "testing", label: "Testing", system_prompt: "You are a testing specialist. Design focused verification, find reproducible failure cases, validate fixes, and call out remaining risks or missing coverage." },
  { id: "planning", label: "Planning", system_prompt: "You are a planning specialist. Break work into durable, ordered steps with clear dependencies, acceptance criteria, and realistic implementation boundaries." },
  { id: "documentation", label: "Documentation", system_prompt: "You are a documentation specialist. Produce clear developer-facing docs, explain behavior accurately, and keep examples aligned with the current implementation." },
  { id: "refactoring", label: "Refactoring", system_prompt: "You are a refactoring specialist. Improve structure and maintainability without changing behavior, preserve intent, and keep edits scoped and defensible." },
] as const;

const emptyForm: SubAgentForm = {
  name: "",
  provider: "",
  model: "",
  role: "",
  system_prompt: "",
  enabled: true,
  showAdvanced: false,
  tool_whitelist: "",
  tool_blacklist: "",
  context_budget_tokens: "",
  max_duration_secs: "",
};

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-[var(--space-2)] border-t border-[var(--border-subtle)] pt-[var(--space-3)] first:border-t-0 first:pt-0 md:grid-cols-[minmax(0,12rem)_minmax(0,1fr)] md:items-start">
      <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{label}</span>
      {children}
    </div>
  );
}

export function SubAgentsTab() {
  const subAgents = useAgentStore((s) => s.subAgents);
  const providerAuthStates = useAgentStore((s) => s.providerAuthStates);
  const refreshSubAgents = useAgentStore((s) => s.refreshSubAgents);
  const refreshProviderAuthStates = useAgentStore((s) => s.refreshProviderAuthStates);
  const addSubAgent = useAgentStore((s) => s.addSubAgent);
  const removeSubAgent = useAgentStore((s) => s.removeSubAgent);
  const updateSubAgent = useAgentStore((s) => s.updateSubAgent);

  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<SubAgentForm>(emptyForm);

  useEffect(() => {
    refreshSubAgents();
    refreshProviderAuthStates();
  }, [refreshProviderAuthStates, refreshSubAgents]);

  const handleSave = async () => {
    const def: Omit<SubAgentDefinition, "id" | "created_at"> = {
      name: form.name,
      provider: form.provider,
      model: form.model,
      role: form.role || undefined,
      system_prompt: form.system_prompt || undefined,
      enabled: form.enabled,
      tool_whitelist: form.tool_whitelist ? form.tool_whitelist.split(",").map((s) => s.trim()).filter(Boolean) : undefined,
      tool_blacklist: form.tool_blacklist ? form.tool_blacklist.split(",").map((s) => s.trim()).filter(Boolean) : undefined,
      context_budget_tokens: form.context_budget_tokens ? Number(form.context_budget_tokens) : undefined,
      max_duration_secs: form.max_duration_secs ? Number(form.max_duration_secs) : undefined,
    };

    if (editingId) {
      const existing = subAgents.find((s) => s.id === editingId);
      await updateSubAgent({ ...def, id: editingId, created_at: existing?.created_at ?? Math.floor(Date.now() / 1000) });
    } else {
      await addSubAgent(def);
    }

    setForm(emptyForm);
    setShowForm(false);
    setEditingId(null);
  };

  const handleEdit = (sa: SubAgentDefinition) => {
    setForm({
      name: sa.name,
      provider: sa.provider,
      model: sa.model,
      role: sa.role || "",
      system_prompt: sa.system_prompt || "",
      enabled: sa.enabled,
      showAdvanced: false,
      tool_whitelist: sa.tool_whitelist?.join(", ") || "",
      tool_blacklist: sa.tool_blacklist?.join(", ") || "",
      context_budget_tokens: sa.context_budget_tokens ? String(sa.context_budget_tokens) : "",
      max_duration_secs: sa.max_duration_secs ? String(sa.max_duration_secs) : "",
    });
    setEditingId(sa.id);
    setShowForm(true);
  };

  const handleRoleChange = (nextRole: string) => {
    const preset = ROLE_PRESETS.find((item) => item.id === nextRole);
    const previousPreset = ROLE_PRESETS.find((item) => item.id === form.role);
    const shouldReplacePrompt = !form.system_prompt || (previousPreset && form.system_prompt === previousPreset.system_prompt);
    setForm({ ...form, role: nextRole, system_prompt: preset && shouldReplacePrompt ? preset.system_prompt : form.system_prompt });
  };

  const providerName = (id: string) => providerAuthStates.find((p) => p.provider_id === id)?.provider_name || id;

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <CardTitle>Sub-Agent Registry</CardTitle>
          <Badge variant="timeline">{subAgents.length} registered</Badge>
        </div>
        <CardDescription>Preserve daemon-backed orchestration, editing, and provider/model selection while moving the registry onto redesign surfaces.</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-[var(--space-4)]">
        {subAgents.length === 0 && !showForm ? <div className="text-[var(--text-sm)] text-[var(--text-secondary)]">No sub-agents configured. Add one to enable orchestration dispatch.</div> : null}

        <div className="grid gap-[var(--space-3)]">
          {subAgents.map((sa) => (
            <div key={sa.id} className="flex flex-wrap items-center justify-between gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--muted)]/50 p-[var(--space-3)]">
              <div className="flex min-w-0 flex-wrap items-center gap-[var(--space-2)]">
                <Badge variant={sa.enabled ? "success" : "default"}>{sa.name}</Badge>
                <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{providerName(sa.provider)} / {sa.model}</span>
                {sa.role ? <Badge variant="accent">{sa.role}</Badge> : null}
              </div>
              <div className="flex flex-wrap gap-[var(--space-2)]">
                <Button variant={sa.enabled ? "outline" : "primary"} size="sm" onClick={() => void updateSubAgent({ ...sa, enabled: !sa.enabled })}>{sa.enabled ? "Disable" : "Enable"}</Button>
                <Button variant="outline" size="sm" onClick={() => handleEdit(sa)}>Edit</Button>
                <Button variant="destructive" size="sm" onClick={() => void removeSubAgent(sa.id)}>Delete</Button>
              </div>
            </div>
          ))}
        </div>

        {showForm ? (
          <div className="grid gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--card)] p-[var(--space-4)]">
            <div className="flex flex-wrap items-center justify-between gap-[var(--space-2)]">
              <div className="text-[var(--text-sm)] font-medium text-[var(--text-primary)]">{editingId ? "Edit Sub-Agent" : "Add Sub-Agent"}</div>
              <Badge variant="accent">{form.enabled ? "Enabled" : "Disabled"}</Badge>
            </div>
            <Field label="Name">
              <Input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="e.g., Code Reviewer" />
            </Field>
            <Field label="Provider">
              <select value={form.provider} onChange={(e) => setForm({ ...form, provider: e.target.value, model: "" })} className={fieldClassName}>
                <option value="">Select provider...</option>
                {providerAuthStates.map((p) => (
                  <option key={p.provider_id} value={p.provider_id} disabled={!p.authenticated}>{p.provider_name}{!p.authenticated ? " (no key)" : ""}</option>
                ))}
              </select>
            </Field>
            <Field label="Model">
              {form.provider ? <ModelSelector providerId={form.provider as AgentProviderId} value={form.model} onChange={(model) => setForm({ ...form, model })} /> : <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">Select a provider first</span>}
            </Field>
            <Field label="Role">
              <select value={form.role} onChange={(e) => handleRoleChange(e.target.value)} className={fieldClassName}>
                <option value="">None</option>
                {ROLE_PRESETS.map((preset) => (
                  <option key={preset.id} value={preset.id}>{preset.label}</option>
                ))}
              </select>
            </Field>
            <Field label="System Prompt">
              <TextArea value={form.system_prompt} onChange={(e) => setForm({ ...form, system_prompt: e.target.value })} placeholder="Optional system prompt override" rows={4} />
            </Field>
            <div className="flex flex-wrap gap-[var(--space-2)]">
              <Button variant={form.showAdvanced ? "primary" : "outline"} size="sm" onClick={() => setForm({ ...form, showAdvanced: !form.showAdvanced })}>{form.showAdvanced ? "Hide Advanced" : "Show Advanced"}</Button>
              <Button variant={form.enabled ? "outline" : "primary"} size="sm" onClick={() => setForm({ ...form, enabled: !form.enabled })}>{form.enabled ? "Disable on save" : "Enable on save"}</Button>
            </div>
            {form.showAdvanced ? (
              <div className="grid gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--muted)]/40 p-[var(--space-3)]">
                <Field label="Tool Whitelist">
                  <Input value={form.tool_whitelist} onChange={(e) => setForm({ ...form, tool_whitelist: e.target.value })} placeholder="tool1, tool2" />
                </Field>
                <Field label="Tool Blacklist">
                  <Input value={form.tool_blacklist} onChange={(e) => setForm({ ...form, tool_blacklist: e.target.value })} placeholder="tool1, tool2" />
                </Field>
                <Field label="Budget (tokens)">
                  <Input type="number" value={form.context_budget_tokens} onChange={(e) => setForm({ ...form, context_budget_tokens: e.target.value })} placeholder="100000" />
                </Field>
                <Field label="Max Duration (s)">
                  <Input type="number" value={form.max_duration_secs} onChange={(e) => setForm({ ...form, max_duration_secs: e.target.value })} placeholder="300" />
                </Field>
              </div>
            ) : null}
            <div className="flex flex-wrap gap-[var(--space-2)]">
              <Button variant="primary" size="sm" onClick={() => void handleSave()} disabled={!form.name || !form.provider || !form.model}>{editingId ? "Update" : "Add"}</Button>
              <Button variant="outline" size="sm" onClick={() => { setShowForm(false); setEditingId(null); setForm(emptyForm); }}>Cancel</Button>
            </div>
          </div>
        ) : (
          <Button variant="primary" size="sm" onClick={() => { setShowForm(true); setEditingId(null); setForm(emptyForm); }}>Add Sub-Agent</Button>
        )}
      </CardContent>
    </Card>
  );
}
