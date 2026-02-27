# fix_plan.md

This file is the single "what next" queue used by the Ralph loop (`PROMPT.md`).

Rules:

1. Keep it short.
2. Each item must link to the governing spec(s).
3. Each item must have a concrete "done when" and a verification command.

## Queue

1. Implement Brain v2 as a structured research backend (MCP + async jobs)
   - Spec: `specs/15-brain-research-backend.md`
   - Done when:
     - `brain` can run as an MCP server exposing:
       - `research_job_start`, `research_job_status`, `research_job_get`, `research_job_cancel`
       - `artifact_list`, `artifact_read`
     - Offline tests prove:
       - job writes `index.json` + `findings.md` + at least one `sources/*` artifact
       - artifact path traversal is rejected
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

## Completed

- 2026-02-27: Implemented `brain` v1 (controller + worker tools + MCP config)
  - Spec: `specs/14-brain-agentic-research-assistant.md`
