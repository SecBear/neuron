# fix_plan.md

This file is the single "what next" queue used by the Ralph loop (`PROMPT.md`).

Rules:

1. Keep it short.
2. Each item must link to the governing spec(s).
3. Each item must have a concrete "done when" and a verification command.

## Queue

1. Implement `brain` v1 (controller + worker tools + MCP config)
   - Spec: `specs/14-brain-agentic-research-assistant.md`
   - Done when:
     - A new `brain` crate exists with an offline integration test proving:
       - controller selects a worker tool
       - worker uses mock provider and returns structured JSON
       - controller synthesizes into final answer
     - `brain` loads a Claude/Codex-compatible `.mcp.json` (with optional `x-brain`) and assembles tools
   - Verify: `nix develop -c cargo test`

2. Add CI (hard enforcement) for formatting, tests, and clippy
   - Specs: `specs/13-documentation-and-dx-parity.md`
   - Done when:
     - GitHub Actions runs `pre-commit` (treefmt), `cargo test`, `cargo clippy` in the Nix dev shell
   - Verify: `nix develop -c cargo test`

3. Add root README + crate map + quickstart
   - Spec: `specs/13-documentation-and-dx-parity.md`
   - Done when:
     - A new `README.md` exists with a minimal quickstart for local composition
     - Includes a crate map and points to `SPECS.md` for requirements
   - Verify: `nix develop -c cargo test`

4. Add umbrella `neuron` crate with feature flags + prelude
   - Spec: `specs/12-packaging-versioning-and-umbrella-crate.md`
   - Done when:
     - `neuron/` crate exists and re-exports the happy-path set behind features
   - Verify: `nix develop -c cargo test`

