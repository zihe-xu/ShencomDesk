import assert from "node:assert/strict";
import test from "node:test";

import {
  loginWithInvoker,
  type LoginRequest,
  type LoginResponse,
} from "../src/services/auth-core.ts";

test("invokes the typed login command with the expected request envelope", async () => {
  const request: LoginRequest = {
    username: "13800000000",
    password: "password",
  };
  const expected = {
    data: {
      additionalInformation: {
        additionalInformation: {
          realname: "测试用户",
          phone: "13800000000",
          username: "13800000000",
          uid: "user-id",
        },
        expiration: 1_800_000_000,
        expiresIn: 3_600,
        refreshToken: { value: "refresh-token" },
        scope: ["all"],
        tokenType: "bearer",
        value: "access-token",
      },
    },
    errcode: "0000",
    errmsg: "",
  } satisfies LoginResponse;
  let receivedCommand = "";
  let receivedArgs: Record<string, unknown> | undefined;
  const invoke = async <T>(
    command: string,
    args?: Record<string, unknown>,
  ): Promise<T> => {
    receivedCommand = command;
    receivedArgs = args;
    return expected as T;
  };

  const response = await loginWithInvoker(invoke, request);

  assert.equal(receivedCommand, "login");
  assert.deepEqual(receivedArgs, { request });
  assert.equal(response, expected);
});
