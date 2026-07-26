import { invokeCommand } from "./tauri";

export interface HealthStatus {
  status: string;
  appName: string;
  version: string;
  uptimeSeconds: number;
}

export function getHealthStatus(): Promise<HealthStatus> {
  return invokeCommand<HealthStatus>("health_check");
}
