import { invokeCommand } from "./tauri";

export type PluginStatus = "disabled" | "enabled";

export interface PluginCommand {
  name: string;
  export: string;
  description?: string;
}

export interface PluginManifest {
  apiVersion: number;
  id: string;
  name: string;
  version: string;
  entrypoint: string;
  description?: string;
  commands: PluginCommand[];
}

export interface PluginSnapshot {
  manifest: PluginManifest;
  status: PluginStatus;
  installedAtUnixMs: number;
  updatedAtUnixMs: number;
}

export interface PluginExecution {
  pluginId: string;
  command: string;
  returnCode: number;
  fuelConsumed: number;
}

export interface InstallPluginRequest {
  manifestPath: string;
}

export interface ExecutePluginCommandRequest {
  pluginId: string;
  command: string;
}

export function installPlugin(
  request: InstallPluginRequest,
): Promise<PluginSnapshot> {
  return invokeCommand<PluginSnapshot>("install_plugin", { request });
}

export function listPlugins(): Promise<PluginSnapshot[]> {
  return invokeCommand<PluginSnapshot[]>("list_plugins");
}

export function getPlugin(pluginId: string): Promise<PluginSnapshot> {
  return invokeCommand<PluginSnapshot>("get_plugin", { pluginId });
}

export function enablePlugin(pluginId: string): Promise<PluginSnapshot> {
  return invokeCommand<PluginSnapshot>("enable_plugin", { pluginId });
}

export function disablePlugin(pluginId: string): Promise<PluginSnapshot> {
  return invokeCommand<PluginSnapshot>("disable_plugin", { pluginId });
}

export function executePluginCommand(
  request: ExecutePluginCommandRequest,
): Promise<PluginExecution> {
  return invokeCommand<PluginExecution>("execute_plugin_command", { request });
}

export function uninstallPlugin(pluginId: string): Promise<string> {
  return invokeCommand<string>("uninstall_plugin", { pluginId });
}
