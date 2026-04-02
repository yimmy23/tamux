import React from "react";
import {
  CommandHistoryPickerLazy,
  CommandLogPanelLazy,
  CommandPaletteLazy,
  ExecutionCanvasLazy,
  FileManagerPanelLazy,
  LazyView,
  NotificationPanelLazy,
  SearchOverlayLazy,
  SessionVaultPanelLazy,
  SettingsPanelLazy,
  SnippetPickerLazy,
  SystemMonitorPanelLazy,
  TimeTravelSliderLazy,
  WebBrowserPanelLazy,
} from "./lazyComponents";
import { renderEditableWrapper, splitViewProps } from "./propUtils";
import type { ViewProps } from "./shared";

function renderLazyWrappedView(props: ViewProps, content: React.ReactNode) {
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

export const CommandPalette: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <CommandPaletteLazy {...(componentProps as any)} />);
};

export const NotificationPanel: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <NotificationPanelLazy {...(componentProps as any)} />);
};

export const SettingsPanel: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <SettingsPanelLazy {...(componentProps as any)} />);
};

export const SessionVaultPanel: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <SessionVaultPanelLazy {...(componentProps as any)} />);
};

export const CommandLogPanel: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <CommandLogPanelLazy {...(componentProps as any)} />);
};

export const CommandHistoryPicker: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <CommandHistoryPickerLazy {...(componentProps as any)} />);
};

export const SearchOverlay: React.FC<ViewProps> = (props) => {
  const { className, componentProps } = splitViewProps(props);
  return renderLazyWrappedView(
    props,
    <SearchOverlayLazy
      {...(componentProps as any)}
      style={{
        position: "static",
        top: "auto",
        right: "auto",
        zIndex: "auto",
        ...(componentProps as any)?.style,
      }}
      className={className}
    />,
  );
};

export const SnippetPicker: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <SnippetPickerLazy {...(componentProps as any)} />);
};

export const SystemMonitorPanel: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <SystemMonitorPanelLazy {...(componentProps as any)} />);
};

export const FileManagerPanel: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <FileManagerPanelLazy {...(componentProps as any)} />);
};

export const TimeTravelSlider: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <TimeTravelSliderLazy {...(componentProps as any)} />);
};

export const ExecutionCanvas: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <ExecutionCanvasLazy {...(componentProps as any)} />);
};

export const WebBrowserPanel: React.FC<ViewProps> = (props) => {
  const { componentProps } = splitViewProps(props);
  return renderLazyWrappedView(props, <WebBrowserPanelLazy {...(componentProps as any)} />);
};
