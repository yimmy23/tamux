import { getBridge } from "./bridge";

export type RuntimeModeDescription = {
  title: string;
  summary: string;
  detail: string;
};

export function describeRuntimeMode(options: { hasBridge: boolean }): RuntimeModeDescription | null {
  if (options.hasBridge) {
    return null;
  }

  return {
    title: "Browser Preview Mode",
    summary: "npm run dev only serves the React UI.",
    detail:
      "The Electron bridge, daemon IPC, terminals, and provider actions are unavailable here. Use npm run dev:electron to launch the desktop shell.",
  };
}

export function getRuntimeModeDescription(): RuntimeModeDescription | null {
  return describeRuntimeMode({ hasBridge: Boolean(getBridge()) });
}
