import type { ReactNode } from "react";
import type { AmuxSettings } from "../../lib/types";
import { Badge, Card, CardContent, CardDescription, CardHeader, CardTitle } from "../ui";
import { ColorInput, FontSelector, SettingRow, type SettingsUpdater, SliderInput, ThemePicker, Toggle, SelectInput } from "./shared";

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

export function AppearanceTab({
  settings,
  updateSetting,
  systemFonts,
}: {
  settings: AmuxSettings;
  updateSetting: SettingsUpdater;
  systemFonts: string[];
}) {
  const monoFonts =
    systemFonts.length > 0
      ? systemFonts
      : [
          "Cascadia Code",
          "Cascadia Mono",
          "Consolas",
          "JetBrains Mono",
          "Fira Code",
          "Source Code Pro",
          "Hack",
          "DejaVu Sans Mono",
          "Ubuntu Mono",
          "Courier New",
          "monospace",
        ];

  return (
    <div className="grid gap-[var(--space-4)]">
      <SettingsSection title="Font" description="Typography and spacing controls for the redesign shell.">
        <SettingRow label="Font Family">
          <FontSelector value={settings.fontFamily} fonts={monoFonts} onChange={(value) => updateSetting("fontFamily", value)} />
        </SettingRow>
        <SettingRow label="Font Size">
          <SliderInput value={settings.fontSize} min={8} max={28} step={1} onChange={(value) => updateSetting("fontSize", value)} />
        </SettingRow>
        <SettingRow label="Line Height">
          <SliderInput value={settings.lineHeight} min={0.8} max={2} step={0.1} onChange={(value) => updateSetting("lineHeight", value)} />
        </SettingRow>
      </SettingsSection>

      <SettingsSection title="Theme" description="Keep the appearance controls wired to the existing theme store.">
        <ThemePicker value={settings.themeName} onChange={(value) => updateSetting("themeName", value)} />
      </SettingsSection>

      <SettingsSection
        title="Custom Terminal Colors"
        description="Optional overrides for terminal backgrounds, text, cursor, and selection colors."
        badge={<Badge variant={settings.useCustomTerminalColors ? "accent" : "default"}>{settings.useCustomTerminalColors ? "Custom" : "Inherited"}</Badge>}
      >
        <SettingRow label="Use Custom Colors">
          <Toggle value={settings.useCustomTerminalColors} onChange={(value) => updateSetting("useCustomTerminalColors", value)} />
        </SettingRow>
        {settings.useCustomTerminalColors ? (
          <>
            <SettingRow label="Background">
              <ColorInput value={settings.customTerminalBackground} onChange={(value) => updateSetting("customTerminalBackground", value)} placeholder="#1e1e2e" />
            </SettingRow>
            <SettingRow label="Foreground">
              <ColorInput value={settings.customTerminalForeground} onChange={(value) => updateSetting("customTerminalForeground", value)} placeholder="#cdd6f4" />
            </SettingRow>
            <SettingRow label="Cursor">
              <ColorInput value={settings.customTerminalCursor} onChange={(value) => updateSetting("customTerminalCursor", value)} placeholder="#f5e0dc" />
            </SettingRow>
            <SettingRow label="Selection">
              <ColorInput value={settings.customTerminalSelection} onChange={(value) => updateSetting("customTerminalSelection", value)} placeholder="#45475a" />
            </SettingRow>
          </>
        ) : null}
      </SettingsSection>

      <div className="grid gap-[var(--space-4)] lg:grid-cols-2">
        <SettingsSection title="Performance" description="Rendering options that can affect runtime responsiveness." badge={<Badge variant="warning">Restart required</Badge>}>
          <SettingRow label="GPU Acceleration">
            <Toggle value={settings.gpuAcceleration} onChange={(value) => updateSetting("gpuAcceleration", value)} />
          </SettingRow>
        </SettingsSection>

        <SettingsSection title="Window" description="Cursor and opacity controls for the active shell surfaces.">
          <SettingRow label="Opacity">
            <SliderInput value={settings.opacity} min={0.3} max={1} step={0.05} onChange={(value) => updateSetting("opacity", value)} />
          </SettingRow>
          <SettingRow label="Cursor Style">
            <SelectInput value={settings.cursorStyle} options={["bar", "block", "underline"]} onChange={(value) => updateSetting("cursorStyle", value as AmuxSettings["cursorStyle"])} />
          </SettingRow>
          <SettingRow label="Cursor Blink">
            <Toggle value={settings.cursorBlink} onChange={(value) => updateSetting("cursorBlink", value)} />
          </SettingRow>
        </SettingsSection>
      </div>
    </div>
  );
}
