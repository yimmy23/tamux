import { useEffect, useState, type CSSProperties } from "react";
import { usePluginStore, type PluginInfoItem, type PluginSettingField, type PluginSettingValue } from "../../lib/pluginStore";
import {
  Section,
  SettingRow,
  TextInput,
  PasswordInput,
  NumberInput,
  SelectInput,
  Toggle,
  smallBtnStyle,
} from "./shared";

// ---------------------------------------------------------------------------
// AuthStatusBadge
// ---------------------------------------------------------------------------

type AuthStatus = "not_configured" | "connected" | "expired";

const AUTH_DOT_COLORS: Record<AuthStatus, string> = {
  not_configured: "var(--text-muted)",
  connected: "var(--success)",
  expired: "var(--warning)",
};

const AUTH_LABELS: Record<AuthStatus, string> = {
  not_configured: "Not configured",
  connected: "Connected",
  expired: "Expired -- Reconnect",
};

function AuthStatusBadge({ hasAuth, authStatus = "not_configured" }: {
  hasAuth: boolean;
  authStatus?: AuthStatus;
}) {
  if (!hasAuth) return null;

  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 4, marginLeft: 8 }}>
      <span style={{
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: AUTH_DOT_COLORS[authStatus],
        flexShrink: 0,
      }} />
      <span style={{ fontSize: 12, color: AUTH_DOT_COLORS[authStatus] }}>
        {AUTH_LABELS[authStatus]}
      </span>
    </span>
  );
}

// ---------------------------------------------------------------------------
// PluginSettingsForm -- dynamic form from manifest schema
// ---------------------------------------------------------------------------

function PluginSettingsForm({ pluginName, schema, values }: {
  pluginName: string;
  schema: PluginSettingField[];
  values: PluginSettingValue[];
}) {
  const updateSetting = usePluginStore((s) => s.updateSetting);
  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});

  // Local state for text/password/number fields (saved on blur)
  const [localValues, setLocalValues] = useState<Record<string, string>>({});

  // Sync local values from store values
  useEffect(() => {
    const map: Record<string, string> = {};
    for (const v of values) {
      map[v.key] = v.value;
    }
    setLocalValues(map);
  }, [values]);

  function getFieldValue(field: PluginSettingField): string {
    const stored = localValues[field.key];
    if (stored !== undefined) return stored;
    if (field.default !== undefined) return String(field.default);
    return "";
  }

  function handleBlur(field: PluginSettingField, rawValue: string) {
    const errors = { ...validationErrors };

    if (field.required && rawValue.trim() === "") {
      errors[field.key] = "This field is required";
      setValidationErrors(errors);
      return;
    }

    if (field.type === "number" && rawValue.trim() !== "" && isNaN(Number(rawValue))) {
      errors[field.key] = "Enter a valid number";
      setValidationErrors(errors);
      return;
    }

    // Clear error
    delete errors[field.key];
    setValidationErrors(errors);

    void updateSetting(pluginName, field.key, rawValue, field.secret || field.type === "secret");
  }

  function handleImmediateChange(field: PluginSettingField, rawValue: string) {
    setLocalValues((prev) => ({ ...prev, [field.key]: rawValue }));
    void updateSetting(pluginName, field.key, rawValue, field.secret || field.type === "secret");
  }

  return (
    <Section title="Settings">
      {schema.map((field) => {
        const fieldValue = getFieldValue(field);
        const label = field.required ? `${field.label} *` : field.label;

        return (
          <div key={field.key}>
            <SettingRow label={label}>
              <div
                onBlur={() => {
                  if (field.type === "string" || field.type === "secret" || field.type === "number") {
                    handleBlur(field, localValues[field.key] ?? fieldValue);
                  }
                }}
              >
                {field.type === "string" && (
                  <TextInput
                    value={localValues[field.key] ?? fieldValue}
                    onChange={(v) => setLocalValues((prev) => ({ ...prev, [field.key]: v }))}
                    placeholder={field.description}
                  />
                )}
                {field.type === "secret" && (
                  <PasswordInput
                    value={localValues[field.key] ?? fieldValue}
                    onChange={(v) => setLocalValues((prev) => ({ ...prev, [field.key]: v }))}
                    placeholder={field.description}
                  />
                )}
                {field.type === "number" && (
                  <NumberInput
                    value={Number(localValues[field.key] ?? fieldValue) || 0}
                    onChange={(v) => setLocalValues((prev) => ({ ...prev, [field.key]: String(v) }))}
                  />
                )}
                {field.type === "select" && (
                  <SelectInput
                    value={(localValues[field.key] ?? fieldValue) || (field.options?.[0] ?? "")}
                    options={field.options ?? []}
                    onChange={(v) => handleImmediateChange(field, v)}
                  />
                )}
                {field.type === "boolean" && (
                  <Toggle
                    value={fieldValue === "true"}
                    onChange={(v) => handleImmediateChange(field, String(v))}
                  />
                )}
              </div>
            </SettingRow>
            {field.description && field.type !== "string" && field.type !== "secret" ? (
              <div style={{ fontSize: 12, color: "var(--text-muted)", paddingLeft: 0, paddingBottom: 4 }}>
                {field.description}
              </div>
            ) : null}
            {validationErrors[field.key] ? (
              <div style={{ fontSize: 12, color: "var(--danger)", paddingBottom: 4 }}>
                {validationErrors[field.key]}
              </div>
            ) : null}
          </div>
        );
      })}
    </Section>
  );
}

// ---------------------------------------------------------------------------
// PluginCard
// ---------------------------------------------------------------------------

const cardRowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  padding: "8px 0",
  cursor: "pointer",
};

function PluginCard({ plugin }: { plugin: PluginInfoItem }) {
  const [expanded, setExpanded] = useState(false);
  const toggleEnabled = usePluginStore((s) => s.toggleEnabled);
  const selectPlugin = usePluginStore((s) => s.selectPlugin);
  const selectedPlugin = usePluginStore((s) => s.selectedPlugin);
  const settingsSchema = usePluginStore((s) => s.settingsSchema);
  const settingsValues = usePluginStore((s) => s.settingsValues);
  const testConnection = usePluginStore((s) => s.testConnection);
  const testResult = usePluginStore((s) => s.testResult);
  const testLoading = usePluginStore((s) => s.testLoading);

  const isSelected = selectedPlugin === plugin.name;

  // Auto-clear success result after 5 seconds
  useEffect(() => {
    if (testResult?.success && isSelected) {
      const timer = setTimeout(() => {
        usePluginStore.setState({ testResult: null });
      }, 5000);
      return () => clearTimeout(timer);
    }
  }, [testResult, isSelected]);

  function handleRowClick(e: React.MouseEvent) {
    // Do not toggle expand when clicking the toggle switch
    const target = e.target as HTMLElement;
    if (target.closest("button")) return;

    const next = !expanded;
    setExpanded(next);
    if (next && !isSelected) {
      void selectPlugin(plugin.name);
    }
  }

  function handleToggle(enabled: boolean) {
    void toggleEnabled(plugin.name, enabled);
  }

  // Phase 16: auth status is always "not_configured" (OAuth flow is Phase 18)
  // eslint-disable-next-line prefer-const
  let authStatus: AuthStatus = "not_configured";

  return (
    <div style={{ borderBottom: "1px solid var(--border)" }}>
      {/* Collapsed row */}
      <div style={cardRowStyle} onClick={handleRowClick}>
        <div style={{ display: "flex", alignItems: "center" }}>
          <span style={{
            fontSize: 14,
            fontWeight: 600,
            color: "var(--text-primary)",
            opacity: plugin.enabled ? 1 : 0.6,
          }}>
            {plugin.name}
          </span>
          <span style={{ fontSize: 12, color: "var(--text-muted)", marginLeft: 8 }}>
            {plugin.version}
          </span>
          <AuthStatusBadge hasAuth={plugin.has_auth} authStatus={authStatus} />
        </div>
        <Toggle value={plugin.enabled} onChange={handleToggle} />
      </div>

      {/* Expanded view */}
      {expanded ? (
        <div style={{ paddingBottom: 12, paddingLeft: 0 }}>
          {/* Description */}
          {plugin.description ? (
            <div style={{ fontSize: 12, color: "var(--text-secondary)", lineHeight: 1.5, paddingTop: 8 }}>
              {plugin.description}
            </div>
          ) : null}

          {/* Settings form */}
          {isSelected && settingsSchema.length > 0 ? (
            <div style={{ paddingTop: 8 }}>
              <PluginSettingsForm
                pluginName={plugin.name}
                schema={settingsSchema}
                values={settingsValues}
              />
            </div>
          ) : null}

          {/* Test Connection */}
          {plugin.has_api ? (
            <div style={{ paddingTop: 8 }}>
              <button
                style={{
                  ...smallBtnStyle,
                  opacity: testLoading && isSelected ? 0.6 : 1,
                }}
                disabled={testLoading && isSelected}
                onClick={() => void testConnection(plugin.name)}
              >
                {testLoading && isSelected ? "Testing..." : "Test Connection"}
              </button>
              {isSelected && testResult ? (
                <div style={{
                  fontSize: 12,
                  color: testResult.success ? "var(--success)" : "var(--danger)",
                  paddingTop: 4,
                }}>
                  {testResult.success ? "Connection successful" : testResult.message}
                </div>
              ) : null}
            </div>
          ) : null}

          {/* Connect / Reconnect for auth plugins */}
          {plugin.has_auth ? (
            <div style={{ paddingTop: 8 }}>
              <button
                style={smallBtnStyle}
                onClick={() => {
                  // Per D-07: opens system browser for OAuth
                  window.open(`https://accounts.google.com/o/oauth2/auth?plugin=${encodeURIComponent(plugin.name)}`, "_blank");
                }}
              >
                {(authStatus as AuthStatus) === "expired" ? "Reconnect" : "Connect"}
              </button>
            </div>
          ) : null}

          {/* Metadata footer */}
          <div style={{ fontSize: 12, color: "var(--text-muted)", paddingTop: 12 }}>
            {plugin.install_source} &middot; installed {plugin.installed_at}
          </div>
        </div>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// PluginsTab (exported)
// ---------------------------------------------------------------------------

export function PluginsTab() {
  const plugins = usePluginStore((s) => s.plugins);
  const loading = usePluginStore((s) => s.loading);
  const fetchPlugins = usePluginStore((s) => s.fetchPlugins);

  useEffect(() => {
    void fetchPlugins();
  }, [fetchPlugins]);

  if (loading) return null;

  // Empty state
  if (plugins.length === 0) {
    return (
      <div style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        minHeight: 200,
        gap: 8,
      }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: "var(--text-primary)" }}>
          No plugins installed
        </div>
        <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
          Run <code style={{ fontFamily: "var(--font-mono)" }}>tamux plugin add &lt;name&gt;</code> to install a plugin, then configure it here.
        </div>
      </div>
    );
  }

  return (
    <div>
      {plugins.map((plugin) => (
        <PluginCard key={plugin.name} plugin={plugin} />
      ))}
    </div>
  );
}
