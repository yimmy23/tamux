import { useEffect, useState } from "react";
import type { AgentProviderConfig, AgentProviderId, AgentSettings } from "../../lib/agentStore";
import { getDefaultApiTransport, getDefaultAuthSource, getDefaultModelForProvider, getEffectiveContextWindow, getProviderApiType, getProviderDefinition, getProviderModels, getSupportedApiTransports, getSupportedAuthSources } from "../../lib/agentStore";
import { addBtnStyle, ModelSelector, NumberInput, PasswordInput, Section, SelectInput, SettingRow, TextInput, Toggle, inputStyle, smallBtnStyle } from "./shared";

export function AgentTab({
    settings, updateSetting, resetSettings,
}: {
    settings: AgentSettings;
    updateSetting: <K extends keyof AgentSettings>(key: K, value: AgentSettings[K]) => void;
    resetSettings: () => void;
}) {
    const [useCustomUrl, setUseCustomUrl] = useState(false);
    const [subscriptionAuthStatus, setSubscriptionAuthStatus] = useState<any>(null);
    const [subscriptionAuthBusy, setSubscriptionAuthBusy] = useState(false);
    const [subscriptionAuthUrl, setSubscriptionAuthUrl] = useState<string | null>(null);

    const openSubscriptionAuthUrl = (url: string) => {
        if (!url) return;
        const opened = typeof window !== "undefined"
            ? window.open(url, "_blank", "noopener,noreferrer")
            : null;
        if (opened) {
            opened.opener = null;
        }
    };

    const providerOptions: { id: AgentProviderId; label: string }[] = [
        { id: "featherless", label: "Featherless" },
        { id: "openai", label: "OpenAI / ChatGPT" },
        { id: "qwen", label: "Qwen" },
        { id: "qwen-deepinfra", label: "Qwen (DeepInfra)" },
        { id: "kimi", label: "Kimi (Moonshot)" },
        { id: "kimi-coding-plan", label: "Kimi Coding Plan" },
        { id: "z.ai", label: "Z.AI" },
        { id: "z.ai-coding-plan", label: "Z.AI Coding Plan" },
        { id: "openrouter", label: "OpenRouter" },
        { id: "cerebras", label: "Cerebras" },
        { id: "together", label: "Together" },
        { id: "groq", label: "Groq" },
        { id: "ollama", label: "Ollama" },
        { id: "chutes", label: "Chutes" },
        { id: "huggingface", label: "HuggingFace" },
        { id: "minimax", label: "MiniMax" },
        { id: "minimax-coding-plan", label: "MiniMax Coding Plan" },
        { id: "alibaba-coding-plan", label: "Alibaba Coding Plan" },
        { id: "opencode-zen", label: "OpenCode Zen" },
        { id: "custom", label: "Custom" },
    ];

    const providerConfig = settings[settings.active_provider] as AgentProviderConfig;
    const providerDef = getProviderDefinition(settings.active_provider);
    const providerApiType = getProviderApiType(
        settings.active_provider,
        providerConfig.model,
        providerConfig.base_url,
    );
    const supportedTransports = getSupportedApiTransports(settings.active_provider);
    const supportedAuthSources = getSupportedAuthSources(settings.active_provider);
    const isCustomProvider = settings.active_provider === "custom";
    const showUrlEditor = isCustomProvider || useCustomUrl || Boolean(providerConfig.base_url && providerConfig.base_url !== providerDef?.defaultBaseUrl);
    const effectiveContextWindow = getEffectiveContextWindow(settings.active_provider, providerConfig);
    const providerAuthenticated = providerConfig.auth_source === "chatgpt_subscription"
        ? Boolean(subscriptionAuthStatus?.available)
        : Boolean(providerConfig.api_key);

    useEffect(() => {
        if (settings.active_provider !== "openai" || providerConfig.auth_source !== "chatgpt_subscription") {
            setSubscriptionAuthStatus(null);
            setSubscriptionAuthUrl(null);
            return;
        }

        const amux = (window as any).amux || (window as any).tamux;
        if (!amux?.openAICodexAuthStatus) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: "ChatGPT auth bridge unavailable" });
            return;
        }

        let cancelled = false;
        void amux.openAICodexAuthStatus({ refresh: true }).then((status: any) => {
            if (!cancelled) {
                setSubscriptionAuthStatus(status);
            }
        }).catch((error: any) => {
            if (!cancelled) {
                setSubscriptionAuthStatus({ ok: false, available: false, error: error?.message || "Failed to read ChatGPT auth" });
            }
        });

        return () => {
            cancelled = true;
        };
    }, [settings.active_provider, providerConfig.auth_source]);

    useEffect(() => {
        if (!subscriptionAuthUrl) {
            return;
        }

        const timer = window.setInterval(() => {
            const amux = (window as any).amux || (window as any).tamux;
            if (!amux?.openAICodexAuthStatus) {
                return;
            }
            void amux.openAICodexAuthStatus({ refresh: true }).then((status: any) => {
                if (status?.available) {
                    setSubscriptionAuthStatus(status);
                    setSubscriptionAuthUrl(null);
                }
            }).catch(() => {});
        }, 2000);

        return () => window.clearInterval(timer);
    }, [subscriptionAuthUrl]);

    const triggerSubscriptionAuth = async () => {
        const amux = (window as any).amux || (window as any).tamux;
        if (!amux?.openAICodexAuthLogin) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: "ChatGPT auth bridge unavailable" });
            return;
        }

        setSubscriptionAuthBusy(true);
        try {
            const result = await amux.openAICodexAuthLogin();
            setSubscriptionAuthStatus(result);
            const authUrl = typeof result?.authUrl === "string" ? result.authUrl : null;
            setSubscriptionAuthUrl(authUrl);
            if (authUrl) {
                openSubscriptionAuthUrl(authUrl);
            }
        } catch (error: any) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: error?.message || "ChatGPT authentication failed" });
        } finally {
            setSubscriptionAuthBusy(false);
        }
    };

    const clearSubscriptionAuth = async () => {
        const amux = (window as any).amux || (window as any).tamux;
        if (!amux?.openAICodexAuthLogout) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: "ChatGPT auth bridge unavailable" });
            return;
        }

        setSubscriptionAuthBusy(true);
        try {
            await amux.openAICodexAuthLogout();
            setSubscriptionAuthStatus({ available: false, authMode: "chatgpt_subscription", error: "No ChatGPT subscription auth found" });
            setSubscriptionAuthUrl(null);
        } catch (error: any) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: error?.message || "Failed to clear ChatGPT auth" });
        } finally {
            setSubscriptionAuthBusy(false);
        }
    };

    return (
        <>
            <div style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                marginBottom: 14,
                padding: "8px 12px",
                border: "1px solid rgba(255,255,255,0.06)",
                background: "rgba(18, 33, 47, 0.5)",
            }}>
                <span style={{
                    width: 8, height: 8, borderRadius: "50%",
                    background: providerAuthenticated ? "#4ade80" : "#6b7280",
                }} />
                <span style={{ fontSize: 12, fontWeight: 600 }}>
                    Active: {providerOptions.find((p) => p.id === settings.active_provider)?.label || settings.active_provider}
                </span>
                <span style={{
                    fontSize: 10,
                    color: "var(--text-secondary)",
                    background: "rgba(255,255,255,0.05)",
                    padding: "1px 6px",
                    borderRadius: 3,
                }}>
                    {providerConfig.model || "No model"}
                </span>
                <button
                    onClick={() => {
                        window.dispatchEvent(new CustomEvent("tamux-open-settings-tab", { detail: { tab: "auth" } }));
                        window.dispatchEvent(new CustomEvent("amux-open-settings-tab", { detail: { tab: "auth" } }));
                    }}
                    style={{ ...smallBtnStyle, fontSize: 10, marginLeft: "auto" }}
                >
                    Manage Auth
                </button>
            </div>
            <Section title="General">
                <SettingRow label="Enable Agent">
                    <Toggle value={settings.enabled} onChange={(value) => updateSetting("enabled", value)} />
                </SettingRow>
                <SettingRow label="Agent Backend">
                    <select value={settings.agent_backend}
                        onChange={(e) => updateSetting("agent_backend", e.target.value as "daemon" | "openclaw" | "hermes" | "legacy")}
                        style={inputStyle}>
                        <option value="daemon">tamux</option>
                        <option value="openclaw">OpenClaw</option>
                        <option value="hermes">Hermes</option>
                        <option value="legacy">Legacy Fallback</option>
                    </select>
                </SettingRow>
                {settings.agent_backend === "legacy" ? (
                    <div style={{ marginTop: 4, marginBottom: 8, fontSize: 11, color: "var(--text-secondary)", lineHeight: 1.4 }}>
                        Legacy now acts as a frontend-only fallback when the desktop daemon bridge is unavailable. When the bridge is present, chat and goal runs still use the daemon stack so memory, goals, and self-orchestrating capabilities stay consistent.
                    </div>
                ) : null}
                {(settings.agent_backend === "openclaw" || settings.agent_backend === "hermes") ? (
                    <div style={{ marginTop: 4, marginBottom: 8, fontSize: 11, color: "var(--text-secondary)", lineHeight: 1.4 }}>
                        <strong>{settings.agent_backend === "openclaw" ? "OpenClaw" : "Hermes"}</strong> will handle LLM inference, tools, memory, and gateway connections using its own infrastructure.
                        {settings.agent_backend === "hermes" ? (
                            <span> Config: <code>~/.hermes/config.yaml</code></span>
                        ) : (
                            <span> Config: <code>~/.openclaw/openclaw.json</code></span>
                        )}
                        <div style={{ marginTop: 6, padding: "4px 6px", background: "var(--bg-secondary)", borderRadius: 3 }}>
                            <strong>tamux tools:</strong> Add <code>tamux-mcp</code> to {settings.agent_backend === "hermes" ? "Hermes" : "OpenClaw"}'s MCP config for terminal session access, command execution, history search, and more.
                            <div style={{ marginTop: 3, fontFamily: "monospace", fontSize: 10 }}>
                                {`{"mcpServers": {"tamux": {"command": "tamux-mcp"}}}`}
                            </div>
                        </div>
                    </div>
                ) : null}
                <SettingRow label="Agent Name">
                    <TextInput value={settings.agent_name} onChange={(value) => updateSetting("agent_name", value)} />
                </SettingRow>
                <SettingRow label="Handler Prefix">
                    <TextInput value={settings.handler} onChange={(value) => updateSetting("handler", value)} />
                </SettingRow>
                <SettingRow label="System Prompt">
                    <textarea value={settings.system_prompt}
                        onChange={(event) => updateSetting("system_prompt", event.target.value)}
                        rows={4}
                        style={{ ...inputStyle, width: "100%", resize: "vertical", fontFamily: "inherit" }} />
                </SettingRow>
            </Section>

            {settings.agent_backend !== "openclaw" && settings.agent_backend !== "hermes" ? (
                <Section title="Provider">
                    <SettingRow label="Active Provider">
                        <SelectInput value={settings.active_provider}
                            options={providerOptions.map((provider) => provider.id)}
                            onChange={(value) => updateSetting("active_provider", value as AgentProviderId)} />
                    </SettingRow>

                    <div style={{ marginTop: 6, marginBottom: 6, fontSize: 11, color: "var(--text-secondary)" }}>
                        {providerOptions.find((provider) => provider.id === settings.active_provider)?.label}
                    </div>

                    {showUrlEditor ? (
                        <SettingRow label="Base URL">
                            <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                                <TextInput value={providerConfig.base_url}
                                    onChange={(value) => updateSetting(settings.active_provider, { ...providerConfig, base_url: value })}
                                    placeholder={providerDef?.defaultBaseUrl} />
                                {!isCustomProvider && (
                                    <button type="button" onClick={() => {
                                        updateSetting(settings.active_provider, { ...providerConfig, base_url: "" });
                                        setUseCustomUrl(false);
                                    }} style={smallBtnStyle} title="Reset to predefined default">
                                        Reset
                                    </button>
                                )}
                            </div>
                        </SettingRow>
                    ) : (
                        <SettingRow label="Base URL">
                            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                                <span style={{
                                    fontSize: 11,
                                    fontFamily: "var(--font-mono)",
                                    color: "var(--text-muted)",
                                    background: "var(--bg-surface)",
                                    padding: "3px 8px",
                                    border: "1px solid var(--border)",
                                    flex: 1,
                                }}>
                                    {providerDef?.defaultBaseUrl || "(none)"}
                                </span>
                                {!isCustomProvider && (
                                    <button type="button" onClick={() => setUseCustomUrl(true)} style={smallBtnStyle}>
                                        Override
                                    </button>
                                )}
                            </div>
                        </SettingRow>
                    )}
                    <SettingRow label="Model">
                        <ModelSelector
                            providerId={settings.active_provider}
                            value={providerConfig.model}
                            customName={providerConfig.custom_model_name}
                            onChange={(value, custom_model_name) => updateSetting(settings.active_provider, {
                                ...providerConfig,
                                model: value,
                                custom_model_name: custom_model_name && custom_model_name !== value ? custom_model_name : "",
                            })}
                            base_url={providerConfig.base_url || providerDef?.defaultBaseUrl}
                            api_key={providerConfig.api_key}
                            auth_source={providerConfig.auth_source}
                        />
                    </SettingRow>
                    {providerApiType === "openai" ? (
                        <SettingRow label="Auth">
                            <select
                                value={providerConfig.auth_source}
                                onChange={(e) => updateSetting(settings.active_provider, {
                                    ...providerConfig,
                                    auth_source: supportedAuthSources.includes(e.target.value as any)
                                      ? e.target.value as AgentProviderConfig["auth_source"]
                                      : getDefaultAuthSource(settings.active_provider),
                                    model: (() => {
                                        const nextAuthSource = supportedAuthSources.includes(e.target.value as any)
                                          ? e.target.value as AgentProviderConfig["auth_source"]
                                          : getDefaultAuthSource(settings.active_provider);
                                        const supportedModels = getProviderModels(settings.active_provider, nextAuthSource);
                                        return supportedModels.some((entry) => entry.id === providerConfig.model)
                                          ? providerConfig.model
                                          : getDefaultModelForProvider(settings.active_provider, nextAuthSource);
                                    })(),
                                })}
                                style={inputStyle}
                            >
                                {supportedAuthSources.map((source) => (
                                    <option key={source} value={source}>
                                        {source === "chatgpt_subscription" ? "ChatGPT Subscription" : "API Key"}
                                    </option>
                                ))}
                            </select>
                        </SettingRow>
                    ) : null}
                    <div style={{ marginTop: 2, marginBottom: 8, fontSize: 11, color: "var(--text-secondary)", lineHeight: 1.4 }}>
                        Credentials are managed in the <strong>Auth</strong> tab. Keep provider selection, model, base URL, and transport here.
                    </div>
                    {settings.active_provider === "openai" && providerConfig.auth_source === "chatgpt_subscription" ? (
                        <SettingRow label="ChatGPT Auth">
                            <div style={{ display: "grid", gap: 6, width: "100%" }}>
                                <div style={{ display: "flex", alignItems: "center", gap: 8, justifyContent: "flex-end" }}>
                                    <span style={{ fontSize: 11, color: subscriptionAuthStatus?.available ? "var(--success, #6ee7b7)" : "var(--text-secondary)" }}>
                                        {subscriptionAuthStatus?.available
                                            ? `Connected (${subscriptionAuthStatus.source || subscriptionAuthStatus.authMode || "tamux"})`
                                            : subscriptionAuthStatus?.error || "No ChatGPT subscription auth found"}
                                    </span>
                                    {subscriptionAuthStatus?.available ? (
                                        <button type="button" onClick={clearSubscriptionAuth} style={smallBtnStyle} disabled={subscriptionAuthBusy}>
                                            {subscriptionAuthBusy ? "Working..." : "Logout"}
                                        </button>
                                    ) : (
                                        <button type="button" onClick={triggerSubscriptionAuth} style={smallBtnStyle} disabled={subscriptionAuthBusy}>
                                            {subscriptionAuthBusy ? "Preparing..." : "Login"}
                                        </button>
                                    )}
                                </div>
                                {subscriptionAuthUrl ? (
                                    <div style={{ display: "grid", gap: 6, justifyItems: "end" }}>
                                        <a
                                            href={subscriptionAuthUrl}
                                            target="_blank"
                                            rel="noreferrer"
                                            onClick={(event) => {
                                                event.preventDefault();
                                                openSubscriptionAuthUrl(subscriptionAuthUrl);
                                            }}
                                            style={{ fontSize: 11, color: "var(--accent, #60a5fa)", wordBreak: "break-all", textAlign: "right" }}
                                        >
                                            {subscriptionAuthUrl}
                                        </a>
                                        <div style={{ display: "flex", gap: 6 }}>
                                            <button
                                                type="button"
                                                onClick={() => openSubscriptionAuthUrl(subscriptionAuthUrl)}
                                                style={smallBtnStyle}
                                            >
                                                Open Browser
                                            </button>
                                            <button
                                                type="button"
                                                onClick={() => {
                                                    const amux = (window as any).amux || (window as any).tamux;
                                                    if (amux?.writeClipboardText) {
                                                        void amux.writeClipboardText(subscriptionAuthUrl);
                                                        return;
                                                    }
                                                    void navigator.clipboard?.writeText(subscriptionAuthUrl).catch(() => {});
                                                }}
                                                style={smallBtnStyle}
                                            >
                                                Copy Link
                                            </button>
                                        </div>
                                    </div>
                                ) : null}
                                {subscriptionAuthUrl ? (
                                    <div style={{ fontSize: 11, color: "var(--text-secondary)", textAlign: "right" }}>
                                        Open the link above and complete ChatGPT authentication. This row updates automatically after the callback returns.
                                    </div>
                                ) : null}
                            </div>
                        </SettingRow>
                    ) : null}
                    {providerApiType === "openai" ? (
                        <SettingRow label="API Transport">
                            <select
                                value={providerConfig.api_transport}
                                onChange={(e) => updateSetting(settings.active_provider, {
                                    ...providerConfig,
                                    api_transport: supportedTransports.includes(e.target.value as any)
                                      ? (e.target.value as AgentProviderConfig["api_transport"])
                                      : getDefaultApiTransport(settings.active_provider),
                                })}
                                style={inputStyle}
                            >
                                {supportedTransports.map((transport) => (
                                    <option key={transport} value={transport}>
                                        {transport === "native_assistant"
                                          ? "Native Assistant"
                                          : transport === "responses"
                                            ? "Responses"
                                            : "Legacy Chat Completions"}
                                    </option>
                                ))}
                            </select>
                        </SettingRow>
                    ) : null}
                    {providerConfig.api_transport === "native_assistant" ? (
                        <SettingRow label="Assistant ID">
                            <TextInput
                                value={providerConfig.assistant_id}
                                onChange={(value) => updateSetting(settings.active_provider, {
                                    ...providerConfig,
                                    assistant_id: value,
                                })}
                                placeholder="asst_..."
                            />
                        </SettingRow>
                    ) : null}
                    <SettingRow label="Context Length">
                        {isCustomProvider ? (
                            <NumberInput
                                value={providerConfig.context_window_tokens ?? 128000}
                                min={16000}
                                max={2000000}
                                step={1000}
                                onChange={(value) => updateSetting(settings.active_provider, {
                                    ...providerConfig,
                                    context_window_tokens: Math.max(1000, Math.trunc(value)),
                                })}
                            />
                        ) : (
                            <span style={{
                                fontSize: 11,
                                fontFamily: "var(--font-mono)",
                                color: "var(--text-muted)",
                                background: "var(--bg-surface)",
                                padding: "3px 8px",
                                border: "1px solid var(--border)",
                                minWidth: 120,
                                textAlign: "right",
                            }}>
                                {effectiveContextWindow.toLocaleString()} tok
                            </span>
                        )}
                    </SettingRow>
                    <SettingRow label="Reasoning Effort">
                        <select value={settings.reasoning_effort}
                            onChange={(e) => updateSetting("reasoning_effort", e.target.value as AgentSettings["reasoning_effort"])}
                            style={inputStyle}>
                            <option value="none">None</option>
                            <option value="minimal">Minimal</option>
                            <option value="low">Low</option>
                            <option value="medium">Medium</option>
                            <option value="high">High</option>
                            <option value="xhigh">Extra High</option>
                        </select>
                    </SettingRow>
                </Section>
            ) : null}

            <Section title="Tools">
                <SettingRow label="Bash Tool">
                    <Toggle value={settings.enable_bash_tool} onChange={(value) => updateSetting("enable_bash_tool", value)} />
                </SettingRow>
                <SettingRow label="Vision Tool">
                    <Toggle value={settings.enable_vision_tool} onChange={(value) => updateSetting("enable_vision_tool", value)} />
                </SettingRow>
                <SettingRow label="Web Browsing Tool">
                    <Toggle value={settings.enable_web_browsing_tool} onChange={(value) => updateSetting("enable_web_browsing_tool", value)} />
                </SettingRow>
                <SettingRow label="Bash Timeout (s)">
                    <NumberInput value={settings.bash_timeout_seconds} min={5} max={300}
                        onChange={(value) => updateSetting("bash_timeout_seconds", value)} />
                </SettingRow>
                <SettingRow label="Web Search Tool">
                    <Toggle value={settings.enable_web_search_tool} onChange={(value) => updateSetting("enable_web_search_tool", value)} />
                </SettingRow>
                {settings.enable_web_search_tool ? (
                    <>
                        <SettingRow label="Search Provider">
                            <SelectInput
                                value={settings.search_provider}
                                options={["none", "firecrawl", "exa", "tavily"]}
                                onChange={(value) => updateSetting("search_provider", value as "none" | "firecrawl" | "exa" | "tavily")}
                            />
                        </SettingRow>
                        <SettingRow label="Firecrawl API Key">
                            <PasswordInput value={settings.firecrawl_api_key} onChange={(value) => updateSetting("firecrawl_api_key", value)} placeholder="fc-..." />
                        </SettingRow>
                        <SettingRow label="Exa API Key">
                            <PasswordInput value={settings.exa_api_key} onChange={(value) => updateSetting("exa_api_key", value)} placeholder="exa_..." />
                        </SettingRow>
                        <SettingRow label="Tavily API Key">
                            <PasswordInput value={settings.tavily_api_key} onChange={(value) => updateSetting("tavily_api_key", value)} placeholder="tvly-..." />
                        </SettingRow>
                        <SettingRow label="Search Max Results">
                            <NumberInput value={settings.search_max_results} min={1} max={20}
                                onChange={(value) => updateSetting("search_max_results", value)} />
                        </SettingRow>
                        <SettingRow label="Search Timeout (s)">
                            <NumberInput value={settings.search_timeout_secs} min={3} max={120}
                                onChange={(value) => updateSetting("search_timeout_secs", value)} />
                        </SettingRow>
                    </>
                ) : null}
            </Section>

            <Section title="Chat">
                <SettingRow label="Streaming">
                    <Toggle value={settings.enable_streaming} onChange={(value) => updateSetting("enable_streaming", value)} />
                </SettingRow>
                <SettingRow label="Conversation Memory">
                    <Toggle value={settings.enable_conversation_memory} onChange={(value) => updateSetting("enable_conversation_memory", value)} />
                </SettingRow>
                <SettingRow label="Honcho Memory">
                    <Toggle value={settings.enable_honcho_memory} onChange={(value) => updateSetting("enable_honcho_memory", value)} />
                </SettingRow>
                {settings.enable_honcho_memory ? (
                    <>
                        <SettingRow label="Honcho API Key">
                            <PasswordInput value={settings.honcho_api_key} onChange={(value) => updateSetting("honcho_api_key", value)} placeholder="hc_..." />
                        </SettingRow>
                        <SettingRow label="Honcho Base URL">
                            <TextInput value={settings.honcho_base_url} onChange={(value) => updateSetting("honcho_base_url", value)} placeholder="Leave blank for managed cloud" />
                        </SettingRow>
                        <SettingRow label="Honcho Workspace">
                            <TextInput value={settings.honcho_workspace_id} onChange={(value) => updateSetting("honcho_workspace_id", value)} placeholder="tamux" />
                        </SettingRow>
                    </>
                ) : null}
                <SettingRow label="Chat Font Family">
                    <TextInput value={settings.chatFontFamily} onChange={(value) => updateSetting("chatFontFamily", value)} />
                </SettingRow>
                <SettingRow label="Chat Font Size">
                    <NumberInput value={settings.chatFontSize} min={10} max={24}
                        onChange={(value) => updateSetting("chatFontSize", value)} />
                </SettingRow>
            </Section>

            <Section title="Self-Orchestrating">
                <SettingRow label="Anticipatory Support">
                    <Toggle value={settings.anticipatory_enabled} onChange={(value) => updateSetting("anticipatory_enabled", value)} />
                </SettingRow>
                {settings.anticipatory_enabled ? (
                    <>
                        <SettingRow label="Morning Brief">
                            <Toggle value={settings.anticipatory_morning_brief} onChange={(value) => updateSetting("anticipatory_morning_brief", value)} />
                        </SettingRow>
                        <SettingRow label="Predictive Hydration">
                            <Toggle value={settings.anticipatory_predictive_hydration} onChange={(value) => updateSetting("anticipatory_predictive_hydration", value)} />
                        </SettingRow>
                        <SettingRow label="Stuck Detection">
                            <Toggle value={settings.anticipatory_stuck_detection} onChange={(value) => updateSetting("anticipatory_stuck_detection", value)} />
                        </SettingRow>
                    </>
                ) : null}
                <SettingRow label="Operator Model">
                    <Toggle value={settings.operator_model_enabled} onChange={(value) => updateSetting("operator_model_enabled", value)} />
                </SettingRow>
                {settings.operator_model_enabled ? (
                    <>
                        <SettingRow label="Message Statistics">
                            <Toggle value={settings.operator_model_allow_message_statistics} onChange={(value) => updateSetting("operator_model_allow_message_statistics", value)} />
                        </SettingRow>
                        <SettingRow label="Approval Learning">
                            <Toggle value={settings.operator_model_allow_approval_learning} onChange={(value) => updateSetting("operator_model_allow_approval_learning", value)} />
                        </SettingRow>
                        <SettingRow label="Attention Tracking">
                            <Toggle value={settings.operator_model_allow_attention_tracking} onChange={(value) => updateSetting("operator_model_allow_attention_tracking", value)} />
                        </SettingRow>
                        <SettingRow label="Implicit Feedback">
                            <Toggle value={settings.operator_model_allow_implicit_feedback} onChange={(value) => updateSetting("operator_model_allow_implicit_feedback", value)} />
                        </SettingRow>
                    </>
                ) : null}
                <SettingRow label="Collaboration">
                    <Toggle value={settings.collaboration_enabled} onChange={(value) => updateSetting("collaboration_enabled", value)} />
                </SettingRow>
                <SettingRow label="Compliance Mode">
                    <select value={settings.compliance_mode}
                        onChange={(e) => updateSetting("compliance_mode", e.target.value as AgentSettings["compliance_mode"])}
                        style={inputStyle}>
                        <option value="standard">Standard</option>
                        <option value="soc2">SOC 2</option>
                        <option value="hipaa">HIPAA</option>
                        <option value="fedramp">FedRAMP</option>
                    </select>
                </SettingRow>
                <SettingRow label="Retention Days">
                    <NumberInput value={settings.compliance_retention_days} min={1} max={3650}
                        onChange={(value) => updateSetting("compliance_retention_days", value)} />
                </SettingRow>
                <SettingRow label="Sign All Events">
                    <Toggle value={settings.compliance_sign_all_events} onChange={(value) => updateSetting("compliance_sign_all_events", value)} />
                </SettingRow>
                <SettingRow label="Tool Synthesis">
                    <Toggle value={settings.tool_synthesis_enabled} onChange={(value) => updateSetting("tool_synthesis_enabled", value)} />
                </SettingRow>
                {settings.tool_synthesis_enabled ? (
                    <>
                        <SettingRow label="Require Activation">
                            <Toggle value={settings.tool_synthesis_require_activation} onChange={(value) => updateSetting("tool_synthesis_require_activation", value)} />
                        </SettingRow>
                        <SettingRow label="Generated Tool Limit">
                            <NumberInput value={settings.tool_synthesis_max_generated_tools} min={1} max={200}
                                onChange={(value) => updateSetting("tool_synthesis_max_generated_tools", value)} />
                        </SettingRow>
                    </>
                ) : null}
            </Section>

            <Section title="Context Compaction">
                <SettingRow label="Auto Compact">
                    <Toggle value={settings.auto_compact_context} onChange={(value) => updateSetting("auto_compact_context", value)} />
                </SettingRow>
                <SettingRow label="Max Context Messages">
                    <NumberInput value={settings.max_context_messages} min={10} max={500}
                        onChange={(value) => updateSetting("max_context_messages", value)} />
                </SettingRow>
                <SettingRow label="Max Tool Loops">
                    <NumberInput value={settings.max_tool_loops} min={0} max={1000}
                        onChange={(value) => updateSetting("max_tool_loops", value)} />
                </SettingRow>
                <SettingRow label="429 Max Retries">
                    <NumberInput value={settings.max_retries} min={0} max={10}
                        onChange={(value) => updateSetting("max_retries", value)} />
                </SettingRow>
                <SettingRow label="429 Retry Delay (ms)">
                    <NumberInput value={settings.retry_delay_ms} min={100} max={60000} step={100}
                        onChange={(value) => updateSetting("retry_delay_ms", value)} />
                </SettingRow>
                <SettingRow label="Budget Tokens">
                    <NumberInput value={settings.context_budget_tokens} min={10000} max={500000} step={10000}
                        onChange={(value) => updateSetting("context_budget_tokens", value)} />
                </SettingRow>
                <SettingRow label="Compact Threshold %">
                    <NumberInput value={settings.compact_threshold_pct} min={50} max={95}
                        onChange={(value) => updateSetting("compact_threshold_pct", value)} />
                </SettingRow>
                <SettingRow label="Keep Recent on Compact">
                    <NumberInput value={settings.keep_recent_on_compact} min={1} max={50}
                        onChange={(value) => updateSetting("keep_recent_on_compact", value)} />
                </SettingRow>
            </Section>

            <div style={{ marginTop: 12 }}>
                <button onClick={resetSettings} style={addBtnStyle}>Reset Agent Settings</button>
            </div>
        </>
    );
}
