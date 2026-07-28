import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { resolveBuildDecision } from "./resolve-post-merge-build.mjs";
import {
  resolveBuildProcess,
  runTauriBuild,
} from "./run-tauri-build.mjs";

const mergeSha = "0123456789abcdef0123456789abcdef01234567";

test("merged PR without skip-build triggers the merge commit build", () => {
  const decision = resolveBuildDecision({
    eventName: "pull_request",
    sha: "default-branch-sha",
    payload: {
      number: 37,
      pull_request: {
        number: 37,
        merged: true,
        merge_commit_sha: mergeSha,
        labels: [{ name: "ci" }],
      },
    },
  });

  assert.equal(decision.shouldBuild, true);
  assert.equal(decision.buildSha, mergeSha);
  assert.equal(decision.shortSha, mergeSha.slice(0, 12));
  assert.equal(decision.prNumber, "37");
  assert.equal(decision.fatal, false);
});

test("merged PR with skip-build skips all build jobs", () => {
  const decision = resolveBuildDecision({
    eventName: "pull_request",
    sha: "default-branch-sha",
    payload: {
      number: 38,
      pull_request: {
        number: 38,
        merged: true,
        merge_commit_sha: mergeSha,
        labels: [{ name: "ci" }, { name: "skip-build" }],
      },
    },
  });

  assert.equal(decision.shouldBuild, false);
  assert.equal(decision.buildSha, mergeSha);
  assert.match(decision.reason, /skip-build/);
  assert.equal(decision.fatal, false);
});

test("closed but unmerged PR does not build", () => {
  const decision = resolveBuildDecision({
    eventName: "pull_request",
    sha: "default-branch-sha",
    payload: {
      number: 39,
      pull_request: {
        number: 39,
        merged: false,
        merge_commit_sha: null,
        labels: [],
      },
    },
  });

  assert.equal(decision.shouldBuild, false);
  assert.equal(decision.buildSha, "");
  assert.match(decision.reason, /未合并/);
  assert.equal(decision.fatal, false);
});

test("workflow_dispatch builds the selected commit", () => {
  const decision = resolveBuildDecision({
    eventName: "workflow_dispatch",
    sha: mergeSha,
    payload: {},
  });

  assert.equal(decision.shouldBuild, true);
  assert.equal(decision.buildSha, mergeSha);
  assert.equal(decision.prNumber, "-");
  assert.equal(decision.trigger, "workflow_dispatch");
});

test("merged PR without merge_commit_sha fails closed", () => {
  const decision = resolveBuildDecision({
    eventName: "pull_request",
    sha: "default-branch-sha",
    payload: {
      number: 40,
      pull_request: {
        number: 40,
        merged: true,
        merge_commit_sha: null,
        labels: [],
      },
    },
  });

  assert.equal(decision.shouldBuild, false);
  assert.equal(decision.fatal, true);
  assert.match(decision.reason, /merge_commit_sha/);
});

test("workflow pins the Windows runner and retains failed build diagnostics", () => {
  const workflow = readFileSync(
    new URL("../workflows/build.yml", import.meta.url),
    "utf8",
  );

  assert.match(workflow, /runner:\s+windows-2022/);
  assert.doesNotMatch(workflow, /runner:\s+windows-latest/);
  assert.match(workflow, /run-tauri-build\.mjs/);
  assert.match(workflow, /Upload failed build diagnostics/);
  assert.match(workflow, /retention-days:\s+7/);
});

test("platform configs declare generated installer icons", () => {
  const configRoot = new URL(
    "../../apps/desktop/src-tauri/",
    import.meta.url,
  );
  const baseConfig = JSON.parse(
    readFileSync(new URL("tauri.conf.json", configRoot), "utf8"),
  );
  const windowsConfig = JSON.parse(
    readFileSync(new URL("tauri.windows.conf.json", configRoot), "utf8"),
  );
  const macosConfig = JSON.parse(
    readFileSync(new URL("tauri.macos.conf.json", configRoot), "utf8"),
  );

  assert.equal(baseConfig.bundle.icon, undefined);
  assert.deepEqual(windowsConfig.bundle.icon, ["icons/icon.ico"]);
  assert.deepEqual(macosConfig.bundle.icon, ["icons/icon.icns"]);
});

test("Windows build invokes pnpm through cmd.exe", () => {
  const command = resolveBuildProcess("win32", {
    ComSpec: "C:\\Windows\\System32\\cmd.exe",
  });

  assert.deepEqual(command, {
    command: "C:\\Windows\\System32\\cmd.exe",
    args: ["/d", "/s", "/c", "pnpm run tauri -- build"],
  });
});

test("non-Windows build invokes pnpm directly", () => {
  assert.deepEqual(resolveBuildProcess("linux", {}), {
    command: "pnpm",
    args: ["run", "tauri", "--", "build"],
  });
});

test("synchronous spawn failures are retained in the diagnostics file", async () => {
  const directory = mkdtempSync(join(tmpdir(), "shendesk-build-wrapper-"));
  const logPath = join(directory, "tauri-build.log");
  const sink = { write() {} };

  try {
    const exitCode = await runTauriBuild({
      platform: "win32",
      env: { ComSpec: "cmd.exe" },
      logPath,
      stdout: sink,
      stderr: sink,
      spawnProcess() {
        throw new Error("synthetic spawn failure");
      },
    });

    assert.equal(exitCode, 1);
    assert.match(readFileSync(logPath, "utf8"), /synthetic spawn failure/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
