import React from "react";
import { AgentApprovalOverlay as AgentApprovalOverlayComponent } from "../AgentApprovalOverlay";
import { LayoutContainer as LayoutContainerComponent } from "../LayoutContainer";
import { Sidebar as SidebarComponent } from "../Sidebar";
import { StatusBar as StatusBarComponent } from "../StatusBar";
import { SurfaceTabBar as SurfaceTabBarComponent } from "../SurfaceTabBar";
import { TitleBar as TitleBarComponent } from "../TitleBar";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import { renderEditableWrapper, splitViewProps } from "./propUtils";
import type { ViewProps } from "./shared";

function renderComponentView(props: ViewProps, content: React.ReactNode) {
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
    content,
  });
}

export const TitleBar: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderComponentView(props, <TitleBarComponent {...(componentProps as any)} />);
};

export const SurfaceTabBar: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderComponentView(props, <SurfaceTabBarComponent {...(componentProps as any)} />);
};

export const LayoutContainer: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderComponentView(props, <LayoutContainerComponent {...(componentProps as any)} />);
};

export const StatusBar: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderComponentView(props, <StatusBarComponent {...(componentProps as any)} />);
};

export const Sidebar: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  const sidebarVisible = useWorkspaceStore((s) => s.sidebarVisible);
  if (!sidebarVisible) {
    return null;
  }

  return renderComponentView(props, <SidebarComponent {...(componentProps as any)} />);
};

export const AgentApprovalOverlay: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderComponentView(props, <AgentApprovalOverlayComponent {...(componentProps as any)} />);
};
