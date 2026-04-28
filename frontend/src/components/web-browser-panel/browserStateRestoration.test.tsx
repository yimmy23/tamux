import { createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";

const workspaceState = {
  activeWorkspaceId: "workspace-main",
  updateCanvasPanelUrl: vi.fn(),
  updateCanvasPanelTitle: vi.fn(),
};

vi.mock("../../lib/workspaceStore", () => ({
  useWorkspaceStore: (selector: (state: typeof workspaceState) => unknown) => selector(workspaceState),
}));

vi.mock("../../lib/canvasBrowserRegistry", () => ({
  registerCanvasBrowserController: vi.fn(() => () => {}),
}));

vi.mock("./BrowserChrome", () => ({
  BrowserChrome: () => <div data-browser-chrome="true" />,
}));

import { WebviewFrame } from "./WebviewFrame";
import { CanvasBrowserPane } from "./CanvasBrowserPane";

describe("browser state restoration", () => {
  beforeEach(() => {
    workspaceState.activeWorkspaceId = "workspace-main";
    workspaceState.updateCanvasPanelUrl.mockReset();
    workspaceState.updateCanvasPanelTitle.mockReset();
  });

  it("WebviewFrame uses a persistent webview partition for restored browser session state", () => {
    const html = renderToStaticMarkup(
      <WebviewFrame
        activeWorkspaceId="workspace-main"
        webviewRef={createRef<any>()}
        webBrowserUrl="https://example.com"
      />,
    );

    expect(html).toContain('partition="persist:zorai-browser"');
    expect(html).toContain('src="https://example.com"');
  });

  it("CanvasBrowserPane uses the same persistent webview partition for restored browser session state", () => {
    const html = renderToStaticMarkup(
      <CanvasBrowserPane paneId="pane-1" initialUrl="https://example.com" />,
    );

    expect(html).toContain('partition="persist:zorai-browser"');
    expect(html).toContain('src="https://example.com"');
  });
});
