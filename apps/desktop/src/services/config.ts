import { invokeCommand } from "./tauri";

import {
  getConfigWithInvoker,
  saveConfigWithInvoker,
  type AppConfig,
} from "./config-core";

export type { AppConfig, ThemePreference } from "./config-core";

export function getConfig(): Promise<AppConfig> {
  return getConfigWithInvoker(invokeCommand);
}

export function saveConfig(config: AppConfig): Promise<AppConfig> {
  return saveConfigWithInvoker(invokeCommand, config);
}

export function resetConfig(): Promise<AppConfig> {
  return invokeCommand<AppConfig>("reset_config");
}
