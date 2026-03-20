import { useEffect, useState } from "react";
import type { AgentProviderConfig, AgentProviderId, AgentSettings } from "../../lib/agentStore";
import { getDefaultApiTransport, getDefaultAuthSource, getEffectiveContextWindow, getProviderDefinition, getSupportedApiTransports, getSupportedAuthSources } from "../../lib/agentStore";
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

    const providerConfig = settings[settings.activeProvider] as AgentProviderConfig;
    const providerDef = getProviderDefinition(settings.activeProvider);
    const supportedTransports = getSupportedApiTransports(settings.activeProvider);
    const supportedAuthSources = getSupportedAuthSources(settings.activeProvider);
    const isCustomProvider = settings.activeProvider === "custom";
    const showUrlEditor = isCustomProvider || useCustomUrl || Boolean(providerConfig.baseUrl && providerConfig.baseUrl !== providerDef?.defaultBaseUrl);
    const effectiveContextWindow = getEffectiveContextWindow(settings.activeProvider, providerConfig);

    useEffect(() => {
        if (settings.activeProvider !== "openai" || providerConfig.authSource !== "chatgpt_subscription") {
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
    }, [settings.activeProvider, providerConfig.authSource]);

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
            setSubscriptionAuthUrl(typeof result?.authUrl === "string" ? result.authUrl : null);
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
            <Section title="General">
                <SettingRow label="Enable Agent">
                    <Toggle value={settings.enabled} onChange={(value) => updateSetting("enabled", value)} />
                </SettingRow>
                <SettingRow label="Agent Backend">
                    <select value={settings.agentBackend}
                        onChange={(e) => updateSetting("agentBackend", e.target.value as "daemon" | "openclaw" | "hermes" | "legacy")}
                        style={inputStyle}>
                        <option value="daemon">tamux</option>
                        <option value="openclaw">OpenClaw</option>
                        <option value="hermes">Hermes</option>
                        <option value="legacy">Legacy (frontend)</option>
                    </select>
                </SettingRow>
                {(settings.agentBackend === "openclaw" || settings.agentBackend === "hermes") ? (
                    <div style={{ marginTop: 4, marginBottom: 8, fontSize: 11, color: "var(--text-secondary)", lineHeight: 1.4 }}>
                        <strong>{settings.agentBackend === "openclaw" ? "OpenClaw" : "Hermes"}</strong> will handle LLM inference, tools, memory, and gateway connections using its own infrastructure.
                        {settings.agentBackend === "hermes" ? (
                            <span> Config: <code>~/.hermes/config.yaml</code></span>
                        ) : (
                            <span> Config: <code>~/.openclaw/openclaw.json</code></span>
                        )}
                        <div style={{ marginTop: 6, padding: "4px 6px", background: "var(--bg-secondary)", borderRadius: 3 }}>
                            <strong>tamux tools:</strong> Add <code>tamux-mcp</code> to {settings.agentBackend === "hermes" ? "Hermes" : "OpenClaw"}'s MCP config for terminal session access, command execution, history search, and more.
                            <div style={{ marginTop: 3, fontFamily: "monospace", fontSize: 10 }}>
                                {`{"mcpServers": {"tamux": {"command": "tamux-mcp"}}}`}
                            </div>
                        </div>
                    </div>
                ) : null}
                <SettingRow label="Agent Name">
                    <TextInput value={settings.agentName} onChange={(value) => updateSetting("agentName", value)} />
                </SettingRow>
                <SettingRow label="Handler Prefix">
                    <TextInput value={settings.handler} onChange={(value) => updateSetting("handler", value)} />
                </SettingRow>
                <SettingRow label="System Prompt">
                    <textarea value={settings.systemPrompt}
                        onChange={(event) => updateSetting("systemPrompt", event.target.value)}
                        rows={4}
                        style={{ ...inputStyle, width: "100%", resize: "vertical", fontFamily: "inherit" }} />
                </SettingRow>
            </Section>

            {settings.agentBackend !== "openclaw" && settings.agentBackend !== "hermes" ? (
                <Section title="Provider">
                    <SettingRow label="Active Provider">
                        <SelectInput value={settings.activeProvider}
                            options={providerOptions.map((provider) => provider.id)}
                            onChange={(value) => updateSetting("activeProvider", value as AgentProviderId)} />
                    </SettingRow>

                    <div style={{ marginTop: 6, marginBottom: 6, fontSize: 11, color: "var(--text-secondary)" }}>
                        {providerOptions.find((provider) => provider.id === settings.activeProvider)?.label}
                    </div>

                    {showUrlEditor ? (
                        <SettingRow label="Base URL">
                            <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                                <TextInput value={providerConfig.baseUrl}
                                    onChange={(value) => updateSetting(settings.activeProvider, { ...providerConfig, baseUrl: value })}
                                    placeholder={providerDef?.defaultBaseUrl} />
                                {!isCustomProvider && (
                                    <button type="button" onClick={() => {
                                        updateSetting(settings.activeProvider, { ...providerConfig, baseUrl: "" });
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
                            providerId={settings.activeProvider}
                            value={providerConfig.model}
                            onChange={(value) => updateSetting(settings.activeProvider, { ...providerConfig, model: value })}
                            baseUrl={providerConfig.baseUrl || providerDef?.defaultBaseUrl}
                            apiKey={providerConfig.apiKey}
                        />
                    </SettingRow>
                    {providerDef?.apiType === "openai" ? (
                        <SettingRow label="Auth">
                            <select
                                value={providerConfig.authSource}
                                onChange={(e) => updateSetting(settings.activeProvider, {
                                    ...providerConfig,
                                    authSource: supportedAuthSources.includes(e.target.value as any)
                                      ? e.target.value as AgentProviderConfig["authSource"]
                                      : getDefaultAuthSource(settings.activeProvider),
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
                    {providerConfig.authSource === "api_key" ? (
                        <SettingRow label="API Key">
                            <PasswordInput value={providerConfig.apiKey}
                                onChange={(value) => updateSetting(settings.activeProvider, { ...providerConfig, apiKey: value })}
                                placeholder="Provider API key" />
                        </SettingRow>
                    ) : null}
                    {settings.activeProvider === "openai" && providerConfig.authSource === "chatgpt_subscription" ? (
                        <SettingRow label="ChatGPT Auth">
                            <div style={{ display: "grid", gap: 6, width: "100%" }}>
                                <div style={{ display: "flex", alignItems: "center", gap: 8, justifyContent: "flex-end" }}>
                                    <span style={{ fontSize: 11, color: subscriptionAuthStatus?.available ? "var(--success, #6ee7b7)" : "var(--text-secondary)" }}>
                                        {subscriptionAuthStatus?.available
                                            ? `Connected (${subscriptionAuthStatus.source || subscriptionAuthStatus.authMode || "tamux"})`
                                            : subscriptionAuthStatus?.error || "No ChatGPT subscription auth found"}
                                    </span>
                                    <button type="button" onClick={triggerSubscriptionAuth} style={smallBtnStyle} disabled={subscriptionAuthBusy}>
                                        {subscriptionAuthBusy ? "Preparing..." : "Get Link"}
                                    </button>
                                    {subscriptionAuthStatus?.available ? (
                                        <button type="button" onClick={clearSubscriptionAuth} style={smallBtnStyle} disabled={subscriptionAuthBusy}>
                                            Clear
                                        </button>
                                    ) : null}
                                </div>
                                {subscriptionAuthUrl ? (
                                    <a
                                        href={subscriptionAuthUrl}
                                        target="_blank"
                                        rel="noreferrer"
                                        style={{ fontSize: 11, color: "var(--accent, #60a5fa)", wordBreak: "break-all", textAlign: "right" }}
                                    >
                                        {subscriptionAuthUrl}
                                    </a>
                                ) : null}
                                {subscriptionAuthUrl ? (
                                    <div style={{ fontSize: 11, color: "var(--text-secondary)", textAlign: "right" }}>
                                        Open the link above and complete ChatGPT authentication. This row updates automatically after the callback returns.
                                    </div>
                                ) : null}
                            </div>
                        </SettingRow>
                    ) : null}
                    {providerDef?.apiType === "openai" ? (
                        <SettingRow label="API Transport">
                            <select
                                value={providerConfig.apiTransport}
                                onChange={(e) => updateSetting(settings.activeProvider, {
                                    ...providerConfig,
                                    apiTransport: supportedTransports.includes(e.target.value as any)
                                      ? (e.target.value as AgentProviderConfig["apiTransport"])
                                      : getDefaultApiTransport(settings.activeProvider),
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
                    {providerConfig.apiTransport === "native_assistant" ? (
                        <SettingRow label="Assistant ID">
                            <TextInput
                                value={providerConfig.assistantId}
                                onChange={(value) => updateSetting(settings.activeProvider, {
                                    ...providerConfig,
                                    assistantId: value,
                                })}
                                placeholder="asst_..."
                            />
                        </SettingRow>
                    ) : null}
                    <SettingRow label="Context Length">
                        {isCustomProvider ? (
                            <NumberInput
                                value={providerConfig.customContextWindowTokens ?? 128000}
                                min={16000}
                                max={2000000}
                                step={1000}
                                onChange={(value) => updateSetting(settings.activeProvider, {
                                    ...providerConfig,
                                    customContextWindowTokens: Math.max(1000, Math.trunc(value)),
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
                        <select value={settings.reasoningEffort}
                            onChange={(e) => updateSetting("reasoningEffort", e.target.value as AgentSettings["reasoningEffort"])}
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
                    <Toggle value={settings.enableBashTool} onChange={(value) => updateSetting("enableBashTool", value)} />
                </SettingRow>
                <SettingRow label="Vision Tool">
                    <Toggle value={settings.enableVisionTool} onChange={(value) => updateSetting("enableVisionTool", value)} />
                </SettingRow>
                <SettingRow label="Web Browsing Tool">
                    <Toggle value={settings.enableWebBrowsingTool} onChange={(value) => updateSetting("enableWebBrowsingTool", value)} />
                </SettingRow>
                <SettingRow label="Bash Timeout (s)">
                    <NumberInput value={settings.bashTimeoutSeconds} min={5} max={300}
                        onChange={(value) => updateSetting("bashTimeoutSeconds", value)} />
                </SettingRow>
                <SettingRow label="Web Search Tool">
                    <Toggle value={settings.enableWebSearchTool} onChange={(value) => updateSetting("enableWebSearchTool", value)} />
                </SettingRow>
                {settings.enableWebSearchTool ? (
                    <>
                        <SettingRow label="Search Provider">
                            <SelectInput
                                value={settings.searchToolProvider}
                                options={["none", "firecrawl", "exa", "tavily"]}
                                onChange={(value) => updateSetting("searchToolProvider", value as "none" | "firecrawl" | "exa" | "tavily")}
                            />
                        </SettingRow>
                        <SettingRow label="Firecrawl API Key">
                            <PasswordInput value={settings.firecrawlApiKey} onChange={(value) => updateSetting("firecrawlApiKey", value)} placeholder="fc-..." />
                        </SettingRow>
                        <SettingRow label="Exa API Key">
                            <PasswordInput value={settings.exaApiKey} onChange={(value) => updateSetting("exaApiKey", value)} placeholder="exa_..." />
                        </SettingRow>
                        <SettingRow label="Tavily API Key">
                            <PasswordInput value={settings.tavilyApiKey} onChange={(value) => updateSetting("tavilyApiKey", value)} placeholder="tvly-..." />
                        </SettingRow>
                        <SettingRow label="Search Max Results">
                            <NumberInput value={settings.searchMaxResults} min={1} max={20}
                                onChange={(value) => updateSetting("searchMaxResults", value)} />
                        </SettingRow>
                        <SettingRow label="Search Timeout (s)">
                            <NumberInput value={settings.searchTimeoutSeconds} min={3} max={120}
                                onChange={(value) => updateSetting("searchTimeoutSeconds", value)} />
                        </SettingRow>
                    </>
                ) : null}
            </Section>

            <Section title="Chat">
                <SettingRow label="Streaming">
                    <Toggle value={settings.enableStreaming} onChange={(value) => updateSetting("enableStreaming", value)} />
                </SettingRow>
                <SettingRow label="Conversation Memory">
                    <Toggle value={settings.enableConversationMemory} onChange={(value) => updateSetting("enableConversationMemory", value)} />
                </SettingRow>
                <SettingRow label="Honcho Memory">
                    <Toggle value={settings.enableHonchoMemory} onChange={(value) => updateSetting("enableHonchoMemory", value)} />
                </SettingRow>
                {settings.enableHonchoMemory ? (
                    <>
                        <SettingRow label="Honcho API Key">
                            <PasswordInput value={settings.honchoApiKey} onChange={(value) => updateSetting("honchoApiKey", value)} placeholder="hc_..." />
                        </SettingRow>
                        <SettingRow label="Honcho Base URL">
                            <TextInput value={settings.honchoBaseUrl} onChange={(value) => updateSetting("honchoBaseUrl", value)} placeholder="Leave blank for managed cloud" />
                        </SettingRow>
                        <SettingRow label="Honcho Workspace">
                            <TextInput value={settings.honchoWorkspaceId} onChange={(value) => updateSetting("honchoWorkspaceId", value)} placeholder="tamux" />
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

            <Section title="Context Compaction">
                <SettingRow label="Auto Compact">
                    <Toggle value={settings.autoCompactContext} onChange={(value) => updateSetting("autoCompactContext", value)} />
                </SettingRow>
                <SettingRow label="Max Context Messages">
                    <NumberInput value={settings.maxContextMessages} min={10} max={500}
                        onChange={(value) => updateSetting("maxContextMessages", value)} />
                </SettingRow>
                <SettingRow label="Max Tool Loops">
                    <NumberInput value={settings.maxToolLoops} min={0} max={1000}
                        onChange={(value) => updateSetting("maxToolLoops", value)} />
                </SettingRow>
                <SettingRow label="429 Max Retries">
                    <NumberInput value={settings.maxRetries} min={0} max={10}
                        onChange={(value) => updateSetting("maxRetries", value)} />
                </SettingRow>
                <SettingRow label="429 Retry Delay (ms)">
                    <NumberInput value={settings.retryDelayMs} min={100} max={60000} step={100}
                        onChange={(value) => updateSetting("retryDelayMs", value)} />
                </SettingRow>
                <SettingRow label="Budget Tokens">
                    <NumberInput value={settings.contextBudgetTokens} min={10000} max={500000} step={10000}
                        onChange={(value) => updateSetting("contextBudgetTokens", value)} />
                </SettingRow>
                <SettingRow label="Compact Threshold %">
                    <NumberInput value={settings.compactThresholdPercent} min={50} max={95}
                        onChange={(value) => updateSetting("compactThresholdPercent", value)} />
                </SettingRow>
                <SettingRow label="Keep Recent on Compact">
                    <NumberInput value={settings.keepRecentOnCompaction} min={1} max={50}
                        onChange={(value) => updateSetting("keepRecentOnCompaction", value)} />
                </SettingRow>
            </Section>

            <div style={{ marginTop: 12 }}>
                <button onClick={resetSettings} style={addBtnStyle}>Reset Agent Settings</button>
            </div>
        </>
    );
}
