#!/usr/bin/env bash
# 获取测试环境 access token，并缓存到 node_modules/temp/token.json。
# 用法：get-token.sh [--print|--force|--check]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
TOKEN_FILE="$PROJECT_DIR/node_modules/temp/token.json"
EXPIRY_MARGIN=60

MODE="default"
case "${1:-}" in
  '') ;;
  --print) MODE="print" ;;
  --force) MODE="force" ;;
  --check) MODE="check" ;;
  *) echo "用法：$0 [--print|--force|--check]" >&2; exit 2 ;;
esac
[ "$#" -le 1 ] || { echo "用法：$0 [--print|--force|--check]" >&2; exit 2; }

if [ -f "$PROJECT_DIR/.env.local" ]; then
  set -a
  # shellcheck disable=SC1091
  source "$PROJECT_DIR/.env.local"
  set +a
fi

: "${TEST_LOGIN_URL:?请在 .env.local 配置 TEST_LOGIN_URL}"
: "${SCID:?请在 .env.local 配置 SCID}"
: "${TEST_USERNAME:?请在 .env.local 配置 TEST_USERNAME}"
: "${TEST_PASSWORD:?请在 .env.local 配置 TEST_PASSWORD}"

LOGIN_URL="${TEST_LOGIN_URL:-$TEST_LOGIN_URL}"
SCID="${TEST_SCID:-$SCID}"
USERNAME="${TEST_USERNAME:-$DEFAULT_USERNAME}"
PASSWORD="${TEST_PASSWORD:-$DEFAULT_PASSWORD}"

read_cache() {
  python3 - "$TOKEN_FILE" "$EXPIRY_MARGIN" "$1" <<'PY'
import json, sys, time

path, margin, mode = sys.argv[1], int(sys.argv[2]), sys.argv[3]
try:
    with open(path) as file:
        data = json.load(file)
    remaining = data["expiration"] / 1000 - time.time()
    if remaining <= margin:
        raise ValueError("token 已过期")
    if mode == "print":
        print(data["token"])
    elif mode == "check":
        print(f"token 有效，剩余 {int(remaining)}s，过期于 {data['expirationISO']}")
except Exception as error:
    if mode == "check":
        print(f"token 无效：{error}", file=sys.stderr)
    sys.exit(1)
PY
}

if [ "$MODE" = "check" ]; then
  read_cache check
  exit $?
fi

if [ "$MODE" != "force" ] && read_cache default 2>/dev/null; then
  if [ "$MODE" = "print" ]; then
    read_cache print
  else
    echo "命中缓存 token：$TOKEN_FILE" >&2
  fi
  exit 0
fi

LOGIN_PAYLOAD=$(USERNAME="$USERNAME" PASSWORD="$PASSWORD" python3 -c \
  'import json, os; print(json.dumps({"username": os.environ["USERNAME"], "password": os.environ["PASSWORD"]}))')

echo "登录 URL：$LOGIN_URL" >&2
echo "请求参数：scid=$SCID, body={\"username\":\"$USERNAME\",\"password\":\"******\"}" >&2

RESPONSE=$(curl -fsS --location --request POST "$LOGIN_URL" \
  --header "scid: $SCID" \
  --header 'Content-Type: application/json' \
  --data-raw "$LOGIN_PAYLOAD") || {
    echo "登录请求失败" >&2
    exit 1
  }

LOGIN_RESPONSE="$RESPONSE" python3 - "$TOKEN_FILE" "$USERNAME" "$MODE" <<'PY'
import json, os, sys
from datetime import datetime, timezone

token_file, username, mode = sys.argv[1:]
try:
    envelope = json.loads(os.environ["LOGIN_RESPONSE"])
    if envelope.get("errcode") != "0000":
        raise ValueError(f'{envelope.get("errcode")}: {envelope.get("errmsg")}')
    info = envelope["data"]["additionalInformation"]
    token, expiration = info["value"], info["expiration"]
except Exception as error:
    print(f"登录响应无效：{error}", file=sys.stderr)
    sys.exit(1)

result = {
    "token": token,
    "expiration": expiration,
    "expirationISO": datetime.fromtimestamp(expiration / 1000, tz=timezone.utc).isoformat(),
    "expiresIn": info.get("expiresIn"),
    "tokenType": info.get("tokenType"),
    "refreshToken": (info.get("refreshToken") or {}).get("value"),
    "refreshTokenExpiration": (info.get("refreshToken") or {}).get("expiration"),
    "username": username,
    "fetchedAt": datetime.now(timezone.utc).isoformat(),
}

if mode == "print":
    print(token)
else:
    os.makedirs(os.path.dirname(token_file), exist_ok=True)
    fd = os.open(token_file, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as file:
        json.dump(result, file, ensure_ascii=False, indent=2)
        file.write("\n")
    print(f"登录成功，token 已写入：{token_file}", file=sys.stderr)
    print(f"账号：{username}，过期：{result['expirationISO']}", file=sys.stderr)
PY
