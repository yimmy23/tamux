import type { BrowserDomSnapshot } from "./browserRegistry";

export type CanvasBrowserController = {
  getUrl: () => string;
  getTitle: () => string;
  navigate: (url: string) => void;
  getDomSnapshot: () => Promise<BrowserDomSnapshot>;
  /** Execute JavaScript in the webview page context and return the result. */
  executeJavaScript: (code: string) => Promise<unknown>;
};

const controllers = new Map<string, CanvasBrowserController>();

export function registerCanvasBrowserController(
  paneId: string,
  controller: CanvasBrowserController,
): () => void {
  controllers.set(paneId, controller);
  return () => {
    if (controllers.get(paneId) === controller) {
      controllers.delete(paneId);
    }
  };
}

export function getCanvasBrowserController(
  paneId: string,
): CanvasBrowserController | undefined {
  return controllers.get(paneId);
}

