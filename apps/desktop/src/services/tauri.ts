import { invoke } from "@tauri-apps/api/core";

export interface IpcErrorPayload {
  code: string;
  message: string;
}

export class ShenDeskIpcError extends Error {
  readonly code: string;

  constructor(payload: IpcErrorPayload) {
    super(payload.message);
    this.name = "ShenDeskIpcError";
    this.code = payload.code;
  }
}

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

function normalizeIpcError(error: unknown): ShenDeskIpcError {
  if (isIpcErrorPayload(error)) {
    return new ShenDeskIpcError(error);
  }

  return new ShenDeskIpcError({
    code: "unknown_error",
    message: error instanceof Error ? error.message : String(error),
  });
}

function isIpcErrorPayload(value: unknown): value is IpcErrorPayload {
  if (typeof value !== "object" || value === null) {
    return false;
  }

  const candidate = value as Record<string, unknown>;
  return typeof candidate.code === "string" && typeof candidate.message === "string";
}
