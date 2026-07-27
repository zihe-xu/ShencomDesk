export const IPC_ERROR_CODES = [
  "database_unavailable",
  "config_load_failed",
  "config_save_failed",
  "config_reset_failed",
  "task_not_found",
  "task_queue_unavailable",
  "file_not_found",
  "file_access_denied",
  "file_too_large",
  "file_not_text",
  "file_watch_unavailable",
  "file_watch_not_found",
  "file_operation_failed",
  "validation_failed",
  "unknown_error",
] as const;

export type IpcErrorCode = (typeof IPC_ERROR_CODES)[number];

export interface IpcErrorPayload {
  code: IpcErrorCode;
  message: string;
}

export class ShenDeskIpcError extends Error {
  readonly code: IpcErrorCode;

  constructor(payload: IpcErrorPayload) {
    super(payload.message);
    this.name = "ShenDeskIpcError";
    this.code = payload.code;
  }
}

export function normalizeIpcError(error: unknown): ShenDeskIpcError {
  if (isIpcErrorPayload(error)) {
    return new ShenDeskIpcError(error);
  }

  return new ShenDeskIpcError({
    code: "unknown_error",
    message: "操作失败，请重试。",
  });
}

function isIpcErrorPayload(value: unknown): value is IpcErrorPayload {
  if (typeof value !== "object" || value === null) {
    return false;
  }

  const candidate = value as Record<string, unknown>;
  return (
    isIpcErrorCode(candidate.code) &&
    typeof candidate.message === "string" &&
    candidate.message.length > 0
  );
}

function isIpcErrorCode(value: unknown): value is IpcErrorCode {
  return (
    typeof value === "string" &&
    (IPC_ERROR_CODES as readonly string[]).includes(value)
  );
}
