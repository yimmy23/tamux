import { create } from "zustand";
import { getBridge } from "./bridge";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type PluginInfoItem = {
  name: string;
  version: string;
  description?: string;
  author?: string;
  enabled: boolean;
  install_source: string;
  has_api: boolean;
  has_auth: boolean;
  has_commands: boolean;
  has_skills: boolean;
  endpoint_count: number;
  settings_count: number;
  installed_at: string;
  updated_at: string;
  auth_status: string;
};

export type PluginSettingValue = {
  key: string;
  value: string;
  is_secret: boolean;
};

/** Parsed from the manifest settings_schema JSON */
export type PluginSettingField = {
  key: string;
  type: string; // "string" | "number" | "boolean" | "select" | "secret"
  label: string;
  required: boolean;
  secret: boolean;
  default?: unknown;
  options?: string[];
  description?: string;
};

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

type PluginStoreState = {
  plugins: PluginInfoItem[];
  loading: boolean;
  error: string | null;

  // Selected plugin detail
  selectedPlugin: string | null;
  settingsSchema: PluginSettingField[];
  settingsValues: PluginSettingValue[];
  settingsLoading: boolean;

  // Test connection state
  testResult: { success: boolean; message: string } | null;
  testLoading: boolean;

  // OAuth state
  oauthError: string | null;

  // Actions
  fetchPlugins: () => Promise<void>;
  selectPlugin: (name: string | null) => Promise<void>;
  toggleEnabled: (name: string, enabled: boolean) => Promise<void>;
  updateSetting: (pluginName: string, key: string, value: string, isSecret: boolean) => Promise<void>;
  testConnection: (name: string) => Promise<void>;
  startOAuth: (name: string) => Promise<void>;
  initOAuthListener: () => void;
};

export const usePluginStore = create<PluginStoreState>((set, get) => ({
  plugins: [],
  loading: false,
  error: null,

  selectedPlugin: null,
  settingsSchema: [],
  settingsValues: [],
  settingsLoading: false,

  testResult: null,
  testLoading: false,

  oauthError: null,

  fetchPlugins: async () => {
    set({ loading: true, error: null });
    try {
      const bridge = getBridge();
      const result = await bridge?.pluginDaemonList?.();
      const plugins: PluginInfoItem[] = (result?.plugins ?? []).map((p) => ({
        ...p,
        auth_status: (p.auth_status as string) ?? "not_configured",
      }));
      set({ plugins, loading: false });
    } catch {
      set({ plugins: [], loading: false, error: "Could not load plugins. Ensure the daemon is running." });
    }
  },

  selectPlugin: async (name) => {
    if (name === null) {
      set({ selectedPlugin: null, settingsSchema: [], settingsValues: [], testResult: null });
      return;
    }

    set({ settingsLoading: true, testResult: null });
    try {
      const bridge = getBridge();

      // Fetch manifest schema
      const detail = await bridge?.pluginDaemonGet?.(name);
      let schema: PluginSettingField[] = [];
      if (detail?.settings_schema) {
        try {
          const parsed = JSON.parse(detail.settings_schema) as Record<string, Omit<PluginSettingField, "key">>;
          schema = Object.entries(parsed).map(([key, field]) => ({
            key,
            type: field.type ?? "string",
            label: field.label ?? key,
            required: field.required ?? false,
            secret: field.secret ?? false,
            default: field.default,
            options: field.options,
            description: field.description,
          }));
        } catch {
          // Malformed schema -- treat as no fields
        }
      }

      // Fetch current values
      const valuesResult = await bridge?.pluginGetSettings?.(name);
      const values: PluginSettingValue[] = valuesResult?.settings ?? [];

      set({ selectedPlugin: name, settingsSchema: schema, settingsValues: values, settingsLoading: false });
    } catch {
      set({ selectedPlugin: name, settingsSchema: [], settingsValues: [], settingsLoading: false });
    }
  },

  toggleEnabled: async (name, enabled) => {
    // Optimistic update
    set((s) => ({
      plugins: s.plugins.map((p) => (p.name === name ? { ...p, enabled } : p)),
    }));
    try {
      const bridge = getBridge();
      if (enabled) {
        await bridge?.pluginDaemonEnable?.(name);
      } else {
        await bridge?.pluginDaemonDisable?.(name);
      }
      // Refetch to confirm
      await get().fetchPlugins();
    } catch {
      // Revert on failure
      await get().fetchPlugins();
    }
  },

  updateSetting: async (pluginName, key, value, isSecret) => {
    try {
      const bridge = getBridge();
      const result = await bridge?.pluginUpdateSettings?.(pluginName, key, value, isSecret);
      if (result?.ok) {
        // Update local values
        set((s) => {
          const existing = s.settingsValues.findIndex((v) => v.key === key);
          const entry: PluginSettingValue = { key, value: isSecret ? "********" : value, is_secret: isSecret };
          const next = [...s.settingsValues];
          if (existing >= 0) {
            next[existing] = entry;
          } else {
            next.push(entry);
          }
          return { settingsValues: next };
        });
      }
    } catch {
      // Save failed silently -- daemon may be unreachable
    }
  },

  testConnection: async (name) => {
    set({ testLoading: true, testResult: null });
    try {
      const bridge = getBridge();
      const result = await bridge?.pluginTestConnection?.(name);
      if (result) {
        set({ testResult: { success: result.success, message: result.message }, testLoading: false });
      } else {
        set({ testResult: null, testLoading: false });
      }
    } catch {
      set({ testResult: { success: false, message: "Connection test failed unexpectedly." }, testLoading: false });
    }
  },

  startOAuth: async (name) => {
    set({ oauthError: null });
    try {
      const bridge = getBridge();
      const result = await bridge?.pluginOAuthStart?.(name);
      if (result?.url) {
        // The Electron main process opens the browser via shell.openExternal
        // when it receives the PluginOAuthUrl from the daemon.
        // No need to open from renderer side.
      }
    } catch {
      set({ oauthError: "Failed to start OAuth flow. Ensure the daemon is running." });
    }
  },

  initOAuthListener: () => {
    const bridge = getBridge();
    bridge?.onPluginOAuthComplete?.((data) => {
      if (data.success) {
        // Refresh plugin list to pick up updated auth_status
        void get().fetchPlugins();
      } else {
        set({ oauthError: data.error ?? "OAuth flow failed." });
      }
    });
  },
}));
