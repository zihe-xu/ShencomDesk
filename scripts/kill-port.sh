#!/usr/bin/env bash
# 关闭占用指定端口的进程（macOS / Linux，依赖 lsof）
set -euo pipefail

usage() {
  cat <<'EOF'
用法:
  kill-port.sh <端口> [端口...]
  kill-port.sh --help

示例:
  ./scripts/kill-port.sh 3000
  ./scripts/kill-port.sh 3000 3001 3002
  pnpm run clean:ports -- 3010

说明:
  查找监听/占用该端口的进程并发送 SIGKILL 强制结束。
  若端口未被占用则跳过，不报错。
  仅供手动清理端口，dev 启动时不会自动调用。
EOF
}

is_valid_port() {
  local port="$1"
  [[ "$port" =~ ^[0-9]+$ ]] || return 1
  # 使用 10# 前缀，避免八进制歧义（如 08）
  (( 10#$port >= 1 && 10#$port <= 65535 ))
}

kill_port() {
  local port="$1"
  local pids

  if ! is_valid_port "$port"; then
    echo "[ERROR] 无效端口号: ${port}（须为 1-65535 的整数）" >&2
    return 1
  fi

  pids="$(lsof -ti ":$port" 2>/dev/null || true)"
  if [[ -z "$pids" ]]; then
    echo "[SKIP] 端口 $port 未被占用"
    return 0
  fi

  # shellcheck disable=SC2086
  kill -9 $pids 2>/dev/null || true
  echo "[OK] 端口 $port 已释放 (PID: $(echo "$pids" | tr '\n' ' '))"
}

main() {
  if [[ $# -eq 0 ]] || [[ "${1:-}" == "-h" ]] || [[ "${1:-}" == "--help" ]]; then
    usage
    [[ $# -eq 0 ]] && exit 1
    exit 0
  fi

  local port
  local failed=0
  for port in "$@"; do
    kill_port "$port" || failed=1
  done

  if [[ $failed -ne 0 ]]; then
    exit 1
  fi
  echo "Ports cleaned"
}

main "$@"
