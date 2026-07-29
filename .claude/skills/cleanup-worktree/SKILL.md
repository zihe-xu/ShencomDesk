---
name: cleanup-worktree
description: >
  在某个 feature worktree 内运行，安全清理当前 worktree 及其已合并分支。先跑预览做安全校验（非主
  worktree、非受保护分支 main/master/develop、分支已合并入 main、工作区干净），把计划交给用户确认后，
  再用 --apply 移除该 worktree 并删除其本地分支。用于「清理当前 worktree」「删除已合并 worktree」
  「cleanup worktree」「worktree 用完了想删掉」等场景。脏工作区、未合并或受保护分支一律中止，不强制删除。
---

# 清理当前 Worktree

安全移除「当前所在」的 feature worktree，并删除其已合并入 `main` 的本地分支。配套脚本
`cleanup-worktree.sh`（与本 skill 同目录）。

## 何时使用

- 用户在某个 feature worktree 内说「清理当前 worktree」「这个 worktree 用完了，删掉」等。
- 该 feature 分支已经合并入 `main`，本地 worktree + 分支成了垃圾需要回收。

不适用：批量清理多个 worktree、清理远端分支（用 `glab`/`gitlab` skill）、在主 worktree 内运行。

## 安全规则（不可绕过）

- **never** 清理主 worktree。
- **never** 删除受保护分支：`main`、`master`、`develop`。
- **never** 删除未合并入 `main` 的分支（squash 合并会被判为未合并而拒绝 —— 这是安全的）。
- **never** 在工作区有未提交/未跟踪变更时清理（中止并报告，不 stash、不 `--force`）。
- **永远先预览、再确认、最后 --apply**；`--apply` 会重新校验，状态变化即中止。
- 只动本地：只删本地 worktree + 本地分支。

## 执行步骤

1. **预览**（从 feature worktree 内运行，脚本自动按当前 worktree 识别目标）：

   ```shell
   MAIN="$(git worktree list --porcelain | awk '/^worktree/ {print $2; exit}')"
   bash "$MAIN/.claude/skills/cleanup-worktree/cleanup-worktree.sh"
   ```

   把输出原样转述给用户。注意：脚本通过主 worktree 路径调用（feature 分支可能还没合入这个新脚本）。

2. 预览**非 0 退出** → 按脚本给的 `abort` 原因报告用户并停止，不要尝试绕过。

3. 预览成功 → 把「清理计划」给用户看，**用 harness 向用户确认**（不要通过脚本 stdin）。只有用户明确同意才继续。

4. 用户确认后，**原样执行**预览末尾打印的那条 `--apply` 命令（它自带 `cd "$MAIN"`，从主 worktree 运行）：

   ```shell
   cd "$MAIN" && bash "$MAIN/.claude/skills/cleanup-worktree/cleanup-worktree.sh" --apply --target "<worktree路径>"
   ```

5. 报告结果。提醒用户：当前 worktree 目录已被移除，后续命令需从主 worktree 执行（`cd "$MAIN"`）。

## 边界

- **只删本地**：远端分支清理交给 `glab`/`gitlab` skill。
- **squash 合并**：`git merge-base --is-ancestor` 检测不到 squash 合并，相关分支会被拒；确需删除时让用户手动 `git branch -D <branch>`。
- **误删恢复**：`git reflog` 找回分支 tip → `git branch <name> <sha>`。
