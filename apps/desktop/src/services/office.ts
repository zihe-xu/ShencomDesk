import { Channel } from "@tauri-apps/api/core";

import {
  closeOfficeDocumentWithInvoker,
  getOfficeEngineStatusWithInvoker,
  type CloseOfficeDocumentRequest,
  type CloseOfficeDocumentResult,
  type OfficeProgress,
} from "./office-core";
import { invokeCommand } from "./tauri";

export type {
  CloseOfficeDocumentRequest,
  CloseOfficeDocumentResult,
  OfficeEngineState,
  OfficeEngineStatus,
  OfficeProgress,
  OfficeProgressStage,
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
