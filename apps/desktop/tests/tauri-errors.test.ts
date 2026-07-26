import assert from "node:assert/strict";
import test from "node:test";

import {
  ShenDeskIpcError,
  normalizeIpcError,
} from "../src/services/tauri-errors.ts";

test("preserves known redacted IPC errors", () => {
  const normalized = normalizeIpcError({
    code: "database_unavailable",
    message: "本地数据服务暂时不可用，请重试。",
  });

  assert.ok(normalized instanceof ShenDeskIpcError);
  assert.equal(normalized.code, "database_unavailable");
  assert.equal(normalized.message, "本地数据服务暂时不可用，请重试。");
});

test("preserves stable task errors", () => {
  const normalized = normalizeIpcError({
    code: "task_not_found",
    message: "未找到指定任务。",
  });

  assert.ok(normalized instanceof ShenDeskIpcError);
  assert.equal(normalized.code, "task_not_found");
  assert.equal(normalized.message, "未找到指定任务。");
});

test("redacts unknown objects and native errors", () => {
  const internalPath = "/Users/example/private/app.sqlite";
  const unknownPayload = normalizeIpcError({
    code: "sqlx_error",
    message: `permission denied: ${internalPath}`,
  });
  const nativeError = normalizeIpcError(
    new Error(`failed to open SQLite database ${internalPath}`),
  );

  for (const normalized of [unknownPayload, nativeError]) {
    assert.equal(normalized.code, "unknown_error");
    assert.equal(normalized.message, "操作失败，请重试。");
    assert.equal(normalized.message.includes(internalPath), false);
  }
});

test("rejects empty messages even for known codes", () => {
  const normalized = normalizeIpcError({
    code: "config_save_failed",
    message: "",
  });

  assert.equal(normalized.code, "unknown_error");
  assert.equal(normalized.message, "操作失败，请重试。");
});
