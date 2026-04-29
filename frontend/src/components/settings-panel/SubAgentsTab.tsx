import { useEffect, useState } from "react";
import { useAgentStore } from "../../lib/agentStore";
import type { SubAgentDefinition, AgentProviderId } from "../../lib/agentStore";
import { getSubAgentCapabilities } from "../../lib/agentStore/providerActions";
import { selectableProviderAuthStates } from "./agentTabHelpers";
import { Section, SettingRow, ModelSelector, inputStyle, smallBtnStyle, addBtnStyle } from "./shared";
import { SUB_AGENT_ROLE_PRESETS, findSubAgentRolePreset } from "./subAgentRolePresets";

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
    reasoning_effort: string;
};

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
    reasoning_effort: "",
};

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
    const selectableProviders = selectableProviderAuthStates(providerAuthStates);

    useEffect(() => {
        refreshSubAgents();
        refreshProviderAuthStates();
    }, []);

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
            reasoning_effort: form.reasoning_effort || undefined,
        };

        if (editingId) {
            const existing = subAgents.find((s) => s.id === editingId);
            await updateSubAgent({
                ...def,
                id: editingId,
                created_at: existing?.created_at ?? Math.floor(Date.now() / 1000),
            });
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
            reasoning_effort: sa.reasoning_effort || "",
        });
        setEditingId(sa.id);
        setShowForm(true);
    };

    const handleDelete = async (id: string) => {
        await removeSubAgent(id);
    };

    const handleRoleChange = (nextRole: string) => {
        const preset = findSubAgentRolePreset(nextRole);
        const previousPreset = findSubAgentRolePreset(form.role);
        const shouldReplacePrompt = !form.system_prompt || (previousPreset && form.system_prompt === previousPreset.system_prompt);
        setForm({
            ...form,
            role: preset?.id ?? nextRole,
            system_prompt: preset && shouldReplacePrompt ? preset.system_prompt : form.system_prompt,
        });
    };

    const handleToggle = async (sa: SubAgentDefinition) => {
        await updateSubAgent({ ...sa, enabled: !sa.enabled });
    };

    const providerName = (id: string) => {
        const state = providerAuthStates.find((p) => p.provider_id === id);
        return state?.provider_name || id;
    };

    return (
        <div>
            <Section title="Sub-Agent Registry">
                {subAgents.length === 0 && !showForm && (
                    <div style={{ fontSize: 12, color: "var(--text-secondary)", marginBottom: 12 }}>
                        No sub-agents configured. Add one to enable orchestration dispatch.
                    </div>
                )}

                <div style={{ display: "grid", gap: 2, marginBottom: 12 }}>
                    {subAgents.map((sa) => (
                        (() => {
                            const capabilities = getSubAgentCapabilities(sa);
                            return (
                                <div key={sa.id} style={{
                                    border: "1px solid rgba(255,255,255,0.06)",
                                    background: "rgba(18, 33, 47, 0.5)",
                                    padding: "8px 12px",
                                }}>
                                    <div style={{
                                        display: "flex",
                                        alignItems: "center",
                                        justifyContent: "space-between",
                                        gap: 8,
                                    }}>
                                        <div style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
                                            <span style={{
                                                width: 8, height: 8, borderRadius: "50%",
                                                background: sa.enabled ? "#4ade80" : "#6b7280",
                                                flexShrink: 0,
                                            }} />
                                            <span style={{ fontSize: 12, fontWeight: 600 }}>{sa.name}</span>
                                            {capabilities.isProtected && (
                                                <span style={{
                                                    fontSize: 10,
                                                    color: "#fbbf24",
                                                    background: "rgba(251,191,36,0.12)",
                                                    padding: "1px 6px",
                                                    borderRadius: 3,
                                                }}>
                                                    Built-in
                                                </span>
                                            )}
                                            <span style={{
                                                fontSize: 10,
                                                color: "var(--text-secondary)",
                                                background: "rgba(255,255,255,0.05)",
                                                padding: "1px 6px",
                                                borderRadius: 3,
                                            }}>
                                                {providerName(sa.provider)} / {sa.model}
                                            </span>
                                            {sa.reasoning_effort && (
                                                <span style={{
                                                    fontSize: 10,
                                                    color: "var(--text-secondary)",
                                                    background: "rgba(255,255,255,0.05)",
                                                    padding: "1px 6px",
                                                    borderRadius: 3,
                                                }}>
                                                    effort: {sa.reasoning_effort}
                                                </span>
                                            )}
                                            {sa.role && (
                                                <span style={{
                                                    fontSize: 10,
                                                    color: "var(--accent)",
                                                    background: "rgba(97, 197, 255, 0.1)",
                                                    padding: "1px 6px",
                                                    borderRadius: 3,
                                                }}>
                                                    {sa.role}
                                                </span>
                                            )}
                                        </div>
                                        <div style={{ display: "flex", gap: 4 }}>
                                            {capabilities.canToggle && (
                                                <button onClick={() => handleToggle(sa)} style={{ ...smallBtnStyle, fontSize: 10 }}>
                                                    {sa.enabled ? "Disable" : "Enable"}
                                                </button>
                                            )}
                                            <button onClick={() => handleEdit(sa)} style={{ ...smallBtnStyle, fontSize: 10 }}>
                                                Edit
                                            </button>
                                            {capabilities.canDelete && (
                                                <button onClick={() => handleDelete(sa.id)} style={{ ...smallBtnStyle, fontSize: 10, color: "#ef4444" }}>
                                                    Delete
                                                </button>
                                            )}
                                        </div>
                                    </div>
                                    {capabilities.isProtected && capabilities.protectedReason && (
                                        <div style={{ fontSize: 11, color: "var(--text-secondary)", marginTop: 8 }}>
                                            {capabilities.protectedReason}
                                        </div>
                                    )}
                                </div>
                            );
                        })()
                    ))}
                </div>

                {showForm ? (
                    <div style={{
                        border: "1px solid rgba(255,255,255,0.1)",
                        background: "rgba(18, 33, 47, 0.7)",
                        padding: 14,
                    }}>
                        <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 10 }}>
                            {editingId ? "Edit Sub-Agent" : "Add Sub-Agent"}
                        </div>
                        <SettingRow label="Name">
                            <input
                                value={form.name}
                                onChange={(e) => setForm({ ...form, name: e.target.value })}
                                placeholder="e.g., Code Reviewer"
                                style={{ ...inputStyle, width: 220 }}
                            />
                        </SettingRow>
                        <SettingRow label="Provider">
                            <select
                                value={form.provider}
                                onChange={(e) => setForm({ ...form, provider: e.target.value, model: "" })}
                                style={{ ...inputStyle, width: 220 }}
                            >
                                <option value="">Select provider...</option>
                                {selectableProviders.map((p) => (
                                    <option key={p.provider_id} value={p.provider_id}>
                                        {p.provider_name}
                                    </option>
                                ))}
                            </select>
                        </SettingRow>
                        <SettingRow label="Model">
                            {form.provider ? (
                                <ModelSelector
                                    providerId={form.provider as AgentProviderId}
                                    value={form.model}
                                    onChange={(model) => setForm({ ...form, model })}
                                    allowProviderAuthFetch={Boolean(providerAuthStates.find((p) => p.provider_id === form.provider)?.authenticated)}
                                />
                            ) : (
                                <span style={{ fontSize: 11, color: "var(--text-secondary)" }}>Select a provider first</span>
                            )}
                        </SettingRow>
                        <SettingRow label="Role">
                            <select
                                value={form.role}
                                onChange={(e) => handleRoleChange(e.target.value)}
                                style={{ ...inputStyle, width: 220 }}
                            >
                                <option value="">None</option>
                                {SUB_AGENT_ROLE_PRESETS.map((preset) => (
                                    <option key={preset.id} value={preset.id}>{preset.label}</option>
                                ))}
                            </select>
                        </SettingRow>
                        <SettingRow label="System Prompt">
                            <textarea
                                value={form.system_prompt}
                                onChange={(e) => setForm({ ...form, system_prompt: e.target.value })}
                                placeholder="Optional system prompt override"
                                rows={3}
                                style={{ ...inputStyle, width: 220, resize: "vertical" }}
                            />
                        </SettingRow>
                        <SettingRow label="Reasoning Effort">
                            <select
                                value={form.reasoning_effort}
                                onChange={(e) => setForm({ ...form, reasoning_effort: e.target.value })}
                                style={{ ...inputStyle, width: 220 }}
                            >
                                <option value="">None</option>
                                <option value="minimal">Minimal</option>
                                <option value="low">Low</option>
                                <option value="medium">Medium</option>
                                <option value="high">High</option>
                                <option value="xhigh">Extra High</option>
                            </select>
                        </SettingRow>
                        <div style={{ marginTop: 6 }}>
                            <button
                                onClick={() => setForm({ ...form, showAdvanced: !form.showAdvanced })}
                                style={{ ...smallBtnStyle, fontSize: 10, marginBottom: 6 }}
                            >
                                {form.showAdvanced ? "Hide Advanced" : "Show Advanced"}
                            </button>
                            {form.showAdvanced && (
                                <div>
                                    <SettingRow label="Tool Whitelist">
                                        <input
                                            value={form.tool_whitelist}
                                            onChange={(e) => setForm({ ...form, tool_whitelist: e.target.value })}
                                            placeholder="tool1, tool2"
                                            style={{ ...inputStyle, width: 220 }}
                                        />
                                    </SettingRow>
                                    <SettingRow label="Tool Blacklist">
                                        <input
                                            value={form.tool_blacklist}
                                            onChange={(e) => setForm({ ...form, tool_blacklist: e.target.value })}
                                            placeholder="tool1, tool2"
                                            style={{ ...inputStyle, width: 220 }}
                                        />
                                    </SettingRow>
                                    <SettingRow label="Budget (tokens)">
                                        <input
                                            type="number"
                                            value={form.context_budget_tokens}
                                            onChange={(e) => setForm({ ...form, context_budget_tokens: e.target.value })}
                                            placeholder="100000"
                                            style={{ ...inputStyle, width: 220 }}
                                        />
                                    </SettingRow>
                                    <SettingRow label="Max Duration (s)">
                                        <input
                                            type="number"
                                            value={form.max_duration_secs}
                                            onChange={(e) => setForm({ ...form, max_duration_secs: e.target.value })}
                                            placeholder="300"
                                            style={{ ...inputStyle, width: 220 }}
                                        />
                                    </SettingRow>
                                </div>
                            )}
                        </div>
                        <div style={{ display: "flex", gap: 6, marginTop: 10 }}>
                            <button
                                onClick={handleSave}
                                disabled={!form.name || !form.provider || !form.model}
                                style={addBtnStyle}
                            >
                                {editingId ? "Update" : "Add"}
                            </button>
                            <button
                                onClick={() => { setShowForm(false); setEditingId(null); setForm(emptyForm); }}
                                style={smallBtnStyle}
                            >
                                Cancel
                            </button>
                        </div>
                    </div>
                ) : (
                    <button
                        onClick={() => { setShowForm(true); setEditingId(null); setForm(emptyForm); }}
                        style={addBtnStyle}
                    >
                        + Add Sub-Agent
                    </button>
                )}
            </Section>
        </div>
    );
}
