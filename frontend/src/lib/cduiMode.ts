import { readPersistedJson, scheduleJsonWrite } from "./persistence";

const CDUI_PREFERENCE_FILE = "preferences/cdui.json";

let cachedPreference: boolean | null = null;

const readStoredCDUIPreference = (): boolean | null => cachedPreference;

export const hydrateCDUIPreference = async (): Promise<void> => {
  const persisted = await readPersistedJson<{ enabled?: boolean } | boolean>(CDUI_PREFERENCE_FILE);
  if (typeof persisted === "boolean") {
    cachedPreference = persisted;
    return;
  }

  if (persisted && typeof persisted === "object" && typeof persisted.enabled === "boolean") {
    cachedPreference = persisted.enabled;
  }
};

const storeCDUIPreference = (enabled: boolean): void => {
  cachedPreference = enabled;
  scheduleJsonWrite(CDUI_PREFERENCE_FILE, { enabled }, 0);
};

export const isCDUIEnabled = (): boolean => {

  const stored = localStorage.getItem("amux_feature_cdui");
  if (stored === "1") {
    return true;
  }
  if (stored === "0") {
    return false;
  }

  const storedPreference = readStoredCDUIPreference();
  if (storedPreference != null) {
    return storedPreference;
  }

  return true;
};

export const setCDUIEnabled = (enabled: boolean): void => {
  storeCDUIPreference(enabled);
  localStorage.setItem("amux_feature_cdui", enabled ? "1" : "0");
  window.dispatchEvent(new Event("amux:cdui-changed"));
  window.location.reload();
};
