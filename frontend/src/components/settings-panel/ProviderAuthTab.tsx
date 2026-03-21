import { useEffect, useState } from "react";
import { useAgentStore, PROVIDER_DEFINITIONS } from "../../lib/agentStore";
import { Section, inputStyle, smallBtnStyle } from "./shared";

export function ProviderAuthTab() {
    const providerAuthStates = useAgentStore((s) => s.providerAuthStates);
    const refreshProviderAuthStates = useAgentStore((s) => s.refreshProviderAuthStates);
    const validateProvider = useAgentStore((s) => s.validateProvider);
    const loginProvider = useAgentStore((s) => s.loginProvider);
    const logoutProvider = useAgentStore((s) => s.logoutProvider);

    const [filter, setFilter] = useState("");
    const [loginTarget, setLoginTarget] = useState<string | null>(null);
    const [loginKey, setLoginKey] = useState("");
    const [validating, setValidating] = useState<string | null>(null);
    const [validationResult, setValidationResult] = useState<Record<string, { valid: boolean; error?: string }>>({});

    useEffect(() => {
        refreshProviderAuthStates();
    }, []);

    const filtered = (providerAuthStates.length > 0 ? providerAuthStates : PROVIDER_DEFINITIONS.map((d) => ({
        provider_id: d.id,
        provider_name: d.name,
        authenticated: false,
        auth_source: "api_key" as const,
        model: d.defaultModel,
        base_url: d.defaultBaseUrl,
    }))).filter((s) =>
        !filter || s.provider_name.toLowerCase().includes(filter.toLowerCase()) || s.provider_id.toLowerCase().includes(filter.toLowerCase())
    );

    const handleLogin = async (providerId: string) => {
        if (!loginKey.trim()) return;
        const state = filtered.find((s) => s.provider_id === providerId);
        await loginProvider(providerId, loginKey, state?.base_url);
        setLoginKey("");
        setLoginTarget(null);
    };

    const handleLogout = async (providerId: string) => {
        await logoutProvider(providerId);
    };

    const handleTest = async (providerId: string) => {
        setValidating(providerId);
        const state = filtered.find((s) => s.provider_id === providerId);
        const result = await validateProvider(
            providerId,
            state?.base_url || "",
            "",
            state?.auth_source || "api_key"
        );
        setValidationResult((prev) => ({ ...prev, [providerId]: result }));
        setValidating(null);
    };

    return (
        <div>
            <Section title="Provider Authentication">
                <div style={{ marginBottom: 12 }}>
                    <input
                        type="text"
                        placeholder="Filter providers..."
                        value={filter}
                        onChange={(e) => setFilter(e.target.value)}
                        style={{ ...inputStyle, width: "100%" }}
                    />
                </div>
                <div style={{ display: "grid", gap: 2 }}>
                    {filtered.map((state) => {
                        const isExpanded = loginTarget === state.provider_id;
                        const vr = validationResult[state.provider_id];
                        return (
                            <div key={state.provider_id} style={{
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
                                            width: 8,
                                            height: 8,
                                            borderRadius: "50%",
                                            background: state.authenticated ? "#4ade80" : "#6b7280",
                                            flexShrink: 0,
                                        }} />
                                        <span style={{ fontSize: 12, fontWeight: 600 }}>{state.provider_name}</span>
                                        {state.authenticated && (
                                            <span style={{
                                                fontSize: 10,
                                                color: "var(--text-secondary)",
                                                background: "rgba(255,255,255,0.05)",
                                                padding: "1px 6px",
                                                borderRadius: 3,
                                            }}>
                                                {state.model || "—"}
                                            </span>
                                        )}
                                    </div>
                                    <div style={{ display: "flex", gap: 4 }}>
                                        {state.authenticated ? (
                                            <>
                                                <button
                                                    onClick={() => handleTest(state.provider_id)}
                                                    disabled={validating === state.provider_id}
                                                    style={{ ...smallBtnStyle, fontSize: 10 }}
                                                >
                                                    {validating === state.provider_id ? "Testing..." : "Test"}
                                                </button>
                                                <button
                                                    onClick={() => handleLogout(state.provider_id)}
                                                    style={{ ...smallBtnStyle, fontSize: 10, color: "#ef4444" }}
                                                >
                                                    Logout
                                                </button>
                                            </>
                                        ) : (
                                            <button
                                                onClick={() => {
                                                    setLoginTarget(isExpanded ? null : state.provider_id);
                                                    setLoginKey("");
                                                }}
                                                style={{ ...smallBtnStyle, fontSize: 10 }}
                                            >
                                                {isExpanded ? "Cancel" : "Login"}
                                            </button>
                                        )}
                                    </div>
                                </div>
                                {vr && (
                                    <div style={{
                                        fontSize: 10,
                                        marginTop: 4,
                                        color: vr.valid ? "#4ade80" : "#ef4444",
                                    }}>
                                        {vr.valid ? "Connection OK" : `Error: ${vr.error || "unknown"}`}
                                    </div>
                                )}
                                {isExpanded && (
                                    <div style={{
                                        display: "flex",
                                        gap: 6,
                                        marginTop: 8,
                                        alignItems: "center",
                                    }}>
                                        <input
                                            type="password"
                                            placeholder="API Key"
                                            value={loginKey}
                                            onChange={(e) => setLoginKey(e.target.value)}
                                            onKeyDown={(e) => {
                                                if (e.key === "Enter") handleLogin(state.provider_id);
                                            }}
                                            style={{ ...inputStyle, flex: 1 }}
                                            autoFocus
                                        />
                                        <button
                                            onClick={() => handleLogin(state.provider_id)}
                                            disabled={!loginKey.trim()}
                                            style={smallBtnStyle}
                                        >
                                            Save
                                        </button>
                                    </div>
                                )}
                            </div>
                        );
                    })}
                </div>
            </Section>
        </div>
    );
}
