import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { resolveBuildDecision } from "./resolve-post-merge-build.mjs";

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

test("workflow pins Windows and captures native build diagnostics", () => {
  const workflow = readFileSync(
    new URL("../workflows/build.yml", import.meta.url),
    "utf8",
  );

  assert.match(workflow, /runner:\s+windows-2022/);
  assert.doesNotMatch(workflow, /runner:\s+windows-latest/);
  assert.match(workflow, /npm run tauri -- icon app-icon\.svg/);
  assert.match(workflow, /tee tauri-build\.log/);
  assert.match(workflow, /Tee-Object -FilePath \$logPath/);
  assert.doesNotMatch(workflow, /run-tauri-build\.mjs/);
  assert.match(workflow, /Upload failed build diagnostics/);
  assert.match(workflow, /if-no-files-found:\s+error/);
  assert.match(workflow, /retention-days:\s+7/);
});

test("Tauri bundle declares every generated desktop icon format", () => {
  const config = JSON.parse(
    readFileSync(
      new URL("../../apps/desktop/src-tauri/tauri.conf.json", import.meta.url),
      "utf8",
    ),
  );

  assert.deepEqual(config.bundle.icon, [
    "icons/32x32.png",
    "icons/128x128.png",
    "icons/128x128@2x.png",
    "icons/icon.icns",
    "icons/icon.ico",
  ]);
});
