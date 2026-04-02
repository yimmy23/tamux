import { MissionDeck } from "./base-components/MissionDeck";
import {
  AgentChatDockShell,
  AgentChatPanel,
  AgentChatPanelChatSurface,
  AgentChatPanelContextSurface,
  AgentChatPanelCurrentSurface,
  AgentChatPanelGraphSurface,
  AgentChatPanelHeader,
  AgentChatPanelProvider,
  AgentChatPanelTabs,
  AgentChatPanelThreadsSurface,
  AgentChatPanelTraceSurface,
  AgentChatPanelUsageSurface,
} from "./base-components/agentChatViews";
import {
  CommandHistoryPicker,
  CommandLogPanel,
  CommandPalette,
  ExecutionCanvas,
  FileManagerPanel,
  NotificationPanel,
  SearchOverlay,
  SessionVaultPanel,
  SettingsPanel,
  SnippetPicker,
  SystemMonitorPanel,
  TimeTravelSlider,
  WebBrowserPanel,
} from "./base-components/lazyViews";
import {
  Button,
  Container,
  Divider,
  Header,
  Input,
  Select,
  Spacer,
  Text,
  TextArea,
  UnknownComponent,
} from "./base-components/primitives";
import {
  AgentApprovalOverlay,
  LayoutContainer,
  Sidebar,
  StatusBar,
  SurfaceTabBar,
  TitleBar,
} from "./base-components/staticViews";

export { AppRuntimeBridge } from "./base-components/AppRuntimeBridge";
export { MissionDeck } from "./base-components/MissionDeck";
export { ViewMount } from "./base-components/ViewMount";

export {
  AgentApprovalOverlay,
  AgentChatDockShell,
  AgentChatPanel,
  AgentChatPanelChatSurface,
  AgentChatPanelContextSurface,
  AgentChatPanelCurrentSurface,
  AgentChatPanelGraphSurface,
  AgentChatPanelHeader,
  AgentChatPanelProvider,
  AgentChatPanelTabs,
  AgentChatPanelThreadsSurface,
  AgentChatPanelTraceSurface,
  AgentChatPanelUsageSurface,
  Button,
  CommandHistoryPicker,
  CommandLogPanel,
  CommandPalette,
  Container,
  Divider,
  ExecutionCanvas,
  FileManagerPanel,
  Header,
  Input,
  LayoutContainer,
  NotificationPanel,
  SearchOverlay,
  Select,
  SessionVaultPanel,
  SettingsPanel,
  Sidebar,
  SnippetPicker,
  Spacer,
  StatusBar,
  SurfaceTabBar,
  SystemMonitorPanel,
  Text,
  TextArea,
  TimeTravelSlider,
  TitleBar,
  UnknownComponent,
  WebBrowserPanel,
};

export const TitleBarView = TitleBar;
export const SurfaceTabBarView = SurfaceTabBar;
export const LayoutContainerView = LayoutContainer;
export const StatusBarView = StatusBar;
export const SidebarView = Sidebar;
export const MissionDeckView = MissionDeck;
export const CommandPaletteView = CommandPalette;
export const NotificationPanelView = NotificationPanel;
export const SettingsPanelView = SettingsPanel;
export const SessionVaultPanelView = SessionVaultPanel;
export const CommandLogPanelView = CommandLogPanel;
export const CommandHistoryPickerView = CommandHistoryPicker;
export const SearchOverlayView = SearchOverlay;
export const AgentChatPanelView = AgentChatPanel;
export const AgentChatDockShellView = AgentChatDockShell;
export const AgentChatPanelProviderView = AgentChatPanelProvider;
export const AgentChatPanelHeaderView = AgentChatPanelHeader;
export const AgentChatPanelTabsView = AgentChatPanelTabs;
export const AgentChatPanelCurrentSurfaceView = AgentChatPanelCurrentSurface;
export const AgentChatPanelThreadsSurfaceView = AgentChatPanelThreadsSurface;
export const AgentChatPanelChatSurfaceView = AgentChatPanelChatSurface;
export const AgentChatPanelTraceSurfaceView = AgentChatPanelTraceSurface;
export const AgentChatPanelUsageSurfaceView = AgentChatPanelUsageSurface;
export const AgentChatPanelContextSurfaceView = AgentChatPanelContextSurface;
export const AgentChatPanelGraphSurfaceView = AgentChatPanelGraphSurface;
export const SnippetPickerView = SnippetPicker;
export const SystemMonitorPanelView = SystemMonitorPanel;
export const FileManagerPanelView = FileManagerPanel;
export const TimeTravelSliderView = TimeTravelSlider;
export const ExecutionCanvasView = ExecutionCanvas;
export const WebBrowserPanelView = WebBrowserPanel;
export const AgentApprovalOverlayView = AgentApprovalOverlay;
