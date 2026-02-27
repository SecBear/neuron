#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

runner_display=""
runner=()
pretty=()

if [[ "${CODEX:-}" == "1" ]]; then
  runner=("codex" "exec")
  runner_display="codex exec"

  if [[ -n "${CODEX_MODEL:-}" ]]; then
    runner+=("--model" "${CODEX_MODEL}")
    runner_display+=" --model ${CODEX_MODEL}"
  fi
else
  if command -v claude-code >/dev/null 2>&1; then
    runner=("claude-code")
    runner_display="claude-code"
  elif command -v claude >/dev/null 2>&1; then
    runner=(
      "claude"
      "-p"
      "--disable-slash-commands"
      "--permission-mode"
      "bypassPermissions"
      "--tools"
      "default"
    )
    runner_display="claude -p (bypassPermissions, tools=default)"

    if [[ -n "${CLAUDE_MODEL:-}" ]]; then
      runner+=("--model" "${CLAUDE_MODEL}")
      runner_display+=" --model ${CLAUDE_MODEL}"
    fi

    if [[ "${RALPH_PRETTY:-}" == "1" ]]; then
      runner+=("--output-format" "stream-json")
      pretty=("python3" "scripts/claude_stream_pretty.py")
      runner_display+=" | pretty"
    fi
  else
    echo "[ralph] neither claude-code nor claude found on PATH"
    echo "[ralph] install Claude Code, or run with: CODEX=1 ./scripts/ralph-once.sh"
    exit 1
  fi
fi

echo "[ralph] runner: ${runner_display}"

if [[ "${RALPH_STOP_ON_EMPTY:-1}" == "1" ]] && [[ -f fix_plan.md ]]; then
  if grep -qE '^[[:space:]]*1[[:space:]]*\\.[[:space:]]*\\(empty\\)[[:space:]]*$' fix_plan.md; then
    echo "[ralph] fix_plan queue is empty; exiting"
    exit 0
  fi
fi

if [[ "${runner[0]}" == "codex" ]]; then
  prompt="$(cat PROMPT.md)"
  "${runner[@]}" "${prompt}"
elif [[ "${runner[0]}" == "claude" ]]; then
  prompt="$(cat PROMPT.md)"
  if [[ "${#pretty[@]}" -gt 0 ]]; then
    "${runner[@]}" "${prompt}" | "${pretty[@]}"
  else
    "${runner[@]}" "${prompt}"
  fi
else
  cat PROMPT.md | "${runner[@]}"
fi
