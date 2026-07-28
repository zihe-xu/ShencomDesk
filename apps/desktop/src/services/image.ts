import { Channel } from "@tauri-apps/api/core";

import {
  compressImagesWithInvoker,
  type CompressImagesRequest,
  type CompressImagesResult,
  type CompressionProgress,
} from "./image-core";
import { invokeCommand } from "./tauri";

export type {
  CompressImagesRequest,
  CompressImagesResult,
  CompressionProgress,
  CompressionStatus,
} from "./image-core";

export function compressImages(
  request: CompressImagesRequest,
  onProgress: (progress: CompressionProgress) => void,
): Promise<CompressImagesResult> {
  return compressImagesWithInvoker(
    invokeCommand,
    () => new Channel<CompressionProgress>(),
    request,
    onProgress,
  );
}
