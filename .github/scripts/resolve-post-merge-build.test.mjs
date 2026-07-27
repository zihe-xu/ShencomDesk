import assert from "node:assert/strict";
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
