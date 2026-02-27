# fix_plan.md

This file is the single "what next" queue used by the Ralph loop (`PROMPT.md`).

Rules:

1. Keep it short.
2. Each item must link to the governing spec(s).
3. Each item must have a concrete "done when" and a verification command.

## Queue

1. Brain: add SpecPack outputs (spec library + manifest + factory queue)
   - Spec: `specs/16-brain-specpack-output-and-queue.md`
   - Done when:
     - Brain exposes `specpack_init`, `specpack_write_file`, `specpack_finalize`
     - Brain writes `specpack/manifest.json` and rejects drift (hash mismatch)
     - Brain validates `specpack/queue.json` references and path safety
     - Offline tests cover finalize success + manifest drift failure
   - Verify: `nix develop -c cargo test -p brain`

2. Brain: enforce SpecPack quality/backpressure (ledger + conformance bundle)
   - Spec: `specs/18-brain-specpack-quality-and-backpressure.md`
   - Done when:
     - `specpack_finalize` validates `ledger.json` and conformance bundle exists
     - `specpack_finalize` rejects invalid ledger spec refs and missing verify commands
     - Offline tests cover: missing ledger, bad refs, missing verify for impl task
   - Verify: `nix develop -c cargo test -p brain`

3. Brain: add artifact ingest + write tools for source-first workflows
   - Spec: `specs/17-brain-artifact-ingest-and-write.md`
   - Done when:
     - Brain exposes `artifact_import` and `artifact_write` (job-local, traversal-safe)
     - Artifact sha256 is over raw bytes on disk
     - Offline tests cover import/write and traversal rejection
   - Verify: `nix develop -c cargo test -p brain`

4. Brain: job groups (fan-out + merge) for large landscapes
   - Spec: `specs/15-brain-research-backend.md`
   - Done when:
     - Brain can start a group job and produce per-target bundles
     - Brain can merge bundles into a single “landscape” bundle + coverage gaps
     - Offline tests cover deterministic merge from fixtures
   - Verify: `nix develop -c cargo test -p brain`


## Completed

- 2026-02-27: Implemented `brain` v1 (controller + worker tools + MCP config)
  - Spec: `specs/14-brain-agentic-research-assistant.md`

- 2026-02-27: Implemented Brain v2 ResearchOps backend (MCP + async jobs + grounded bundles)
  - Spec: `specs/15-brain-research-backend.md`

- 2026-02-27: Hardened Brain v2 research backend (bundle contract + acquisition roles)
  - Spec: `specs/15-brain-research-backend.md`

- 2026-02-27: CI hard enforcement (format, tests, clippy) is present
  - Spec: `specs/13-documentation-and-dx-parity.md`
  - Workflow: `.github/workflows/ci.yml`

- 2026-02-27: Root README added (crate map + quickstart)
  - Spec: `specs/13-documentation-and-dx-parity.md`
  - File: `README.md`

- 2026-02-27: Umbrella `neuron` crate added (features + prelude)
  - Spec: `specs/12-packaging-versioning-and-umbrella-crate.md`
  - Crate: `neuron/`
