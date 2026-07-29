#!/usr/bin/env bash
# 安全清理当前 feature worktree 及其已合并分支（只动本地）。
#
# 用法（脚本位于 .claude/skills/cleanup-worktree/，通过主 worktree 路径调用以兼容旧 feature 分支）：
#   bash "<MAIN>/.claude/skills/cleanup-worktree/cleanup-worktree.sh"                                    # 预览：校验 + 打印计划，不删除
#   bash "<MAIN>/.claude/skills/cleanup-worktree/cleanup-worktree.sh" --apply --target <worktree路径>    # 执行删除（会重新校验）
#
# 预览模式按「当前所在 worktree」识别目标，需在某个 feature worktree 内运行。
# 删除前强制校验：非主 worktree、非受保护分支、分支已合并入 main、工作区干净。

set -uo pipefail

# 脚本自身绝对路径，用于打印 --apply 命令（与脚本所在位置无关）
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"

PROTECTED_BRANCHES="main master develop"

# ---- helpers ----

resolve_main() {
  git worktree list --porcelain | awk '/^worktree/ {print $2; exit}'
}

branch_of_worktree() {
  # $1 = worktree 路径；输出分支名，detached HEAD 时输出空串
  git -C "$1" branch --show-current 2>/dev/null
}

is_protected() {
  local b="$1"
  for p in $PROTECTED_BRANCHES; do
    [ "$b" = "$p" ] && return 0
  done
  return 1
}

is_registered_worktree() {
  git worktree list --porcelain | awk -v t="$1" '/^worktree/ {if ($2==t) found=1} END{exit !found}'
}

# 校验单个目标 worktree；通过则用全局变量 VALID_TARGET/VALID_BRANCH/VALID_MAIN 返回。
# 失败时把原因打到 stderr 并返回非 0。
validate_target() {
  local target="$1"
  local main="$2"

  if ! is_registered_worktree "$target"; then
    echo "  abort  目标不是已注册的 worktree：$target" >&2
    return 1
  fi
  if [ "$target" = "$main" ]; then
    echo "  abort  目标是主 worktree，拒绝清理：$target" >&2
    return 1
  fi

  local branch
  branch="$(branch_of_worktree "$target")"
  if [ -z "$branch" ]; then
    echo "  abort  目标 worktree 处于 detached HEAD，无分支可处理：$target" >&2
    return 1
  fi
  if is_protected "$branch"; then
    echo "  abort  目标分支受保护，拒绝删除：$branch" >&2
    return 1
  fi
  local base_branch
  base_branch="$(git -C "$main" branch --show-current 2>/dev/null)"
  if [ -z "$base_branch" ]; then
    echo "  abort  无法解析主 worktree 的集成分支（主 worktree 可能处于 detached HEAD）" >&2
    return 1
  fi
  if ! git merge-base --is-ancestor "$branch" "$base_branch" 2>/dev/null; then
    echo "  abort  分支「${branch}」未合并入 ${base_branch}，拒绝删除（squash 合并也会被判为未合并；如确需删除请手动 git branch -D）" >&2
    return 1
  fi

  local dirty
  dirty="$(git -C "$target" status --porcelain 2>/dev/null)"
  if [ -n "$dirty" ]; then
    echo "  abort  目标 worktree 有未提交/未跟踪变更，拒绝清理：" >&2
    printf '%s\n' "$dirty" | sed 's/^/        /' >&2
    return 1
  fi

  VALID_TARGET="$target"
  VALID_BRANCH="$branch"
  VALID_MAIN="$main"
  return 0
}

# ---- 预览 ----
do_preview() {
  local current main
  current="$(git rev-parse --show-toplevel 2>/dev/null)" || {
    echo "  abort  当前不在 git 仓库内" >&2
    exit 1
  }
  main="$(resolve_main)"

  echo "主 worktree:   $main"
  echo "当前 worktree:  $current"
  echo

  if [ "$current" = "$main" ]; then
    echo "  abort  当前在主 worktree，不能清理主 worktree。请 cd 到某个 feature worktree 后重试。" >&2
    exit 1
  fi

  if ! validate_target "$current" "$main"; then
    exit 1
  fi

  echo "  ok    分支「${VALID_BRANCH}」已合并入 main，工作区干净"
  echo
  echo "清理计划："
  echo "  - 移除 worktree：$VALID_TARGET"
  echo "  - 删除本地分支：$VALID_BRANCH"
  echo "  - main worktree：$VALID_MAIN"
  echo
  echo "确认后执行（从主 worktree 运行）："
  echo "  cd \"$VALID_MAIN\" && bash \"$SELF\" --apply --target \"$VALID_TARGET\""
}

# ---- 执行 ----
do_apply() {
  local target=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --target) target="${2-}"; shift 2 ;;
      *) echo "  abort  未知参数：$1" >&2; exit 2 ;;
    esac
  done

  if [ -z "$target" ]; then
    echo "  abort  缺少 --target <worktree路径>" >&2
    exit 2
  fi

  local main
  main="$(resolve_main)"
  # 切到主 worktree 再操作，避免删掉自身所在目录导致后续命令失效
  cd "$main" || { echo "  abort  无法进入主 worktree：$main" >&2; exit 1; }

  echo "重新校验目标（预览后状态可能已变化）…"
  if ! validate_target "$target" "$main"; then
    exit 1
  fi
  echo "  ok    校验通过：${VALID_TARGET}（分支 ${VALID_BRANCH}）"
  echo

  echo "移除 worktree…"
  if git worktree remove "$VALID_TARGET"; then
    echo "  ok    worktree 已移除：$VALID_TARGET"
  else
    echo "  abort  git worktree remove 失败，未删除分支，请手动检查" >&2
    exit 1
  fi

  echo "删除本地分支…"
  if git branch -d "$VALID_BRANCH"; then
    echo "  ok    分支已删除：$VALID_BRANCH"
  else
    echo "  warn  git branch -d 失败（worktree 已移除、分支保留），请手动处理：$VALID_BRANCH" >&2
    exit 1
  fi

  echo
  echo "完成。后续命令请从主 worktree 执行：cd \"$VALID_MAIN\""
}

# ---- 入口 ----
if [ "$#" -eq 0 ]; then
  do_preview
else
  case "$1" in
    --apply) shift; do_apply "$@" ;;
    *) echo "  abort  未知命令：$1（仅支持 --apply）" >&2; exit 2 ;;
  esac
fi
