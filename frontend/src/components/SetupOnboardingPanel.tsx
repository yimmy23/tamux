import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import { cn, overlayClassName, panelSurfaceClassName } from "./ui/shared";

const SETUP_PANEL_STATE_KEY = "tamux-setup-onboarding-state-v1";
const SETUP_PANEL_VERSION = "1";
const OPEN_EVENTS = ["tamux-open-setup-onboarding", "amux-open-setup-onboarding"] as const;

type SetupPanelState = {
  seenVersion?: string;
  dismissedAt?: number;
};

function readSetupPanelState(): SetupPanelState {
  try {
    const raw = window.localStorage.getItem(SETUP_PANEL_STATE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return {};
    return parsed as SetupPanelState;
  } catch {
    return {};
  }
}

function writeSetupPanelState(next: SetupPanelState): void {
  try {
    window.localStorage.setItem(SETUP_PANEL_STATE_KEY, JSON.stringify(next));
  } catch {
    // Ignore localStorage failures.
  }
}

function bridge(): AmuxBridge | null {
  return (window.tamux ?? window.amux) ?? null;
}

export function SetupOnboardingPanel() {
  const [report, setReport] = useState<AmuxSetupPrereqReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [forcedOpen, setForcedOpen] = useState(false);
  const [panelState, setPanelState] = useState<SetupPanelState>(() => readSetupPanelState());

  const refresh = useCallback(async () => {
    const amux = bridge();
    if (!amux?.checkSetupPrereqs) {
      setLoading(false);
      setReport(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const next = await amux.checkSetupPrereqs("desktop");
      setReport(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const openPanel = () => setForcedOpen(true);
    OPEN_EVENTS.forEach((eventName) => window.addEventListener(eventName, openPanel));
    return () => {
      OPEN_EVENTS.forEach((eventName) => window.removeEventListener(eventName, openPanel));
    };
  }, []);

  const shouldShow = useMemo(() => {
    if (forcedOpen) return true;
    if (!report) return false;
    return panelState.seenVersion !== SETUP_PANEL_VERSION;
  }, [forcedOpen, panelState.seenVersion, report]);

  const dismiss = useCallback(() => {
    const next: SetupPanelState = {
      seenVersion: SETUP_PANEL_VERSION,
      dismissedAt: Date.now(),
    };
    writeSetupPanelState(next);
    setPanelState(next);
    setForcedOpen(false);
  }, []);

  const openGuide = useCallback(async () => {
    if (!report?.gettingStartedPath) return;
    const amux = bridge() as any;
    if (typeof amux?.openFsPath === "function") {
      await amux.openFsPath(report.gettingStartedPath);
    }
  }, [report?.gettingStartedPath]);

  if (!shouldShow) return null;

  return (
    <div
      className={overlayClassName}
      style={{
        zIndex: 4100,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 24,
      }}
    >
      <div
        className={cn(panelSurfaceClassName, "overflow-hidden")}
        style={{
          width: "min(980px, 96vw)",
          maxHeight: "90vh",
          overflow: "auto",
          padding: 20,
          display: "grid",
          gap: 14,
          background: "color-mix(in srgb, var(--card) 94%, var(--bg-overlay))",
          boxShadow: "var(--shadow-lg)",
        }}
      >
        <div style={{ display: "grid", gap: 4 }}>
          <div style={{ fontSize: 12, letterSpacing: "0.08em", textTransform: "uppercase", color: "var(--text-secondary)" }}>
            First-run setup
          </div>
          <h2 style={{ margin: 0, fontSize: 24 }}>tamux Setup Assistant</h2>
          <div style={{ fontSize: 13, color: "var(--text-secondary)", lineHeight: 1.5 }}>
            {report?.whatIsTamux ?? "tamux is an AI-native terminal multiplexer with a Rust daemon and agent workflows."}
          </div>
        </div>

        {loading ? (
          <div style={{ color: "var(--text-secondary)", fontSize: 13 }}>Checking dependencies...</div>
        ) : null}

        {error ? (
          <div
            style={{
              border: "1px solid var(--danger-border)",
              borderRadius: "var(--radius-md)",
              padding: 10,
              fontSize: 13,
              color: "var(--danger)",
              background: "var(--danger-soft)",
            }}
          >
            Setup check failed: {error}
          </div>
        ) : null}

        {report ? (
          <>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 10 }}>
              <InfoCard label="Install Root" value={report.installRoot} />
              <InfoCard label="Daemon Path" value={report.daemonPath} />
              <InfoCard label="Data Directory" value={report.dataDir} />
            </div>

            <div
              style={{
                border: "1px solid var(--border)",
                borderRadius: "var(--radius-lg)",
                padding: 12,
                display: "grid",
                gap: 10,
                background: "var(--bg-secondary)",
              }}
            >
              <div style={{ fontSize: 13, fontWeight: 700 }}>
                Required runtime dependencies ({report.required.length})
              </div>
              {report.required.length === 0 ? (
                <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                  No hard blockers for this runtime profile. Optional integrations are listed below.
                </div>
              ) : null}
              {report.required.map((dep) => (
                <div
                  key={dep.name}
                  style={{
                    border: `1px solid ${dep.found ? "var(--success-border)" : "var(--warning-border)"}`,
                    borderRadius: "var(--radius-md)",
                    padding: 10,
                    display: "grid",
                    gap: 6,
                    background: dep.found ? "var(--success-soft)" : "var(--warning-soft)",
                  }}
                >
                  <div style={{ display: "flex", justifyContent: "space-between", gap: 10 }}>
                    <span style={{ fontWeight: 700 }}>{dep.label}</span>
                    <span style={{ color: dep.found ? "var(--success)" : "var(--warning)", fontSize: 12 }}>
                      {dep.found ? "installed" : "missing"}
                    </span>
                  </div>
                  {dep.path ? <code style={{ fontSize: 12 }}>{dep.path}</code> : null}
                  {!dep.found && dep.installHints.length > 0 ? (
                    <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                      Install: <code>{dep.installHints[0]}</code>
                    </div>
                  ) : null}
                </div>
              ))}
            </div>

            {report.optional.length > 0 ? (
              <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                Optional tools: {report.optional.map((dep) => dep.name).join(", ")}
              </div>
            ) : null}
          </>
        ) : null}

        <div style={{ display: "flex", justifyContent: "flex-end", gap: 10, flexWrap: "wrap" }}>
          <button type="button" onClick={() => void refresh()} style={actionButtonStyle}>
            Re-check
          </button>
          <button
            type="button"
            onClick={() => void openGuide()}
            style={actionButtonStyle}
            disabled={!report?.gettingStartedPath}
          >
            Open Getting Started
          </button>
          <button type="button" onClick={dismiss} style={actionButtonStyle}>
            Dismiss
          </button>
        </div>
      </div>
    </div>
  );
}

function InfoCard({ label, value }: { label: string; value: string }) {
  return (
    <div
      style={{
        border: "1px solid var(--border)",
        borderRadius: "var(--radius-lg)",
        padding: 10,
        display: "grid",
        gap: 4,
        background: "var(--bg-secondary)",
      }}
    >
      <span style={{ fontSize: 11, letterSpacing: "0.08em", textTransform: "uppercase", color: "var(--text-secondary)" }}>{label}</span>
      <code style={{ fontSize: 12 }}>{value}</code>
    </div>
  );
}

const actionButtonStyle: CSSProperties = {
  border: "1px solid var(--border)",
  background: "var(--bg-secondary)",
  color: "var(--text-primary)",
  borderRadius: "var(--radius-md)",
  padding: "8px 12px",
  cursor: "pointer",
  fontSize: 12,
  transition: "background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast)",
};
