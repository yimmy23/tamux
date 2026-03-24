import { useEffect, useState } from "react";
import { isCDUIEnabled, setCDUIEnabled } from "../../lib/cduiMode";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle } from "../ui";

function defaultViewsPathLabel(): string {
  if (typeof navigator !== "undefined") {
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes("windows")) {
      return "%LOCALAPPDATA%\\tamux\\views";
    }
    if (ua.includes("mac")) {
      return "~/Library/Application Support/tamux/views";
    }
  }
  return "~/.tamux/views";
}

export function AboutTab() {
  const [cduiEnabled, setCduiEnabledState] = useState<boolean>(() => isCDUIEnabled());
  const [viewsPathLabel, setViewsPathLabel] = useState<string>(() => defaultViewsPathLabel());

  useEffect(() => {
    const bridge = (window as any).tamux ?? (window as any).amux;
    if (!bridge?.getDataDir) return;
    bridge
      .getDataDir()
      .then((dataDir: string) => {
        if (typeof dataDir !== "string" || !dataDir.trim()) return;
        const separator = dataDir.includes("\\") ? "\\" : "/";
        setViewsPathLabel(`${dataDir}${separator}views`);
      })
      .catch(() => {});
  }, []);

  const applyRuntimeMode = () => {
    setCDUIEnabled(cduiEnabled);
    window.location.reload();
  };

  return (
    <div className="grid gap-[var(--space-4)]">
      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            <CardTitle>Runtime Mode</CardTitle>
            <Badge variant={cduiEnabled ? "accent" : "default"}>{cduiEnabled ? "CDUI enabled" : "Legacy UI"}</Badge>
          </div>
          <CardDescription>
            CDUI remains the coherent default. Toggle the interface mode here without touching the underlying runtime wiring.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-[var(--space-4)]">
          <div className="flex flex-wrap items-center justify-between gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--muted)]/70 p-[var(--space-3)]">
            <div className="grid gap-[var(--space-1)]">
              <span className="text-[var(--text-sm)] font-medium text-[var(--text-primary)]">Use New CDUI</span>
              <span className="text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
                Reload into the YAML-driven experience or switch back to the legacy UI. Views are read from {viewsPathLabel}.
              </span>
            </div>
            <Button variant={cduiEnabled ? "primary" : "outline"} size="sm" onClick={() => setCduiEnabledState((value) => !value)}>
              {cduiEnabled ? "Enabled" : "Disabled"}
            </Button>
          </div>
          <div className="flex flex-wrap gap-[var(--space-2)]">
            <Button variant="outline" size="sm" onClick={() => void ((window as any).tamux ?? (window as any).amux)?.revealDataPath?.("views")}>
              Open {viewsPathLabel}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                window.dispatchEvent(new Event("tamux-cdui-views-reload"));
                window.dispatchEvent(new Event("amux-cdui-views-reload"));
              }}
            >
              Reload Views
            </Button>
            <Button variant="primary" size="sm" onClick={applyRuntimeMode}>
              Apply & Reload
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                window.dispatchEvent(new Event("tamux-open-setup-onboarding"));
                window.dispatchEvent(new Event("amux-open-setup-onboarding"));
              }}
            >
              Open Setup Assistant
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            <CardTitle>About</CardTitle>
            <Badge variant="mission">tamux 0.1.10</Badge>
          </div>
          <CardDescription>
            Desktop runtime details for the Electron + React frontend and Rust daemon pairing.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-[var(--space-3)] text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
          <p className="font-medium text-[var(--text-primary)]">tamux - Terminal Multiplexer</p>
          <p>
            A cross-platform terminal multiplexer with workspaces, surfaces, pane management, AI agent integration,
            snippet library, and session persistence.
          </p>
          <p>Built with Electron, React, xterm.js, and a Rust daemon.</p>
        </CardContent>
      </Card>
    </div>
  );
}
