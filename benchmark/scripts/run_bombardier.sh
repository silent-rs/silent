#!/usr/bin/env bash
set -euo pipefail

SCENARIO=${SCENARIO:-A}
PORT=${PORT:-8080}
DURATION=${DURATION:-30s}
CONCURRENCY=${CONCURRENCY:-256}
HOST=${HOST:-127.0.0.1}

case "$SCENARIO" in
  A)
    TARGET="http://$HOST:$PORT/"
    ;;
  B)
    # 示例路径与查询参数
    TARGET="http://$HOST:$PORT/b/abc/123/xyz?q1=a&q2=b&q3=42&q4=true&q5=z"
    ;;
  C)
    TARGET="http://$HOST:$PORT/static"
    ;;
  *)
    echo "Unknown SCENARIO: $SCENARIO" >&2
    exit 1
    ;;

esac

echo "==> Running server: SCENARIO=$SCENARIO PORT=$PORT (press Ctrl+C after test)"
# 提示：建议在另一个终端先启动服务：
#   SCENARIO=$SCENARIO PORT=$PORT cargo run -p benchmark --release

if [ "$SCENARIO" = "C" ]; then
  echo "==> Fetching ETag for If-None-Match"
  ETAG=$(curl -sI "$TARGET" | awk -F': ' '/^etag:/{print $2}' | tr -d '\r')
  if [ -n "$ETAG" ]; then
    echo "Using ETag: $ETAG"
    bombardier -c "$CONCURRENCY" -d "$DURATION" -H "If-None-Match: $ETAG" "$TARGET"
  else
    echo "No ETag returned, hitting 200 path"
    bombardier -c "$CONCURRENCY" -d "$DURATION" "$TARGET"
  fi
else
  bombardier -c "$CONCURRENCY" -d "$DURATION" "$TARGET"
fi
