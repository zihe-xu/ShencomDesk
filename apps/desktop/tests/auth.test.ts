import assert from "node:assert/strict";
import test from "node:test";

import {
  getAuthStateWithInvoker,
  loginWithInvoker,
  logoutWithInvoker,
  type AuthState,
  type LoginRequest,
} from "../src/services/auth-core.ts";

test("invokes the typed login command with the expected request envelope", async () => {
  const request: LoginRequest = {
    username: "13800000000",
    password: "password",
  };
  const expected = {
    authenticated: true,
    user: {
      realname: "测试用户",
      phone: "13800000000",
      username: "13800000000",
      uid: "user-id",
    },
    expiresAt: 1_800_000_000,
  } satisfies AuthState;
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

test("invokes the auth state command without arguments", async () => {
  const expected = {
    authenticated: false,
    user: null,
    expiresAt: null,
  } satisfies AuthState;
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

  const response = await getAuthStateWithInvoker(invoke);

  assert.equal(receivedCommand, "get_auth_state");
  assert.equal(receivedArgs, undefined);
  assert.equal(response, expected);
});

test("invokes logout without exposing session tokens", async () => {
  const expected = {
    authenticated: false,
    user: null,
    expiresAt: null,
  } satisfies AuthState;
  let receivedCommand = "";
  const invoke = async <T>(command: string): Promise<T> => {
    receivedCommand = command;
    return expected as T;
  };

  const response = await logoutWithInvoker(invoke);

  assert.equal(receivedCommand, "logout");
  assert.equal(response, expected);
  assert.deepEqual(Object.keys(response), ["authenticated", "user", "expiresAt"]);
});
