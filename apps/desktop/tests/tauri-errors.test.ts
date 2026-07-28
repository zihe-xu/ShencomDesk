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

test("preserves stable authentication errors", () => {
  const failed = normalizeIpcError({
    code: "auth_failed",
    message: "账号或密码有误",
  });
  const unavailable = normalizeIpcError({
    code: "auth_unavailable",
    message: "登录服务暂时不可用，请稍后重试。",
  });

  assert.equal(failed.code, "auth_failed");
  assert.equal(failed.message, "账号或密码有误");
  assert.equal(unavailable.code, "auth_unavailable");
  assert.equal(unavailable.message, "登录服务暂时不可用，请稍后重试。");
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

test("preserves stable file errors", () => {
  const normalized = normalizeIpcError({
    code: "file_access_denied",
    message: "没有权限访问指定文件或目录。",
  });

  assert.ok(normalized instanceof ShenDeskIpcError);
  assert.equal(normalized.code, "file_access_denied");
  assert.equal(normalized.message, "没有权限访问指定文件或目录。");
});

test("preserves stable image errors", () => {
  const normalized = normalizeIpcError({
    code: "image_output_failed",
    message: "无法写入输出文件，文件可能已存在或目录不可写。",
  });

  assert.ok(normalized instanceof ShenDeskIpcError);
  assert.equal(normalized.code, "image_output_failed");
  assert.equal(
    normalized.message,
    "无法写入输出文件，文件可能已存在或目录不可写。",
  );
});

test("preserves stable plugin errors", () => {
  const normalized = normalizeIpcError({
    code: "plugin_invalid_package",
    message: "插件包无效或与当前版本不兼容。",
  });

  assert.ok(normalized instanceof ShenDeskIpcError);
  assert.equal(normalized.code, "plugin_invalid_package");
  assert.equal(normalized.message, "插件包无效或与当前版本不兼容。");
});

test("preserves stable update errors", () => {
  const normalized = normalizeIpcError({
    code: "update_install_failed",
    message: "更新下载、验证或安装失败，请重试。",
  });

  assert.ok(normalized instanceof ShenDeskIpcError);
  assert.equal(normalized.code, "update_install_failed");
  assert.equal(normalized.message, "更新下载、验证或安装失败，请重试。");
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
