export type OfficeEngineState = "ready" | "unavailable";

export interface OfficeEngineStatus {
  state: OfficeEngineState;
  version: string | null;
}

export interface CloseOfficeDocumentRequest {
  path: string;
}

export interface CreateOfficeDocumentRequest {
  path: string;
}

export interface InspectOfficeDocumentRequest {
  path: string;
}

export type OfficeDocumentOperation =
  | { type: "add_word_paragraph"; text: string }
  | { type: "set_spreadsheet_cell"; cell: string; value: string }
  | { type: "add_presentation_slide"; title: string }
  | { type: "add_presentation_text"; slide: number; text: string };

export interface ApplyOfficeOperationsRequest {
  path: string;
  outputPath: string;
  operations: OfficeDocumentOperation[];
}

export interface RenderOfficePreviewRequest {
  path: string;
  page?: number;
}

export type OfficeProgressStage =
  | "creating"
  | "inspecting"
  | "applying"
  | "rendering"
  | "closing"
  | "completed";

export interface OfficeProgress {
  stage: OfficeProgressStage;
}

export interface CloseOfficeDocumentResult {
  succeeded: boolean;
}

export interface CreateOfficeDocumentResult {
  succeeded: boolean;
}

export interface ApplyOfficeOperationsResult {
  succeeded: boolean;
  operationCount: number;
}

export type OfficeDocumentFormat = "word" | "spreadsheet" | "presentation";

export interface OfficeInspection {
  format: OfficeDocumentFormat;
  structure: JsonValue;
}

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface OfficePreview {
  mimeType: "image/png";
  dataUrl: `data:image/png;base64,${string}`;
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
  return invokeOfficeCommandWithProgress(
    invoke,
    createChannel,
    "close_office_document",
    request,
    onProgress,
  );
}

export function createOfficeDocumentWithInvoker(
  invoke: OfficeCommandInvoker,
  createChannel: () => ProgressChannel<OfficeProgress>,
  request: CreateOfficeDocumentRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<CreateOfficeDocumentResult> {
  return invokeOfficeCommandWithProgress(
    invoke,
    createChannel,
    "create_office_document",
    request,
    onProgress,
  );
}

export function inspectOfficeDocumentWithInvoker(
  invoke: OfficeCommandInvoker,
  createChannel: () => ProgressChannel<OfficeProgress>,
  request: InspectOfficeDocumentRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<OfficeInspection> {
  return invokeOfficeCommandWithProgress(
    invoke,
    createChannel,
    "inspect_office_document",
    request,
    onProgress,
  );
}

export function applyOfficeOperationsWithInvoker(
  invoke: OfficeCommandInvoker,
  createChannel: () => ProgressChannel<OfficeProgress>,
  request: ApplyOfficeOperationsRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<ApplyOfficeOperationsResult> {
  return invokeOfficeCommandWithProgress(
    invoke,
    createChannel,
    "apply_office_operations",
    request,
    onProgress,
  );
}

export function renderOfficePreviewWithInvoker(
  invoke: OfficeCommandInvoker,
  createChannel: () => ProgressChannel<OfficeProgress>,
  request: RenderOfficePreviewRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<OfficePreview> {
  return invokeOfficeCommandWithProgress(
    invoke,
    createChannel,
    "render_office_preview",
    request,
    onProgress,
  );
}

function invokeOfficeCommandWithProgress<TRequest, TResult>(
  invoke: OfficeCommandInvoker,
  createChannel: () => ProgressChannel<OfficeProgress>,
  command: string,
  request: TRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<TResult> {
  const onProgressChannel = createChannel();
  onProgressChannel.onmessage = onProgress;
  return invoke<TResult>(command, {
    request,
    onProgress: onProgressChannel,
  });
}
