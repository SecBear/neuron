#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

require_worktree() {
  if [[ "${RALPH_REQUIRE_WORKTREE:-1}" != "1" ]]; then
    return 0
  fi

  if [[ ! -e .git ]]; then
    echo "[ralph] error: .git not found (not a git checkout?)"
    exit 2
  fi

  if [[ -d .git ]] && [[ "${RALPH_ALLOW_MAIN:-0}" != "1" ]]; then
    echo "[ralph] error: refusing to run in main worktree ('.git' is a directory)"
    echo "[ralph] create a worktree first, then run ralph inside it"
    echo "[ralph] example: ./scripts/new-worktree.sh brain-v1 redesign/v2"
    echo "[ralph] override: RALPH_ALLOW_MAIN=1 ./scripts/ralph-once.sh"
    exit 2
  fi
}

require_worktree

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

if [[ "${RALPH_STOP_ON_EMPTY:-1}" == "1" ]] && [[ -f ralph_queue.md ]]; then
  # Consider the queue empty if there are no numbered items under "## Queue",
  # or if the only numbered items are explicit "(empty)" markers.
  if grep -qE '^##[[:space:]]+Queue[[:space:]]*$' ralph_queue.md; then
    queue_lines="$(
      awk '
        BEGIN { in=0 }
        /^##[[:space:]]+Queue[[:space:]]*$/ { in=1; next }
        in && /^##[[:space:]]+/ { exit }
        in { print }
      ' ralph_queue.md | tr -d '\r'
    )"
    numbered="$(printf '%s\n' "${queue_lines}" | grep -E '^[[:space:]]*[0-9]+[[:space:]]*\\.' || true)"
    if [[ -z "${numbered}" ]]; then
      echo "[ralph] ralph_queue is empty; exiting"
      exit 0
    fi
    if ! printf '%s\n' "${numbered}" | grep -qEv '^[[:space:]]*[0-9]+[[:space:]]*\\.[[:space:]]*\\(empty\\)[[:space:]]*$'; then
      echo "[ralph] ralph_queue is empty; exiting"
      exit 0
    fi
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

if [[ "${RALPH_AUTO_COMMIT:-1}" == "1" ]]; then
  ./scripts/ralph-auto-commit.sh
fi
