#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [[ ! -e .git ]]; then
  echo "[ralph] error: .git not found (not a git checkout?)"
  exit 2
fi

# In the main worktree, `.git` is a directory. In linked worktrees, `.git` is a file.
if [[ -d .git ]]; then
  topic="${1:-${RALPH_TOPIC:-}}"
  if [[ -z "${topic}" ]]; then
    topic="ralph-$(date -u +%Y%m%d-%H%M%S)"
    echo "[ralph] main worktree detected; using generated topic: ${topic}"
    echo "[ralph] tip: pass a stable topic slug to reuse a worktree (e.g. brain-v1)"
  fi

  base="${2:-${RALPH_BASE:-$(git rev-parse --abbrev-ref HEAD)}}"

  root="$(git rev-parse --show-toplevel)"
  parent="$(cd "$root/.." && pwd)"
  path="${parent}/neuron-explore-${topic}"

  if [[ ! -d "${path}" ]]; then
    ./scripts/new-worktree.sh "${topic}" "${base}"
  fi

  if [[ ! -e "${path}/.git" ]]; then
    echo "[ralph] error: expected worktree at ${path}, but it is not a git checkout"
    exit 2
  fi

  cd "${path}"
fi

exec ./scripts/ralph.sh

