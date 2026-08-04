export type OfficeEngineState = "ready" | "unavailable";

export interface OfficeEngineStatus {
  state: OfficeEngineState;
  version: string | null;
}

export interface CloseOfficeDocumentRequest {
  path: string;
}

export type OfficeProgressStage = "closing" | "completed";

export interface OfficeProgress {
  stage: OfficeProgressStage;
}

export interface CloseOfficeDocumentResult {
  succeeded: boolean;
}

export interface ProgressChannel<T> {
  onmessage: ((message: T) => void) | null;
}

export type OfficeCommandInvoker = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;

export function getOfficeEngineStatusWithInvoker(
  invoke: OfficeCommandInvoker,
): Promise<OfficeEngineStatus> {
  return invoke<OfficeEngineStatus>("get_office_engine_status");
}

export function closeOfficeDocumentWithInvoker(
  invoke: OfficeCommandInvoker,
  createChannel: () => ProgressChannel<OfficeProgress>,
  request: CloseOfficeDocumentRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<CloseOfficeDocumentResult> {
  const onProgressChannel = createChannel();
  onProgressChannel.onmessage = onProgress;
  return invoke<CloseOfficeDocumentResult>("close_office_document", {
    request,
    onProgress: onProgressChannel,
  });
}
