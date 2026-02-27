# Verification And Nix

This repo assumes Rust tooling is provided by the Nix flake.

## Command Policy

Do not assume `cargo`/`rustc` exist on PATH.

Preferred:

1. `nix develop -c cargo test`
2. `nix develop -c cargo clippy -- -D warnings`
3. `nix develop -c nix fmt`

If a command fails, do not guess. Read the output, find root cause, and fix it with a test-first
approach when behavior is changing.

## No Claims Without Evidence

Do not claim:

1. "Tests pass"
2. "Fixed"
3. "Done"

Unless the relevant command was run in the current session and you have the exit status and the
failure count.

## Minimal Verification Sets

1. Rust code change:
   - `nix develop -c cargo test`
2. Public API / protocol change:
   - `nix develop -c cargo test`
   - plus any crate-specific tests touching the boundary
3. Formatting-only change:
   - `nix develop -c nix fmt`

