import { appendFileSync, readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export const DEFAULT_SKIP_BUILD_LABEL = "skip-build";

function normalizeLabels(labels) {
  if (!Array.isArray(labels)) {
    return [];
  }

  return labels.flatMap((label) => {
    if (typeof label === "string") {
      return [label];
    }

    if (label && typeof label.name === "string") {
      return [label.name];
    }

    return [];
  });
}

function createDecision({
  shouldBuild,
  buildSha = "",
  prNumber = "-",
  trigger,
  reason,
  fatal = false,
}) {
  return {
    shouldBuild,
    buildSha,
    shortSha: buildSha ? buildSha.slice(0, 12) : "not-applicable",
    prNumber: String(prNumber),
    trigger,
    reason,
    fatal,
  };
}

export function resolveBuildDecision({
  eventName,
  sha,
  payload = {},
  skipLabel = DEFAULT_SKIP_BUILD_LABEL,
}) {
  if (eventName === "workflow_dispatch") {
    if (!sha) {
      return createDecision({
        shouldBuild: false,
        trigger: eventName,
        reason: "手动触发事件缺少目标提交 SHA",
        fatal: true,
      });
    }

    return createDecision({
      shouldBuild: true,
      buildSha: sha,
      trigger: eventName,
      reason: "通过 workflow_dispatch 手动触发完整构建",
    });
  }

  if (eventName !== "pull_request") {
    return createDecision({
      shouldBuild: false,
      trigger: eventName || "unknown",
      reason: `不支持的触发事件：${eventName || "unknown"}`,
      fatal: true,
    });
  }

  const pullRequest = payload.pull_request;
  if (!pullRequest || typeof pullRequest !== "object") {
    return createDecision({
      shouldBuild: false,
      trigger: eventName,
      reason: "pull_request 事件缺少 Pull Request 数据",
      fatal: true,
    });
  }

  const prNumber = pullRequest.number ?? payload.number ?? "-";

  if (pullRequest.merged !== true) {
    return createDecision({
      shouldBuild: false,
      prNumber,
      trigger: eventName,
      reason: `PR #${prNumber} 已关闭但未合并到 main`,
    });
  }

  const mergeSha =
    typeof pullRequest.merge_commit_sha === "string"
      ? pullRequest.merge_commit_sha.trim()
      : "";

  if (!mergeSha) {
    return createDecision({
      shouldBuild: false,
      prNumber,
      trigger: eventName,
      reason: `PR #${prNumber} 已合并，但事件中缺少 merge_commit_sha`,
      fatal: true,
    });
  }

  const labels = normalizeLabels(pullRequest.labels);
  if (labels.includes(skipLabel)) {
    return createDecision({
      shouldBuild: false,
      buildSha: mergeSha,
      prNumber,
      trigger: eventName,
      reason: `PR #${prNumber} 包含 ${skipLabel} 标签，跳过自动化构建`,
    });
  }

  return createDecision({
    shouldBuild: true,
    buildSha: mergeSha,
    prNumber,
    trigger: eventName,
    reason: `PR #${prNumber} 已合并到 main，执行自动化构建`,
  });
}

function writeOutput(name, value) {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (!outputPath) {
    return;
  }

  const normalized = String(value).replace(/[\r\n]+/g, " ");
  appendFileSync(outputPath, `${name}=${normalized}\n`, "utf8");
}

function escapeMarkdown(value) {
  return String(value).replaceAll("|", "\\|").replace(/[\r\n]+/g, "<br>");
}

function writeSummary(decision) {
  const summaryPath = process.env.GITHUB_STEP_SUMMARY;
  if (!summaryPath) {
    return;
  }

  const buildState = decision.shouldBuild ? "执行构建" : "跳过构建";
  const summary = [
    "## 自动化构建判定",
    "",
    "| 字段 | 值 |",
    "| --- | --- |",
    `| 触发方式 | ${escapeMarkdown(decision.trigger)} |`,
    `| PR | ${escapeMarkdown(decision.prNumber)} |`,
    `| 目标提交 | ${escapeMarkdown(decision.buildSha || "-")} |`,
    `| 判定 | ${buildState} |`,
    `| 原因 | ${escapeMarkdown(decision.reason)} |`,
    "",
  ].join("\n");

  appendFileSync(summaryPath, summary, "utf8");
}

export function runFromEnvironment() {
  const eventPath = process.env.GITHUB_EVENT_PATH;
  if (!eventPath) {
    throw new Error("GITHUB_EVENT_PATH is required");
  }

  const payload = JSON.parse(readFileSync(eventPath, "utf8"));
  const decision = resolveBuildDecision({
    eventName: process.env.GITHUB_EVENT_NAME,
    sha: process.env.GITHUB_SHA,
    payload,
    skipLabel:
      process.env.SKIP_BUILD_LABEL?.trim() || DEFAULT_SKIP_BUILD_LABEL,
  });

  writeOutput("should_build", decision.shouldBuild);
  writeOutput("build_sha", decision.buildSha);
  writeOutput("short_sha", decision.shortSha);
  writeOutput("pr_number", decision.prNumber);
  writeOutput("trigger", decision.trigger);
  writeOutput("reason", decision.reason);
  writeSummary(decision);

  console.log(
    JSON.stringify(
      {
        shouldBuild: decision.shouldBuild,
        buildSha: decision.buildSha,
        prNumber: decision.prNumber,
        reason: decision.reason,
      },
      null,
      2,
    ),
  );

  if (decision.fatal) {
    process.exitCode = 1;
  }

  return decision;
}

const isMainModule =
  process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isMainModule) {
  try {
    runFromEnvironment();
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
