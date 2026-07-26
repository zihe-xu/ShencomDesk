import { invokeCommand } from "./tauri";

export type ThemePreference = "dark" | "light" | "system";

export interface AppConfig {
  schemaVersion: number;
  theme: ThemePreference;
  language: string;
  autoStart: boolean;
}

export function getConfig(): Promise<AppConfig> {
  return invokeCommand<AppConfig>("get_config");
}

export function saveConfig(config: AppConfig): Promise<AppConfig> {
  return invokeCommand<AppConfig>("save_config", { config });
}

export function resetConfig(): Promise<AppConfig> {
  return invokeCommand<AppConfig>("reset_config");
}
