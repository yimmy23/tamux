import { ComponentRegistryAPI } from "../registry/componentRegistry";
import { CommandRegistryAPI } from "../registry/commandRegistry";
import { PluginManager, type Plugin } from "./PluginManager";

const getPluginManager = (): PluginManager => {
  if (!window.__zoraiPluginManager) {
    window.__zoraiPluginManager = new PluginManager();
  }

  return window.__zoraiPluginManager!;
};

export interface ZoraiPluginAPI {
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
    ZoraiApi?: ZoraiPluginAPI;
    __zoraiPluginManager?: PluginManager;
  }
}

const pluginManager = getPluginManager();

const pluginApi: ZoraiPluginAPI = {
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

window.ZoraiApi = pluginApi;

export { pluginManager };
