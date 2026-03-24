import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "../../lib/agentStore";
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

    // ChatGPT subscription auth state
    const [chatgptAuthUrl, setChatgptAuthUrl] = useState<string | null>(null);
    const [chatgptAuthBusy, setChatgptAuthBusy] = useState(false);

    const openChatgptAuthUrl = (url: string) => {
        if (!url) return;
        const opened = typeof window !== "undefined"
            ? window.open(url, "_blank", "noopener,noreferrer")
            : null;
        if (opened) {
            opened.opener = null;
        }
    };

    useEffect(() => {
        refreshProviderAuthStates();
    }, []);

    // Poll ChatGPT auth status when auth URL is active
    useEffect(() => {
        if (!chatgptAuthUrl) return;
        const timer = window.setInterval(() => {
            const amux = getBridge();
            if (!amux?.openAICodexAuthStatus) return;
            void amux.openAICodexAuthStatus({ refresh: true }).then((status: any) => {
                if (status?.available) {
                    setChatgptAuthUrl(null);
                    refreshProviderAuthStates();
                }
            }).catch(() => {});
        }, 2000);
        return () => window.clearInterval(timer);
    }, [chatgptAuthUrl]);

    const filtered = providerAuthStates.filter((s) =>
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

    const handleChatgptLogin = async () => {
        const amux = getBridge();
        if (!amux?.openAICodexAuthLogin) return;
        setChatgptAuthBusy(true);
        try {
            const result = await amux.openAICodexAuthLogin();
            const authUrl = typeof result?.authUrl === "string" ? result.authUrl : null;
            setChatgptAuthUrl(authUrl);
            if (authUrl) {
                openChatgptAuthUrl(authUrl);
            }
            if (result?.available) {
                refreshProviderAuthStates();
            }
        } catch (error: any) {
            void error;
        } finally {
            setChatgptAuthBusy(false);
        }
    };

    const handleChatgptLogout = async () => {
        const amux = getBridge();
        if (!amux?.openAICodexAuthLogout) return;
        setChatgptAuthBusy(true);
        try {
            await amux.openAICodexAuthLogout();
            setChatgptAuthUrl(null);
            refreshProviderAuthStates();
        } catch { /* ignore */ } finally {
            setChatgptAuthBusy(false);
        }
    };

    const handleTest = async (providerId: string) => {
        setValidating(providerId);
        const state = filtered.find((s) => s.provider_id === providerId);
        try {
            if (providerId === "openai" && state?.auth_source === "chatgpt_subscription") {
                const amux = getBridge();
                if (!amux?.openAICodexAuthStatus) {
                    setValidationResult((prev) => ({
                        ...prev,
                        [providerId]: { valid: false, error: "ChatGPT auth bridge unavailable" },
                    }));
                    return;
                }
                const status = await amux.openAICodexAuthStatus({ refresh: true });
                setValidationResult((prev) => ({
                    ...prev,
                    [providerId]: status?.available
                        ? { valid: true }
                        : { valid: false, error: status?.error || "ChatGPT subscription auth not available" },
                }));
                return;
            }

            const result = await validateProvider(
                providerId,
                state?.base_url || "",
                "",
                state?.auth_source || "api_key"
            );
            setValidationResult((prev) => ({ ...prev, [providerId]: result }));
        } finally {
            setValidating(null);
        }
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
                {providerAuthStates.length === 0 && (
                    <div style={{ fontSize: 12, color: "var(--text-secondary)", padding: "8px 0" }}>
                        Loading providers from daemon...
                    </div>
                )}
                <div style={{ display: "grid", gap: 2 }}>
                    {filtered.map((state) => {
                        const isExpanded = loginTarget === state.provider_id;
                        const vr = validationResult[state.provider_id];
                        const isOpenAI = state.provider_id === "openai";
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
                                                    onClick={() => isOpenAI ? handleChatgptLogout() : handleLogout(state.provider_id)}
                                                    style={{ ...smallBtnStyle, fontSize: 10, color: "#ef4444" }}
                                                >
                                                    Logout
                                                </button>
                                            </>
                                        ) : (
                                            <>
                                                <button
                                                    onClick={() => {
                                                        setLoginTarget(isExpanded ? null : state.provider_id);
                                                        setLoginKey("");
                                                    }}
                                                    style={{ ...smallBtnStyle, fontSize: 10 }}
                                                >
                                                    {isExpanded ? "Cancel" : "API Key"}
                                                </button>
                                                {isOpenAI && (
                                                    <button
                                                        onClick={handleChatgptLogin}
                                                        disabled={chatgptAuthBusy}
                                                        style={{ ...smallBtnStyle, fontSize: 10, color: "var(--accent)" }}
                                                    >
                                                        {chatgptAuthBusy ? "..." : "ChatGPT Login"}
                                                    </button>
                                                )}
                                            </>
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
                                {isOpenAI && chatgptAuthUrl && (
                                    <div style={{ fontSize: 10, marginTop: 4, color: "var(--accent)" }}>
                                        <span>Auth URL: </span>
                                        <a
                                            href={chatgptAuthUrl}
                                            target="_blank"
                                            rel="noreferrer"
                                            onClick={(event) => {
                                                event.preventDefault();
                                                openChatgptAuthUrl(chatgptAuthUrl);
                                            }}
                                            style={{ color: "var(--accent)", textDecoration: "underline" }}
                                        >
                                            Open in browser
                                        </a>
                                        <span style={{ marginLeft: 8, color: "var(--text-secondary)" }}>Waiting for confirmation...</span>
                                    </div>
                                )}
                                {isExpanded && !isOpenAI && (
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
                                {isExpanded && isOpenAI && (
                                    <div style={{
                                        display: "flex",
                                        gap: 6,
                                        marginTop: 8,
                                        alignItems: "center",
                                    }}>
                                        <input
                                            type="password"
                                            placeholder="OpenAI API Key"
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
