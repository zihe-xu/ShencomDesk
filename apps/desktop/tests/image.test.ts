import assert from "node:assert/strict";
import test from "node:test";

import {
  compressImagesWithInvoker,
  type CompressImagesResult,
  type CompressionProgress,
  type ProgressChannel,
} from "../src/services/image-core.ts";

test("invokes image compression with the request and progress channel", async () => {
  const expected: CompressImagesResult = {
    total: 1,
    succeeded: 1,
    skipped: 0,
    failed: 0,
    totalOriginalBytes: 100,
    totalCompressedBytes: 60,
    outputDir: "/tmp/output",
  };
  let receivedCommand = "";
  let receivedArgs: Record<string, unknown> | undefined;
  const channel: ProgressChannel<CompressionProgress> = { onmessage: null };
  const receivedProgress: CompressionProgress[] = [];
  const invoke = async <T>(
    command: string,
    args?: Record<string, unknown>,
  ): Promise<T> => {
    receivedCommand = command;
    receivedArgs = args;
    return expected as T;
  };
  const request = {
    items: ["/tmp/photo.jpg"],
    outputDir: "/tmp/output",
    quality: 75,
  };

  const result = await compressImagesWithInvoker(
    invoke,
    () => channel,
    request,
    (progress) => receivedProgress.push(progress),
  );
  const progress: CompressionProgress = {
    index: 1,
    total: 1,
    fileName: "photo.jpg",
    status: "completed",
    originalBytes: 100,
    compressedBytes: 60,
    error: null,
  };
  channel.onmessage?.(progress);

  assert.equal(receivedCommand, "compress_images");
  assert.deepEqual(receivedArgs, { request, onProgress: channel });
  assert.equal(result, expected);
  assert.deepEqual(receivedProgress, [progress]);
});
