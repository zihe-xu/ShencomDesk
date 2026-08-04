import { Channel } from "@tauri-apps/api/core";

import {
  applyOfficeOperationsWithInvoker,
  closeOfficeDocumentWithInvoker,
  createOfficeDocumentWithInvoker,
  getOfficeEngineStatusWithInvoker,
  inspectOfficeDocumentWithInvoker,
  renderOfficePreviewWithInvoker,
  type ApplyOfficeOperationsRequest,
  type ApplyOfficeOperationsResult,
  type CloseOfficeDocumentRequest,
  type CloseOfficeDocumentResult,
  type CreateOfficeDocumentRequest,
  type CreateOfficeDocumentResult,
  type InspectOfficeDocumentRequest,
  type OfficeInspection,
  type OfficeProgress,
  type OfficePreview,
  type RenderOfficePreviewRequest,
} from "./office-core";
import { invokeCommand } from "./tauri";

export type {
  ApplyOfficeOperationsRequest,
  ApplyOfficeOperationsResult,
  CloseOfficeDocumentRequest,
  CloseOfficeDocumentResult,
  CreateOfficeDocumentRequest,
  CreateOfficeDocumentResult,
  InspectOfficeDocumentRequest,
  JsonValue,
  OfficeDocumentFormat,
  OfficeDocumentOperation,
  OfficeEngineState,
  OfficeEngineStatus,
  OfficeInspection,
  OfficeProgress,
  OfficeProgressStage,
  OfficePreview,
  RenderOfficePreviewRequest,
} from "./office-core";

export function getOfficeEngineStatus() {
  return getOfficeEngineStatusWithInvoker(invokeCommand);
}

export function closeOfficeDocument(
  request: CloseOfficeDocumentRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<CloseOfficeDocumentResult> {
  return closeOfficeDocumentWithInvoker(
    invokeCommand,
    () => new Channel<OfficeProgress>(),
    request,
    onProgress,
  );
}

export function createOfficeDocument(
  request: CreateOfficeDocumentRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<CreateOfficeDocumentResult> {
  return createOfficeDocumentWithInvoker(
    invokeCommand,
    () => new Channel<OfficeProgress>(),
    request,
    onProgress,
  );
}

export function inspectOfficeDocument(
  request: InspectOfficeDocumentRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<OfficeInspection> {
  return inspectOfficeDocumentWithInvoker(
    invokeCommand,
    () => new Channel<OfficeProgress>(),
    request,
    onProgress,
  );
}

export function applyOfficeOperations(
  request: ApplyOfficeOperationsRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<ApplyOfficeOperationsResult> {
  return applyOfficeOperationsWithInvoker(
    invokeCommand,
    () => new Channel<OfficeProgress>(),
    request,
    onProgress,
  );
}

export function renderOfficePreview(
  request: RenderOfficePreviewRequest,
  onProgress: (progress: OfficeProgress) => void,
): Promise<OfficePreview> {
  return renderOfficePreviewWithInvoker(
    invokeCommand,
    () => new Channel<OfficeProgress>(),
    request,
    onProgress,
  );
}
