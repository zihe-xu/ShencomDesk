export type CompressionStatus =
  | "processing"
  | "completed"
  | "skipped"
  | "failed";

export interface CompressImagesRequest {
  items: string[];
  outputDir: string;
  quality: number;
}

export interface CompressionProgress {
  index: number;
  total: number;
  fileName: string;
  status: CompressionStatus;
  originalBytes: number;
  compressedBytes: number;
  error: string | null;
}

export interface CompressImagesResult {
  total: number;
  succeeded: number;
  skipped: number;
  failed: number;
  totalOriginalBytes: number;
  totalCompressedBytes: number;
  outputDir: string;
}

export interface ProgressChannel<T> {
  onmessage: ((message: T) => void) | null;
}

export type CommandInvoker = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;

export function compressImagesWithInvoker(
  invoke: CommandInvoker,
  createChannel: () => ProgressChannel<CompressionProgress>,
  request: CompressImagesRequest,
  onProgress: (progress: CompressionProgress) => void,
): Promise<CompressImagesResult> {
  const onProgressChannel = createChannel();
  onProgressChannel.onmessage = onProgress;

  return invoke<CompressImagesResult>("compress_images", {
    request,
    onProgress: onProgressChannel,
  });
}
