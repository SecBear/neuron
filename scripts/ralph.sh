#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

runner_display=""
runner=()

if [[ "${CODEX:-}" == "1" ]]; then
  runner=("codex")
  runner_display="codex"
else
  if command -v claude-code >/dev/null 2>&1; then
    runner=("claude-code")
    runner_display="claude-code"
  elif command -v claude >/dev/null 2>&1; then
    runner=("claude" "-p")
    runner_display="claude -p"

    if [[ -n "${CLAUDE_MODEL:-}" ]]; then
      runner+=("--model" "${CLAUDE_MODEL}")
      runner_display+=" --model ${CLAUDE_MODEL}"
    fi
  else
    echo "[ralph] neither claude-code nor claude found on PATH"
    echo "[ralph] install Claude Code, or run with: CODEX=1 ./scripts/ralph.sh"
    exit 1
  fi
fi

echo "[ralph] runner: ${runner_display}"
echo "[ralph] ctrl-c to stop"

while :; do
  cat PROMPT.md | "${runner[@]}"
done
