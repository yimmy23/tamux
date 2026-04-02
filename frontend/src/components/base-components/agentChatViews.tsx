import React, { useRef, useState } from "react";
import {
  AgentChatPanelChatSurfaceLazy,
  AgentChatPanelContextSurfaceLazy,
  AgentChatPanelCurrentSurfaceLazy,
  AgentChatPanelGraphSurfaceLazy,
  AgentChatPanelHeaderLazy,
  AgentChatPanelLazy,
  AgentChatPanelProviderLazy,
  AgentChatPanelTabsLazy,
  AgentChatPanelThreadsSurfaceLazy,
  AgentChatPanelTraceSurfaceLazy,
  AgentChatPanelUsageSurfaceLazy,
  LazyView,
} from "./lazyComponents";
import { renderEditableWrapper, splitViewProps } from "./propUtils";
import type { ViewProps } from "./shared";

function renderAgentLazyView(props: ViewProps, content: React.ReactNode) {
  const {
    style,
    className,
    children,
    visible,
    hidden,
    resizable,
    resizeAxis,
    minWidth,
    minHeight,
    maxWidth,
    maxHeight,
    builderMeta,
  } = splitViewProps(props);

  return renderEditableWrapper({
    style,
    className,
    children,
    visible,
    hidden,
    resizable,
    resizeAxis,
    minWidth,
    minHeight,
    maxWidth,
    maxHeight,
    builderMeta,
    content: <LazyView>{content}</LazyView>,
  });
}

export const AgentChatPanel: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderAgentLazyView(props, <AgentChatPanelLazy {...(componentProps as any)} />);
};

export const AgentChatDockShell: React.FC<ViewProps> = (props) => {
  const { style, className, children, visible, hidden, minWidth, maxWidth, builderMeta } = splitViewProps(props);
  const shellRef = useRef<HTMLDivElement | null>(null);

  const parseConstraint = (value?: number | string): number | undefined => {
    if (typeof value === "number" && Number.isFinite(value)) {
      return value;
    }

    if (typeof value === "string") {
      const trimmed = value.trim();
      if (trimmed.endsWith("vw")) {
        const amount = Number.parseFloat(trimmed.slice(0, -2));
        return Number.isFinite(amount) ? (window.innerWidth * amount) / 100 : undefined;
      }
      const parsed = Number.parseFloat(trimmed);
      return Number.isFinite(parsed) ? parsed : undefined;
    }

    return undefined;
  };

  const minWidthValue = parseConstraint(minWidth ?? style?.minWidth) ?? 280;
  const maxWidthValue = parseConstraint(maxWidth ?? style?.maxWidth) ?? Math.round(window.innerWidth * 0.8);
  const initialWidth = parseConstraint(style?.width) ?? 440;
  const clampWidth = (value: number) => Math.min(maxWidthValue, Math.max(minWidthValue, Math.round(value)));
  const [width, setWidth] = useState(() => clampWidth(initialWidth));

  const handleResizeStart = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!shellRef.current) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();

    const pointerId = event.pointerId;
    const target = event.currentTarget;
    const rect = shellRef.current.getBoundingClientRect();
    const startRight = rect.right;

    target.setPointerCapture(pointerId);
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";

    const onPointerMove = (moveEvent: PointerEvent) => {
      setWidth(clampWidth(startRight - moveEvent.clientX));
    };

    const onPointerEnd = () => {
      if (target.hasPointerCapture(pointerId)) {
        target.releasePointerCapture(pointerId);
      }
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerEnd);
      window.removeEventListener("pointercancel", onPointerEnd);
    };

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerEnd);
    window.addEventListener("pointercancel", onPointerEnd);
  };

  return renderEditableWrapper({
    style: {
      ...(style ?? {}),
      width: `${width}px`,
      flex: `0 0 ${width}px`,
      flexShrink: 0,
      minWidth: `${minWidthValue}px`,
      maxWidth: `${maxWidthValue}px`,
      position: style?.position ?? "relative",
      overflow: style?.overflow ?? "hidden",
    },
    className,
    visible,
    hidden,
    minWidth,
    maxWidth,
    builderMeta,
    content: (
      <div ref={shellRef} style={{ display: "flex", flexDirection: "column", height: "100%", minHeight: 0, position: "relative" }}>
        {children}
        <div
          onPointerDown={handleResizeStart}
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            width: 10,
            height: "100%",
            cursor: "col-resize",
            zIndex: 20,
            background: "linear-gradient(90deg, rgba(148, 163, 184, 0.2), rgba(148, 163, 184, 0.45), transparent)",
          }}
        />
      </div>
    ),
  });
};

export const AgentChatPanelProvider: React.FC<ViewProps> = (props) => {
  const {
    style,
    className,
    children,
    visible,
    hidden,
    resizable,
    resizeAxis,
    minWidth,
    minHeight,
    maxWidth,
    maxHeight,
    builderMeta,
    componentProps,
  } = splitViewProps(props);

  return renderEditableWrapper({
    style,
    className,
    children,
    visible,
    hidden,
    resizable,
    resizeAxis,
    minWidth,
    minHeight,
    maxWidth,
    maxHeight,
    builderMeta,
    appendChildren: false,
    content: <LazyView><AgentChatPanelProviderLazy {...(componentProps as any)}>{children}</AgentChatPanelProviderLazy></LazyView>,
  });
};

export const AgentChatPanelHeader: React.FC<ViewProps> = (props) => renderAgentLazyView(props, <AgentChatPanelHeaderLazy />);
export const AgentChatPanelTabs: React.FC<ViewProps> = (props) => renderAgentLazyView(props, <AgentChatPanelTabsLazy />);
export const AgentChatPanelCurrentSurface: React.FC<ViewProps> = (props) => renderAgentLazyView(props, <AgentChatPanelCurrentSurfaceLazy />);
export const AgentChatPanelThreadsSurface: React.FC<ViewProps> = (props) => renderAgentLazyView(props, <AgentChatPanelThreadsSurfaceLazy />);
export const AgentChatPanelChatSurface: React.FC<ViewProps> = (props) => renderAgentLazyView(props, <AgentChatPanelChatSurfaceLazy />);
export const AgentChatPanelTraceSurface: React.FC<ViewProps> = (props) => renderAgentLazyView(props, <AgentChatPanelTraceSurfaceLazy />);
export const AgentChatPanelUsageSurface: React.FC<ViewProps> = (props) => renderAgentLazyView(props, <AgentChatPanelUsageSurfaceLazy />);
export const AgentChatPanelContextSurface: React.FC<ViewProps> = (props) => renderAgentLazyView(props, <AgentChatPanelContextSurfaceLazy />);
export const AgentChatPanelGraphSurface: React.FC<ViewProps> = (props) => renderAgentLazyView(props, <AgentChatPanelGraphSurfaceLazy />);
