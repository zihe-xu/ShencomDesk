import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  applyOfficeOperationsWithInvoker,
  closeOfficeDocumentWithInvoker,
  createOfficeDocumentWithInvoker,
  getOfficeEngineStatusWithInvoker,
  inspectOfficeDocumentWithInvoker,
  renderOfficePreviewWithInvoker,
  type OfficeProgress,
  type ProgressChannel,
} from "../src/services/office-core.ts";

test("invokes the typed engine status command", async () => {
  let receivedCommand = "";
  let receivedArgs: Record<string, unknown> | undefined;
  const expected = { state: "ready" as const, version: "1.0.143" };
  const result = await getOfficeEngineStatusWithInvoker(
    async <T>(command: string, args?: Record<string, unknown>) => {
      receivedCommand = command;
      receivedArgs = args;
      return expected as T;
    },
  );

  assert.equal(receivedCommand, "get_office_engine_status");
  assert.equal(receivedArgs, undefined);
  assert.deepEqual(result, expected);
});

test("invokes typed create, inspect, batch, and preview commands", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const createChannel = (): ProgressChannel<OfficeProgress> => ({ onmessage: null });
  const invoke = async <T>(command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args });
    return {} as T;
  };
  const onProgress = () => undefined;
  const operations = [
    { type: "add_word_paragraph" as const, text: "fixture" },
  ];

  await createOfficeDocumentWithInvoker(
    invoke,
    createChannel,
    { path: "/tmp/new.docx" },
    onProgress,
  );
  await inspectOfficeDocumentWithInvoker(
    invoke,
    createChannel,
    { path: "/tmp/source.docx" },
    onProgress,
  );
  await applyOfficeOperationsWithInvoker(
    invoke,
    createChannel,
    {
      path: "/tmp/source.docx",
      outputPath: "/tmp/output.docx",
      operations,
    },
    onProgress,
  );
  await renderOfficePreviewWithInvoker(
    invoke,
    createChannel,
    { path: "/tmp/output.docx", page: 1 },
    onProgress,
  );

  assert.deepEqual(
    calls.map(({ command }) => command),
    [
      "create_office_document",
      "inspect_office_document",
      "apply_office_operations",
      "render_office_preview",
    ],
  );
  assert.deepEqual(calls[2]?.args?.request, {
    path: "/tmp/source.docx",
    outputPath: "/tmp/output.docx",
    operations,
  });
  for (const { args } of calls) {
    assert.ok(args?.onProgress);
    assert.equal("binaryPath" in (args?.request as object), false);
    assert.equal("argv" in (args?.request as object), false);
  }
});

test("invokes close with a camelCase progress channel envelope", async () => {
  let receivedCommand = "";
  let receivedArgs: Record<string, unknown> | undefined;
  const channel: ProgressChannel<OfficeProgress> = { onmessage: null };
  const progressEvents: OfficeProgress[] = [];
  const request = { path: "/tmp/report.docx" };

  const result = await closeOfficeDocumentWithInvoker(
    async <T>(command: string, args?: Record<string, unknown>) => {
      receivedCommand = command;
      receivedArgs = args;
      return { succeeded: true } as T;
    },
    () => channel,
    request,
    (progress) => progressEvents.push(progress),
  );
  channel.onmessage?.({ stage: "closing" });
  channel.onmessage?.({ stage: "completed" });

  assert.equal(receivedCommand, "close_office_document");
  assert.deepEqual(receivedArgs, { request, onProgress: channel });
  assert.deepEqual(result, { succeeded: true });
  assert.deepEqual(progressEvents, [
    { stage: "closing" },
    { stage: "completed" },
  ]);
  assert.equal("binaryPath" in request, false);
  assert.equal("environment" in request, false);
  assert.equal("argv" in request, false);
});

test("registers only explicit Office permissions for the main window", async () => {
  const capabilityUrl = new URL(
    "../src-tauri/capabilities/default.json",
    import.meta.url,
  );
  const capability = JSON.parse(await readFile(capabilityUrl, "utf8")) as {
    permissions: string[];
  };

  assert.ok(capability.permissions.includes("allow-get-office-engine-status"));
  assert.ok(capability.permissions.includes("allow-create-office-document"));
  assert.ok(capability.permissions.includes("allow-inspect-office-document"));
  assert.ok(capability.permissions.includes("allow-apply-office-operations"));
  assert.ok(capability.permissions.includes("allow-render-office-preview"));
  assert.ok(capability.permissions.includes("allow-close-office-document"));
  assert.equal(
    capability.permissions.some((permission) => permission.startsWith("shell:")),
    false,
  );

  const buildScript = await readFile(
    new URL("../src-tauri/build.rs", import.meta.url),
    "utf8",
  );
  const invokeHandler = await readFile(
    new URL("../src-tauri/src/lib.rs", import.meta.url),
    "utf8",
  );
  const officeRuntime = await readFile(
    new URL("../src-tauri/src/infrastructure/office/mod.rs", import.meta.url),
    "utf8",
  );
  const tauriConfig = await readFile(
    new URL("../src-tauri/tauri.conf.json", import.meta.url),
    "utf8",
  );
  for (const command of [
    "get_office_engine_status",
    "create_office_document",
    "inspect_office_document",
    "apply_office_operations",
    "render_office_preview",
    "close_office_document",
  ]) {
    assert.match(buildScript, new RegExp(`"${command}"`));
    assert.match(invokeHandler, new RegExp(`commands::office::${command}`));
    const permission = await readFile(
      new URL(
        `../src-tauri/permissions/autogenerated/${command}.toml`,
        import.meta.url,
      ),
      "utf8",
    );
    assert.match(permission, new RegExp(`commands\\.allow = \\["${command}"\\]`));
  }
  assert.doesNotMatch(buildScript, /execute_office_cli/);
  assert.doesNotMatch(invokeHandler, /execute_office_cli/);
  assert.doesNotMatch(officeRuntime, /OsString::from\("watch"\)/);
  assert.doesNotMatch(officeRuntime, /OsString::from\("html"\)/);
  assert.doesNotMatch(tauriConfig, /26315/);
});
