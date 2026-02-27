#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "[verify] formatting (treefmt via nix fmt)"
nix fmt >/dev/null
if ! git diff --exit-code >/dev/null; then
  echo "[verify] formatting changed files; re-run and commit the changes"
  git diff --stat
  exit 1
fi

echo "[verify] tests"
nix develop -c cargo test

echo "[verify] clippy"
nix develop -c cargo clippy -- -D warnings

echo "[verify] ok"

