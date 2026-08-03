export type ThemePreference = "dark" | "light" | "system";

export interface AppConfig {
  schemaVersion: number;
  theme: ThemePreference;
  language: string;
  autoStart: boolean;
}

export type CommandInvoker = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;

export function getConfigWithInvoker(invoke: CommandInvoker): Promise<AppConfig> {
  return invoke<AppConfig>("get_config");
}

export function saveConfigWithInvoker(
  invoke: CommandInvoker,
  config: AppConfig,
): Promise<AppConfig> {
  return invoke<AppConfig>("save_config", { config });
}
