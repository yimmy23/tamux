import { useEffect, useState } from "react";
import { useAgentStore } from "../../lib/agentStore";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, Input } from "../ui";

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
  const [chatgptAuthUrl, setChatgptAuthUrl] = useState<string | null>(null);
  const [chatgptAuthBusy, setChatgptAuthBusy] = useState(false);

  const openChatgptAuthUrl = (url: string) => {
    if (!url) return;
    const opened = typeof window !== "undefined" ? window.open(url, "_blank", "noopener,noreferrer") : null;
    if (opened) opened.opener = null;
  };

  useEffect(() => {
    refreshProviderAuthStates();
  }, [refreshProviderAuthStates]);

  useEffect(() => {
    if (!chatgptAuthUrl) return;
    const timer = window.setInterval(() => {
      const amux = (window as any).amux || (window as any).tamux;
      if (!amux?.openAICodexAuthStatus) return;
      void amux.openAICodexAuthStatus({ refresh: true }).then((status: any) => {
        if (status?.available) {
          setChatgptAuthUrl(null);
          refreshProviderAuthStates();
        }
      }).catch(() => {});
    }, 2000);
    return () => window.clearInterval(timer);
  }, [chatgptAuthUrl, refreshProviderAuthStates]);

  const filtered = providerAuthStates.filter((s) => !filter || s.provider_name.toLowerCase().includes(filter.toLowerCase()) || s.provider_id.toLowerCase().includes(filter.toLowerCase()));

  const handleLogin = async (providerId: string) => {
    if (!loginKey.trim()) return;
    const state = filtered.find((s) => s.provider_id === providerId);
    await loginProvider(providerId, loginKey, state?.base_url);
    setLoginKey("");
    setLoginTarget(null);
  };

  const handleChatgptLogin = async () => {
    const amux = (window as any).amux || (window as any).tamux;
    if (!amux?.openAICodexAuthLogin) return;
    setChatgptAuthBusy(true);
    try {
      const result = await amux.openAICodexAuthLogin();
      const authUrl = typeof result?.authUrl === "string" ? result.authUrl : null;
      setChatgptAuthUrl(authUrl);
      if (authUrl) openChatgptAuthUrl(authUrl);
      if (result?.available) refreshProviderAuthStates();
    } finally {
      setChatgptAuthBusy(false);
    }
  };

  const handleTest = async (providerId: string) => {
    setValidating(providerId);
    const state = filtered.find((s) => s.provider_id === providerId);
    try {
      if (providerId === "openai" && state?.auth_source === "chatgpt_subscription") {
        const amux = (window as any).amux || (window as any).tamux;
        if (!amux?.openAICodexAuthStatus) {
          setValidationResult((prev) => ({ ...prev, [providerId]: { valid: false, error: "ChatGPT auth bridge unavailable" } }));
          return;
        }
        const status = await amux.openAICodexAuthStatus({ refresh: true });
        setValidationResult((prev) => ({
          ...prev,
          [providerId]: status?.available ? { valid: true } : { valid: false, error: status?.error || "ChatGPT subscription auth not available" },
        }));
        return;
      }

      const result = await validateProvider(providerId, state?.base_url || "", "", state?.auth_source || "api_key");
      setValidationResult((prev) => ({ ...prev, [providerId]: result }));
    } finally {
      setValidating(null);
    }
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <CardTitle>Provider Authentication</CardTitle>
          <Badge variant="accent">Daemon-backed</Badge>
        </div>
        <CardDescription>Keep provider login flows and validation intact while moving the auth roster onto redesign surfaces.</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-[var(--space-4)]">
        <Input type="text" placeholder="Filter providers..." value={filter} onChange={(e) => setFilter(e.target.value)} />
        {providerAuthStates.length === 0 ? <div className="text-[var(--text-sm)] text-[var(--text-secondary)]">Loading providers from daemon...</div> : null}
        <div className="grid gap-[var(--space-3)]">
          {filtered.map((state) => {
            const isExpanded = loginTarget === state.provider_id;
            const vr = validationResult[state.provider_id];
            const isOpenAI = state.provider_id === "openai";
            return (
              <div key={state.provider_id} className="grid gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--muted)]/50 p-[var(--space-3)]">
                <div className="flex flex-wrap items-center justify-between gap-[var(--space-3)]">
                  <div className="flex min-w-0 flex-wrap items-center gap-[var(--space-2)]">
                    <Badge variant={state.authenticated ? "success" : "default"}>{state.provider_name}</Badge>
                    <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{state.provider_id}</span>
                    {state.authenticated ? <Badge variant="accent">{state.model || "authenticated"}</Badge> : null}
                  </div>
                  <div className="flex flex-wrap gap-[var(--space-2)]">
                    {state.authenticated ? (
                      <>
                        <Button variant="outline" size="sm" onClick={() => handleTest(state.provider_id)} disabled={validating === state.provider_id}>
                          {validating === state.provider_id ? "Testing..." : "Test"}
                        </Button>
                        <Button variant="destructive" size="sm" onClick={() => isOpenAI ? void ((window as any).amux || (window as any).tamux)?.openAICodexAuthLogout?.().then(() => refreshProviderAuthStates()) : void logoutProvider(state.provider_id)}>
                          Logout
                        </Button>
                      </>
                    ) : (
                      <>
                        <Button variant="outline" size="sm" onClick={() => { setLoginTarget(isExpanded ? null : state.provider_id); setLoginKey(""); }}>
                          {isExpanded ? "Cancel" : "API Key"}
                        </Button>
                        {isOpenAI ? <Button variant="primary" size="sm" onClick={handleChatgptLogin} disabled={chatgptAuthBusy}>{chatgptAuthBusy ? "..." : "ChatGPT Login"}</Button> : null}
                      </>
                    )}
                  </div>
                </div>
                {vr ? <div className={`text-[var(--text-sm)] ${vr.valid ? "text-[var(--success)]" : "text-[var(--danger)]"}`}>{vr.valid ? "Connection OK" : `Error: ${vr.error || "unknown"}`}</div> : null}
                {isOpenAI && chatgptAuthUrl ? (
                  <div className="text-[var(--text-sm)] text-[var(--accent)]">
                    <span>Auth URL: </span>
                    <a href={chatgptAuthUrl} target="_blank" rel="noreferrer" onClick={(event) => { event.preventDefault(); openChatgptAuthUrl(chatgptAuthUrl); }} className="underline">Open in browser</a>
                    <span className="ml-[var(--space-2)] text-[var(--text-secondary)]">Waiting for confirmation...</span>
                  </div>
                ) : null}
                {isExpanded ? (
                  <div className="flex flex-wrap items-center gap-[var(--space-2)]">
                    <Input type="password" placeholder={isOpenAI ? "OpenAI API Key" : "API Key"} value={loginKey} onChange={(e) => setLoginKey(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") void handleLogin(state.provider_id); }} autoFocus />
                    <Button variant="primary" size="sm" onClick={() => void handleLogin(state.provider_id)} disabled={!loginKey.trim()}>Save</Button>
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}
