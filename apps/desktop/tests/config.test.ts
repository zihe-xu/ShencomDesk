import assert from "node:assert/strict";
import test from "node:test";

import {
  getConfigWithInvoker,
  saveConfigWithInvoker,
  type AppConfig,
} from "../src/services/config-core.ts";

const config = {
  schemaVersion: 1,
  theme: "system",
  language: "zh-CN",
  autoStart: true,
} satisfies AppConfig;

test("invokes the config command without arguments", async () => {
  let receivedCommand = "";
  let receivedArgs: Record<string, unknown> | undefined;
  const invoke = async <T>(
    command: string,
    args?: Record<string, unknown>,
  ): Promise<T> => {
    receivedCommand = command;
    receivedArgs = args;
    return config as T;
  };

  const response = await getConfigWithInvoker(invoke);

  assert.equal(receivedCommand, "get_config");
  assert.equal(receivedArgs, undefined);
  assert.equal(response, config);
});

test("saves the selected theme in the config envelope", async () => {
  const darkConfig = { ...config, theme: "dark" } satisfies AppConfig;
  let receivedCommand = "";
  let receivedArgs: Record<string, unknown> | undefined;
  const invoke = async <T>(
    command: string,
    args?: Record<string, unknown>,
  ): Promise<T> => {
    receivedCommand = command;
    receivedArgs = args;
    return darkConfig as T;
  };

  const response = await saveConfigWithInvoker(invoke, darkConfig);

  assert.equal(receivedCommand, "save_config");
  assert.deepEqual(receivedArgs, { config: darkConfig });
  assert.equal(response, darkConfig);
});
