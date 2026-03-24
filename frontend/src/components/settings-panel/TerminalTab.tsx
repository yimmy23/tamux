import type { ReactNode } from "react";
import type { AmuxSettings, ShellProfile } from "../../lib/types";
import { Button, Card, CardContent, CardDescription, CardHeader, CardTitle, Input, Badge, fieldClassName } from "../ui";
import { NumberInput, SettingRow, type SettingsUpdater, SliderInput, TextInput, Toggle } from "./shared";

interface AvailableShell {
  name: string;
  path: string;
  args?: string;
}

function SettingsSection({ title, description, children, badge }: { title: string; description?: string; children: ReactNode; badge?: ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <CardTitle>{title}</CardTitle>
          {badge}
        </div>
        {description ? <CardDescription>{description}</CardDescription> : null}
      </CardHeader>
      <CardContent className="grid gap-[var(--space-2)]">{children}</CardContent>
    </Card>
  );
}

export function TerminalTab({
  settings,
  updateSetting,
  availableShells,
  profiles,
  addProfile,
  removeProfile,
  updateProfile,
  setDefaultProfile,
}: {
  settings: AmuxSettings;
  updateSetting: SettingsUpdater;
  availableShells: AvailableShell[];
  profiles: ShellProfile[];
  addProfile: (profile: ShellProfile) => void;
  removeProfile: (id: string) => void;
  updateProfile: (id: string, updates: Partial<ShellProfile>) => void;
  setDefaultProfile: (id: string) => void;
}) {
  const currentShellInList = availableShells.some((s) => s.path === settings.defaultShell);

  return (
    <div className="grid gap-[var(--space-4)]">
      <div className="grid gap-[var(--space-4)] xl:grid-cols-2">
        <SettingsSection title="Shell" description="Default shell selection and launch arguments for new sessions.">
          <SettingRow label="Default Shell">
            <select
              value={settings.defaultShell}
              onChange={(e) => {
                const selected = availableShells.find((s) => s.path === e.target.value);
                updateSetting("defaultShell", e.target.value);
                if (selected?.args && !settings.defaultShellArgs) {
                  updateSetting("defaultShellArgs", selected.args);
                }
              }}
              className={fieldClassName}
            >
              <option value="">(system default)</option>
              {availableShells.map((shell) => (
                <option key={`${shell.path}:${shell.args ?? ""}`} value={shell.path}>
                  {shell.name} — {shell.path}
                </option>
              ))}
              {settings.defaultShell && !currentShellInList ? <option value={settings.defaultShell}>{settings.defaultShell} (custom)</option> : null}
            </select>
          </SettingRow>
          <SettingRow label="Shell Arguments">
            <TextInput value={settings.defaultShellArgs} onChange={(value) => updateSetting("defaultShellArgs", value)} placeholder="" />
          </SettingRow>
        </SettingsSection>

        <SettingsSection title="Buffer" description="Terminal viewport comfort and shell interaction defaults.">
          <SettingRow label="Scrollback Lines">
            <NumberInput value={settings.scrollbackLines} min={1000} max={100000} step={1000} onChange={(value) => updateSetting("scrollbackLines", value)} />
          </SettingRow>
          <SettingRow label="Pane Padding">
            <SliderInput value={settings.padding} min={0} max={24} step={1} onChange={(value) => updateSetting("padding", value)} />
          </SettingRow>
          <SettingRow label="Visual Bell">
            <Toggle value={settings.visualBell} onChange={(value) => updateSetting("visualBell", value)} />
          </SettingRow>
          <SettingRow label="Bell Sound">
            <Toggle value={settings.bellSound} onChange={(value) => updateSetting("bellSound", value)} />
          </SettingRow>
          <SettingRow label="Bracketed Paste">
            <Toggle value={settings.bracketedPaste} onChange={(value) => updateSetting("bracketedPaste", value)} />
          </SettingRow>
          <SettingRow label="Cursor Blink Speed (ms)">
            <NumberInput value={settings.cursorBlinkMs} min={100} max={2000} step={10} onChange={(value) => updateSetting("cursorBlinkMs", value)} />
          </SettingRow>
        </SettingsSection>
      </div>

      <SettingsSection title="Shell Profiles" description="Named launch presets that keep profile editing on the existing settings store." badge={<Badge variant="timeline">{profiles.length} profiles</Badge>}>
        <div className="grid gap-[var(--space-3)]">
          {profiles.map((profile) => (
            <div key={profile.id} className="grid gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--muted)]/50 p-[var(--space-3)]">
              <div className="flex flex-wrap items-center justify-between gap-[var(--space-2)]">
                <div className="flex flex-wrap items-center gap-[var(--space-2)]">
                  <strong className="text-[var(--text-sm)] text-[var(--text-primary)]">{profile.name || "Unnamed profile"}</strong>
                  {profile.isDefault ? <Badge variant="accent">Default</Badge> : null}
                </div>
                <div className="flex flex-wrap gap-[var(--space-2)]">
                  {!profile.isDefault ? <Button variant="outline" size="sm" onClick={() => setDefaultProfile(profile.id)}>Set Default</Button> : null}
                  <Button variant="destructive" size="sm" onClick={() => removeProfile(profile.id)}>Delete</Button>
                </div>
              </div>
              <div className="grid gap-[var(--space-2)] md:grid-cols-2">
                <Input value={profile.name} onChange={(event) => updateProfile(profile.id, { name: event.target.value })} placeholder="Profile name" />
                <Input value={profile.command} onChange={(event) => updateProfile(profile.id, { command: event.target.value })} placeholder="Shell path" />
                <Input value={Array.isArray(profile.args) ? profile.args.join(" ") : ""} onChange={(event) => updateProfile(profile.id, { args: event.target.value.split(/\s+/).filter(Boolean) })} placeholder="Arguments" />
                <Input value={profile.cwd} onChange={(event) => updateProfile(profile.id, { cwd: event.target.value })} placeholder="Working directory" />
              </div>
            </div>
          ))}
        </div>
        <Button
          variant="primary"
          size="sm"
          onClick={() =>
            addProfile({
              id: `prof_${Date.now()}`,
              name: "New Profile",
              command: "/bin/bash",
              args: [],
              cwd: "~",
              env: {},
              themeOverride: null,
              isDefault: false,
            })
          }
        >
          Add Profile
        </Button>
      </SettingsSection>
    </div>
  );
}
