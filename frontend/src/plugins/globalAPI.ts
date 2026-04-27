import { ComponentRegistryAPI } from "../registry/componentRegistry";
import { CommandRegistryAPI } from "../registry/commandRegistry";
import { PluginManager, type Plugin } from "./PluginManager";

const getPluginManager = (): PluginManager => {
  if (!window.__tamuxPluginManager && !window.__amuxPluginManager) {
    window.__tamuxPluginManager = new PluginManager();
    window.__amuxPluginManager = window.__tamuxPluginManager;
  }

  return window.__tamuxPluginManager ?? window.__amuxPluginManager!;
};

export interface AmuxPluginAPI {
  registerComponent: typeof ComponentRegistryAPI.register;
  registerCommand: typeof CommandRegistryAPI.register;
  registerPlugin: (plugin: Plugin) => void;
  unregisterPlugin: (id: string) => void;
  getComponents: typeof ComponentRegistryAPI.list;
  getCommands: typeof CommandRegistryAPI.list;
  getPlugins: () => string[];
}

declare global {
  interface Window {
    TamuxApi?: AmuxPluginAPI;
    AmuxApi?: AmuxPluginAPI;
    __tamuxPluginManager?: PluginManager;
    __amuxPluginManager?: PluginManager;
  }
}

const pluginManager = getPluginManager();

const pluginApi: AmuxPluginAPI = {
  registerComponent: ComponentRegistryAPI.register,
  registerCommand: CommandRegistryAPI.register,
  registerPlugin: (plugin: Plugin) => {
    pluginManager.registerPlugin(plugin);
  },
  unregisterPlugin: (id: string) => {
    pluginManager.unregisterPlugin(id);
  },
  getComponents: ComponentRegistryAPI.list,
  getCommands: CommandRegistryAPI.list,
  getPlugins: () => pluginManager.listPlugins(),
};

window.TamuxApi = pluginApi;
window.AmuxApi = pluginApi;

export { pluginManager };
