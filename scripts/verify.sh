#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "[verify] formatting (treefmt via nix fmt)"
# `nix fmt` modifies files in-place. Verification should ensure formatting has
# converged (idempotent), not that the git tree is clean.
nix fmt >/dev/null
diff_after_first="$(git diff)"
nix fmt >/dev/null
diff_after_second="$(git diff)"
if [[ "${diff_after_first}" != "${diff_after_second}" ]]; then
  echo "[verify] formatting is not stable; re-run nix fmt and commit the changes"
  git diff --stat
  exit 1
fi

echo "[verify] tests"
nix develop -c cargo test --workspace --all-targets

echo "[verify] clippy"
nix develop -c cargo clippy --workspace --all-targets -- -D warnings

echo "[verify] ok"
