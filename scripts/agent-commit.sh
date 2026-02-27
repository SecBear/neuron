#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [[ $# -lt 1 ]]; then
  echo "usage: $0 \"<conventional commit title>\""
  echo "example: $0 \"feat(brain): add offline controller+worker loop\""
  exit 2
fi

title="$1"

./scripts/verify.sh

git status --porcelain=v1
if [[ -z "$(git status --porcelain=v1)" ]]; then
  echo "[agent-commit] nothing to commit"
  exit 0
fi

git add -A
git commit -m "${title}" -m "Verified via: ./scripts/verify.sh"

