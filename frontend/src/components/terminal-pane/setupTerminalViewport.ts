import type { MutableRefObject } from "react";
import type { FitAddon } from "@xterm/addon-fit";
import type { Terminal } from "@xterm/xterm";

export function setupTerminalViewport({
  term,
  containerRef,
  handleFocus,
  fitAddon,
  sendResize,
}: {
  term: Terminal;
  containerRef: MutableRefObject<HTMLDivElement | null>;
  handleFocus: () => void;
  fitAddon: FitAddon;
  sendResize: () => void;
}) {
  const container = containerRef.current;
  if (!container) {
    throw new Error("Terminal container is required");
  }

  term.open(container);
  term.textarea?.focus({ preventScroll: true });
  const viewportElement = container.querySelector<HTMLElement>(".xterm-viewport");
  const isViewportAtBottom = () => {
    const buffer = term.buffer.active;
    if (!buffer) return true;
    return (buffer.baseY - buffer.viewportY) <= 1;
  };
  let followOutput = true;
  const syncFollowOutputWithViewport = () => {
    followOutput = isViewportAtBottom();
  };
  syncFollowOutputWithViewport();
  const scrollSyncDisposable = term.onScroll(() => {
    syncFollowOutputWithViewport();
  });
  const stopViewportWheelPropagation = (event: Event) => {
    event.stopPropagation();
  };
  viewportElement?.addEventListener("wheel", stopViewportWheelPropagation, { capture: true, passive: true });

  const snapshotAncestorScroll = () => {
    const positions = new Map<HTMLElement, { top: number; left: number }>();
    let ancestor: HTMLElement | null = containerRef.current?.parentElement ?? null;
    while (ancestor) {
      positions.set(ancestor, { top: ancestor.scrollTop, left: ancestor.scrollLeft });
      ancestor = ancestor.parentElement;
    }
    const documentScroller = document.scrollingElement;
    if (documentScroller instanceof HTMLElement && !positions.has(documentScroller)) {
      positions.set(documentScroller, { top: documentScroller.scrollTop, left: documentScroller.scrollLeft });
    }
    return positions;
  };

  const restoreAncestorScroll = (positions: Map<HTMLElement, { top: number; left: number }>) => {
    for (const [element, previous] of positions.entries()) {
      if (element.scrollTop !== previous.top) {
        element.scrollTop = previous.top;
      }
      if (element.scrollLeft !== previous.left) {
        element.scrollLeft = previous.left;
      }
    }
  };

  const writeWithPreservedAncestorScroll = (data: Uint8Array) => {
    const positions = snapshotAncestorScroll();
    const maybeStickViewportToBottom = () => {
      const textarea = term.textarea;
      if (!textarea || document.activeElement !== textarea) return;
      if (term.hasSelection()) return;
      if (!followOutput) return;
      if (!isViewportAtBottom()) {
        term.scrollToBottom();
      }
    };
    term.write(data, () => {
      restoreAncestorScroll(positions);
      maybeStickViewportToBottom();
      window.requestAnimationFrame(() => restoreAncestorScroll(positions));
    });
    restoreAncestorScroll(positions);
    maybeStickViewportToBottom();
    window.requestAnimationFrame(() => restoreAncestorScroll(positions));
  };

  const handleTextareaFocus = () => {
    handleFocus();
    const positions = snapshotAncestorScroll();
    restoreAncestorScroll(positions);
    window.requestAnimationFrame(() => restoreAncestorScroll(positions));
  };
  term.textarea?.addEventListener("focus", handleTextareaFocus);

  let fitFrame = 0;
  let fitAttempts = 0;
  const fitWhenReady = () => {
    const currentContainer = containerRef.current;
    if (!currentContainer) return;

    const rect = currentContainer.getBoundingClientRect();
    if ((rect.width < 24 || rect.height < 24) && fitAttempts < 12) {
      fitAttempts += 1;
      fitFrame = window.requestAnimationFrame(fitWhenReady);
      return;
    }

    fitAttempts = 0;
    fitAddon.fit();
    sendResize();
  };

  const scheduleFit = () => {
    fitAttempts = 0;
    fitFrame = window.requestAnimationFrame(fitWhenReady);
  };

  scheduleFit();

  return {
    writeWithPreservedAncestorScroll,
    scheduleFit,
    cleanupViewport: () => {
      window.cancelAnimationFrame(fitFrame);
      term.textarea?.removeEventListener("focus", handleTextareaFocus);
      viewportElement?.removeEventListener("wheel", stopViewportWheelPropagation, true);
      scrollSyncDisposable.dispose();
    },
  };
}
