#!/usr/bin/env bash
# 把主 worktree 中被 .gitignore 过滤的本地配置 / 参考目录软链到当前 worktree。
# 用法：在新建的 worktree 根目录执行 setup-worktree.sh

set -uo pipefail

CURRENT="$(git rev-parse --show-toplevel)"
MAIN="$(git worktree list --porcelain | awk '/^worktree/ {print $2; exit}')"

if [ "$CURRENT" = "$MAIN" ]; then
  echo "当前已在主 worktree，无需执行本脚本。"
  exit 0
fi

# 待软链的文件
FILES=(
  ".env.local"
  ".env.apifox.local"
  ".envrc"
  ".claude/skills/mysql/connections.json"
)

# 待软链的目录
DIRS=(
  "temp"
  ".zcode/plans"
  ".plans"
)

linked=0
skipped=0

link_item() {
  local rel="$1"
  local src="$MAIN/$rel"
  local dst="$CURRENT/$rel"

  if [ ! -e "$src" ]; then
    echo "  warn  跳过 ${rel}：主 worktree 不存在"
    skipped=$((skipped + 1)) || true
    return 0
  fi

  if [ -e "$dst" ] && [ ! -L "$dst" ]; then
    echo "  warn  跳过 ${rel}：目标已存在且非软链，避免覆盖"
    skipped=$((skipped + 1)) || true
    return 0
  fi

  if [ -L "$dst" ] && [ "$(readlink "$dst")" = "$src" ]; then
    echo "  ok    ${rel}（已指向主 worktree）"
    return 0
  fi

  mkdir -p "$(dirname "$dst")"
  ln -sfn "$src" "$dst"
  echo "  ok    ${rel} -> ${src}"
  linked=$((linked + 1)) || true
  return 0
}

echo "主 worktree: $MAIN"
echo "当前 worktree: $CURRENT"
echo
echo "软链文件："
for f in "${FILES[@]}"; do link_item "$f"; done
echo "软链目录："
for d in "${DIRS[@]}"; do link_item "$d"; done

echo
echo "完成：新增软链 ${linked}，跳过 ${skipped}。"

echo
echo "codegraph 索引："
if [ -d "$CURRENT/.codegraph" ]; then
  echo "  run   .codegraph/ 已存在，执行 sync"
  if codegraph sync "$CURRENT"; then
    echo "  ok    codegraph sync 完成"
  else
    echo "  warn  codegraph sync 失败"
  fi
else
  echo "  run   .codegraph/ 不存在，执行 init -i 重建"
  if codegraph init -i "$CURRENT"; then
    echo "  ok    codegraph init -i 完成"
  else
    echo "  warn  codegraph init -i 失败"
  fi
fi
