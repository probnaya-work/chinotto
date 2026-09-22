#!/usr/bin/env bash
# Default development/GitHub-release entry point. Store builds intentionally bypass this
# wrapper and select the mutually exclusive `mas` feature/config themselves.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMMAND="${1:-}"
if [ -z "$COMMAND" ]; then
  exec npx tauri
fi
shift

case "$COMMAND" in
  dev|build|bundle)
    exec npx tauri "$COMMAND" "$@" \
      --features direct-distribution \
      --config "$ROOT/src-tauri/tauri.direct-build.json"
    ;;
  *) exec npx tauri "$COMMAND" "$@" ;;
esac
