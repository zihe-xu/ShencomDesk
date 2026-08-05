import assert from "node:assert/strict";
import test from "node:test";

import {
  normalizeArgs,
  resolveMode,
  selectAuthEnvironment,
} from "../scripts/run-tauri.mjs";

test("normalizes pnpm workspace argument forwarding", () => {
  assert.deepEqual(normalizeArgs(["--", "dev"]), ["dev"]);
  assert.deepEqual(normalizeArgs(["build"]), ["build"]);
});

test("selects the matching environment mode for Tauri commands", () => {
  assert.equal(resolveMode(["dev"], {}), "development");
  assert.equal(resolveMode(["build"], {}), "production");
  assert.equal(
    resolveMode(["dev"], { SHENDESK_AUTH_ENVIRONMENT: "test" }),
    "test",
  );
  assert.throws(
    () => resolveMode(["dev"], { SHENDESK_AUTH_ENVIRONMENT: "staging" }),
    /Unsupported authentication environment/,
  );
});

test("passes only authentication environment values to the Rust process", () => {
  assert.deepEqual(
    selectAuthEnvironment("test", {
      HOST: " https://test.example.com ",
      SCID: " test-scid ",
      TEST_USERNAME: "private-user",
      TEST_PASSWORD: "private-password",
    }),
    {
      SHENDESK_AUTH_ENVIRONMENT: "test",
      SHENDESK_AUTH_HOST: "https://test.example.com",
      SHENDESK_AUTH_SCID: "test-scid",
    },
  );
});

test("rejects incomplete authentication environments", () => {
  assert.throws(
    () => selectAuthEnvironment("development", { SCID: "development-scid" }),
    /HOST is missing/,
  );
  assert.throws(
    () => selectAuthEnvironment("production", { HOST: "https:\/\/example.com" }),
    /SCID is missing/,
  );
});
