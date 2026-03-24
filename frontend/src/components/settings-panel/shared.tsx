import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { BUILTIN_THEMES } from "../../lib/themes";
import type { AgentProviderId, AuthSource, ModelDefinition } from "../../lib/agentStore";
import { getProviderDefinition, getProviderModels } from "../../lib/agentStore";
import type { AmuxSettings } from "../../lib/types";
import { Button, Input, TextArea, cn, fieldClassName, popoverSurfaceClassName } from "../ui";

export type SettingsUpdater = <K extends keyof AmuxSettings>(key: K, value: AmuxSettings[K]) => void;

type SelectOption = string | { value: string; label: string };

const selectClassName = cn(
  fieldClassName,
  "appearance-none pr-[var(--space-8)] [background-image:linear-gradient(45deg,transparent_50%,var(--text-muted)_50%),linear-gradient(135deg,var(--text-muted)_50%,transparent_50%)] [background-position:calc(100%-18px)_calc(50%-2px),calc(100%-12px)_calc(50%-2px)] [background-repeat:no-repeat] [background-size:6px_6px]"
);

function NativeSelect({
  className,
  children,
  ...props
}: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select className={cn(selectClassName, "min-w-[12.5rem]", className)} {...props}>
      {children}
    </select>
  );
}

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="grid gap-[var(--space-2)]">
      <div className="text-[var(--text-xs)] font-semibold uppercase tracking-[0.08em] text-[var(--accent)]">
        {title}
      </div>
      {children}
    </section>
  );
}

export function SettingRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-[var(--space-3)] py-[var(--space-1)] text-[var(--text-sm)]">
      <span className="shrink-0 text-[var(--text-secondary)]">{label}</span>
      {children}
    </div>
  );
}

export function FontSelector({
  value,
  fonts,
  onChange,
}: {
  value: string;
  fonts: string[];
  onChange: (value: string) => void;
}) {
  return (
    <div className="flex items-center gap-[var(--space-2)]">
      <NativeSelect
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="w-[12.5rem]"
      >
        {fonts.map((font) => (
          <option key={font} value={font} style={{ fontFamily: font }}>
            {font}
          </option>
        ))}
        {!fonts.includes(value) ? <option value={value}>{value}</option> : null}
      </NativeSelect>
      <span className="text-[var(--text-sm)] text-[var(--text-secondary)]" style={{ fontFamily: value }}>
        Abc
      </span>
    </div>
  );
}

export function ThemePicker({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return (
    <div className="mt-[var(--space-1)] grid grid-cols-2 gap-[var(--space-2)] md:grid-cols-4">
      {BUILTIN_THEMES.map((theme) => (
        <button
          key={theme.name}
          type="button"
          onClick={() => onChange(theme.name)}
          className={cn(
            "flex flex-col gap-[var(--space-1)] rounded-[var(--radius-md)] border p-[var(--space-2)] text-left transition-colors",
            value === theme.name
              ? "border-[var(--accent)] shadow-[0_0_0_1px_var(--accent-border)]"
              : "border-[var(--border)] hover:border-[var(--border-strong)]"
          )}
          style={{ background: theme.colors.background }}
        >
          <div className="flex gap-[2px]">
            {[
              theme.colors.red,
              theme.colors.green,
              theme.colors.yellow,
              theme.colors.blue,
              theme.colors.magenta,
              theme.colors.cyan,
            ].map((color, index) => (
              <div
                key={index}
                className="h-[8px] w-[8px] rounded-[2px]"
                style={{ background: color }}
              />
            ))}
          </div>
          <span
            className="truncate text-[10px] font-medium"
            style={{ color: theme.colors.foreground }}
          >
            {theme.name}
          </span>
        </button>
      ))}
    </div>
  );
}

export function ColorInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  return (
    <div className="flex items-center gap-[var(--space-2)]">
      <input
        type="color"
        value={value || placeholder || "#000000"}
        onChange={(event) => onChange(event.target.value)}
        className="h-[var(--space-8)] w-[var(--space-9)] cursor-pointer rounded-[var(--radius-md)] border border-[var(--border)] bg-transparent p-[2px]"
      />
      <Input
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        className="w-[6.5rem] font-mono text-[var(--text-xs)]"
      />
    </div>
  );
}

export function SliderInput({
  value,
  min,
  max,
  step,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="flex items-center gap-[var(--space-2)]">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(parseFloat(event.target.value))}
        className="w-[7.5rem] accent-[var(--accent)]"
      />
      <span className="min-w-[2rem] text-right text-[var(--text-xs)] text-[var(--text-secondary)]">
        {Number.isInteger(step) ? value : value.toFixed(step < 0.1 ? 2 : 1)}
      </span>
    </div>
  );
}

export function TextInput({
  value,
  onChange,
  placeholder,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
}) {
  return (
    <Input
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder={placeholder}
      className={cn("w-[12.5rem]", className)}
    />
  );
}

export function TextAreaInput({
  value,
  onChange,
  placeholder,
  rows,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  rows?: number;
  className?: string;
}) {
  return (
    <TextArea
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder={placeholder}
      rows={rows}
      className={cn("min-h-[7rem] w-full font-[inherit]", className)}
    />
  );
}

export function PasswordInput({
  value,
  onChange,
  placeholder,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
}) {
  const [visible, setVisible] = useState(false);

  return (
    <div className={cn("flex items-center gap-[var(--space-2)]", className)}>
      <Input
        type={visible ? "text" : "password"}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        className="w-[12.5rem]"
      />
      <Button
        variant="outline"
        size="sm"
        onClick={() => setVisible(!visible)}
        title={visible ? "Hide" : "Show"}
      >
        {visible ? "\u25C9" : "\u25CB"}
      </Button>
    </div>
  );
}

export function NumberInput({
  value,
  min,
  max,
  step,
  onChange,
  className,
}: {
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange: (value: number) => void;
  className?: string;
}) {
  return (
    <Input
      type="number"
      value={value}
      min={min}
      max={max}
      step={step ?? 1}
      onChange={(event) => {
        const nextValue = parseFloat(event.target.value);
        if (!isNaN(nextValue)) onChange(nextValue);
      }}
      className={cn("w-20", className)}
    />
  );
}

export function SelectInput({
  value,
  options,
  onChange,
  className,
}: {
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  className?: string;
}) {
  return (
    <NativeSelect
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className={className}
    >
      {options.map((option) => {
        const normalized = typeof option === "string" ? { value: option, label: option } : option;
        return (
          <option key={normalized.value} value={normalized.value}>
            {normalized.label}
          </option>
        );
      })}
    </NativeSelect>
  );
}

export function Toggle({ value, onChange }: { value: boolean; onChange: (value: boolean) => void }) {
  return (
    <button
      type="button"
      onClick={() => onChange(!value)}
      className={cn(
        "relative h-[1.5rem] w-[2.75rem] rounded-full border transition-colors",
        value
          ? "border-[var(--accent-border)] bg-[var(--accent-soft)]"
          : "border-[var(--border)] bg-[var(--muted)]"
      )}
      aria-pressed={value}
    >
      <span
        className={cn(
          "absolute top-[3px] h-[1rem] w-[1rem] rounded-full bg-[var(--text-primary)] transition-[left]",
          value ? "left-[1.55rem]" : "left-[3px]"
        )}
      />
    </button>
  );
}

export const inputStyle: CSSProperties = {
  background: "var(--input)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-md)",
  color: "var(--input-foreground)",
  fontSize: "var(--text-sm)",
  padding: "var(--space-2) var(--space-3)",
  fontFamily: "inherit",
  outline: "none",
  width: 200,
};

export const headerBtnStyle: CSSProperties = {
  background: "transparent",
  border: "1px solid transparent",
  color: "var(--text-secondary)",
  cursor: "pointer",
  fontSize: "var(--text-sm)",
  padding: "var(--space-1) var(--space-2)",
  borderRadius: "var(--radius-md)",
};

export const addBtnStyle: CSSProperties = {
  background: "var(--secondary)",
  border: "1px solid var(--border)",
  color: "var(--secondary-foreground)",
  cursor: "pointer",
  fontSize: "var(--text-xs)",
  padding: "var(--space-2) var(--space-3)",
  borderRadius: "var(--radius-md)",
  marginTop: 8,
};

export const kbdStyle: CSSProperties = {
  background: "var(--muted)",
  padding: "2px 6px",
  borderRadius: "var(--radius-sm)",
  fontSize: "var(--text-xs)",
  fontFamily: "var(--font-mono)",
};

export const rebindBtnStyle: CSSProperties = {
  background: "transparent",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-md)",
  color: "var(--text-primary)",
  cursor: "pointer",
  fontSize: "var(--text-xs)",
  padding: "var(--space-2) var(--space-3)",
};

export const smallBtnStyle: CSSProperties = {
  background: "transparent",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-md)",
  color: "var(--text-primary)",
  cursor: "pointer",
  fontSize: "var(--text-xs)",
  padding: "var(--space-2) var(--space-3)",
};

export function ModelSelector({
  providerId,
  value,
  customName,
  onChange,
  disabled,
  base_url,
  api_key,
  auth_source,
}: {
  providerId: AgentProviderId;
  value: string;
  customName?: string;
  onChange: (value: string, name?: string) => void;
  disabled?: boolean;
  base_url?: string;
  api_key?: string;
  auth_source?: AuthSource;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [useCustom, setUseCustom] = useState(false);
  const [customModelId, setCustomModelId] = useState(value);
  const [custom_model_name, setCustomModelName] = useState(customName || "");
  const [fetchedModels, setFetchedModels] = useState<ModelDefinition[]>([]);
  const [isFetching, setIsFetching] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const definition = getProviderDefinition(providerId);
  const predefinedModels = getProviderModels(providerId, auth_source);
  const supportsFetch =
    (definition?.supportsModelFetch ?? false) &&
    !(providerId === "openai" && auth_source === "chatgpt_subscription");

  const allModels = useMemo(() => {
    const merged = [...predefinedModels];
    for (const fm of fetchedModels) {
      if (!merged.some((m) => m.id === fm.id)) {
        merged.push(fm);
      }
    }
    if (value.trim() && !merged.some((m) => m.id === value.trim())) {
      merged.unshift({
        id: value.trim(),
        name: customName?.trim() || value.trim(),
        contextWindow: 0,
      });
    }
    return merged;
  }, [predefinedModels, fetchedModels, value, customName]);

  const filteredModels = useMemo(() => {
    if (!search) return allModels;
    const lower = search.toLowerCase();
    return allModels.filter(
      (m) => m.id.toLowerCase().includes(lower) || m.name.toLowerCase().includes(lower)
    );
  }, [allModels, search]);

  const exactMatch = useMemo(() => {
    return filteredModels.some((m) => m.id === search || m.id === value);
  }, [filteredModels, search, value]);

  const handleFetchModels = async () => {
    const amux = (window as any).amux || (window as any).tamux;
    if (!amux?.agentFetchModels) {
      setFetchError("API not available");
      return;
    }

    setIsFetching(true);
    setFetchError(null);

    try {
      const result = await amux.agentFetchModels(
        providerId,
        base_url || definition?.defaultBaseUrl || "",
        api_key || ""
      );

      if (result && typeof result === "object") {
        if ("models" in result && Array.isArray(result.models)) {
          setFetchedModels(
            result.models.map((m: any) => ({
              id: m.id,
              name: m.name || m.id,
              contextWindow: m.context_window || m.contextWindow || 0,
            }))
          );
        } else if ("error" in result) {
          setFetchError(result.error);
        }
      }
    } catch (e: any) {
      setFetchError(e.message || "Failed to fetch models");
    } finally {
      setIsFetching(false);
    }
  };

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
        setUseCustom(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  useEffect(() => {
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isOpen]);

  useEffect(() => {
    setCustomModelId(value);
  }, [value]);

  useEffect(() => {
    setCustomModelName(customName || "");
  }, [customName]);

  const formatContextWindow = (tokens: number): string => {
    if (tokens >= 1000000) return `${(tokens / 1000000).toFixed(1)}M`;
    if (tokens >= 1000) return `${(tokens / 1000).toFixed(0)}K`;
    return `${tokens}`;
  };

  if (useCustom) {
    return (
      <div className="grid w-full gap-[var(--space-2)]">
        <Input
          value={custom_model_name}
          onChange={(e) => setCustomModelName(e.target.value)}
          placeholder="Display name (optional)"
          disabled={disabled}
        />
        <div className="flex items-center gap-[var(--space-2)]">
          <Input
            ref={inputRef}
            value={customModelId}
            onChange={(e) => setCustomModelId(e.target.value)}
            placeholder="Enter model ID"
            disabled={disabled}
            className="flex-1"
          />
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              const nextId = customModelId.trim();
              if (!nextId) return;
              onChange(nextId, custom_model_name.trim() || nextId);
              setUseCustom(false);
              setIsOpen(false);
              setSearch("");
            }}
            title="Apply custom model"
          >
            Apply
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setUseCustom(false)} title="Back to model list">
            ✕
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div ref={containerRef} className="relative w-full">
      <div className="flex items-center gap-[var(--space-2)]">
        <Input
          ref={inputRef}
          value={isOpen ? search : value}
          onChange={(e) => {
            setSearch(e.target.value);
            if (!isOpen) setIsOpen(true);
          }}
          onFocus={() => {
            setIsOpen(true);
            setSearch("");
          }}
          placeholder="Select or type model ID"
          disabled={disabled}
          className="flex-1"
        />
        {supportsFetch && api_key ? (
          <Button
            variant="outline"
            size="sm"
            onClick={handleFetchModels}
            disabled={isFetching || !api_key}
            title="Fetch models from provider"
          >
            {isFetching ? "..." : "↻"}
          </Button>
        ) : null}
      </div>

      {isOpen ? (
        <div
          className={cn(
            popoverSurfaceClassName,
            "absolute left-0 right-0 top-full z-[1000] mt-[2px] max-h-[15rem] overflow-y-auto"
          )}
        >
          {fetchError ? (
            <div className="border-b border-[var(--border)] px-[var(--space-3)] py-[var(--space-2)] text-[var(--text-xs)] text-[var(--danger)]">
              {fetchError}
            </div>
          ) : null}

          {filteredModels.length > 0
            ? filteredModels.map((model) => (
                <button
                  key={model.id}
                  type="button"
                  onClick={() => {
                    onChange(model.id, model.name);
                    setIsOpen(false);
                    setSearch("");
                  }}
                  className={cn(
                    "flex w-full items-center justify-between gap-[var(--space-3)] border-b border-[var(--border-subtle)] px-[var(--space-3)] py-[var(--space-2)] text-left transition-colors last:border-b-0 hover:bg-[var(--muted)]",
                    model.id === value && "bg-[var(--accent-soft)]"
                  )}
                >
                  <div className="min-w-0">
                    <div className="truncate text-[var(--text-sm)] text-[var(--text-primary)]">
                      {model.name}
                    </div>
                    <div className="truncate font-mono text-[10px] text-[var(--text-muted)]">
                      {model.id}
                    </div>
                  </div>
                  {model.contextWindow > 0 ? (
                    <div className="shrink-0 text-[10px] text-[var(--text-secondary)]">
                      {formatContextWindow(model.contextWindow)} ctx
                    </div>
                  ) : null}
                </button>
              ))
            : null}

          {!exactMatch ? (
            <button
              type="button"
              onClick={() => {
                if (search) {
                  onChange(search, search);
                  setIsOpen(false);
                  setSearch("");
                } else {
                  setCustomModelId(value);
                  setCustomModelName(customName || "");
                  setUseCustom(true);
                }
              }}
              className={cn(
                "w-full px-[var(--space-3)] py-[var(--space-2)] text-left text-[var(--text-sm)] transition-colors hover:bg-[var(--muted)]",
                filteredModels.length > 0 && "border-t border-[var(--border)]",
                search ? "text-[var(--accent)]" : "text-[var(--text-secondary)]"
              )}
            >
              {search ? <>Use "{search}" anyway</> : <>Type custom model ID...</>}
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
