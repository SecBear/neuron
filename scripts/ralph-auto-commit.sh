#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [[ -z "$(git status --porcelain=v1)" ]]; then
  exit 0
fi

echo "[ralph] auto-commit: formatting (nix fmt)"
nix fmt >/dev/null

echo "[ralph] auto-commit: staging"
git add -A

if ! git diff --exit-code >/dev/null; then
  echo "[ralph] auto-commit: formatting changed files; re-run and commit the changes"
  git diff --stat
  exit 1
fi

echo "[ralph] auto-commit: verify"
./scripts/verify.sh

if [[ -z "$(git status --porcelain=v1)" ]]; then
  echo "[ralph] auto-commit: nothing to commit"
  exit 0
fi

title="${RALPH_COMMIT_TITLE:-chore(ralph): auto-commit $(date -u +%Y-%m-%dT%H:%M:%SZ)}"
git commit -m "${title}" -m "Verified via: ./scripts/verify.sh"

