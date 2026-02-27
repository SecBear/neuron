#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if ! command -v claude-code >/dev/null 2>&1; then
  echo "[ralph] claude-code not found on PATH"
  echo "[ralph] install it or run with: CODEX=1 ./scripts/ralph.sh"
fi

if [[ "${CODEX:-}" == "1" ]]; then
  runner="codex"
else
  runner="claude-code"
fi

echo "[ralph] runner: ${runner}"
echo "[ralph] ctrl-c to stop"

while :; do
  cat PROMPT.md | ${runner}
done

