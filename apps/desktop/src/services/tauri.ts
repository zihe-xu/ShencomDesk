import { invoke } from "@tauri-apps/api/core";

import { normalizeIpcError } from "./tauri-errors";

export * from "./tauri-errors";

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}
