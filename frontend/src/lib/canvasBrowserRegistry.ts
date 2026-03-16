export type CanvasBrowserDomSnapshot = {
  title: string;
  url: string;
  text: string;
};

export type CanvasBrowserController = {
  getUrl: () => string;
  getTitle: () => string;
  getDomSnapshot: () => Promise<CanvasBrowserDomSnapshot>;
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

export function hasCanvasBrowserController(paneId: string): boolean {
  return controllers.has(paneId);
}
