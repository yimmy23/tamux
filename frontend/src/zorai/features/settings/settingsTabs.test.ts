import { describe, expect, it } from "vitest";
import { getDefaultZoraiSettingsTab, zoraiSettingsTabs } from "./settingsTabs";

describe("Zorai settings tabs", () => {
  it("opens auth settings by default like the TUI settings overlay", () => {
    expect(getDefaultZoraiSettingsTab()).toBe("auth");
  });

  it("lists settings sections in TUI-compatible order", () => {
    expect(zoraiSettingsTabs.map((tab) => tab.id)).toEqual([
      "auth",
      "model",
      "concierge",
      "tools",
      "search",
      "runtime",
      "gateway",
      "subagents",
      "features",
      "advanced",
      "plugins",
      "interface",
      "about",
    ]);
    expect(zoraiSettingsTabs.map((tab) => tab.title)).toEqual([
      "Auth",
      "Svarog",
      "Rarog",
      "Tools",
      "Search",
      "Chat",
      "Gateway",
      "Sub-agents",
      "Features",
      "Advanced",
      "Plugins",
      "Terminal interface",
      "About",
    ]);
  });
});
