# fix_plan.md

This file is the single "what next" queue used by the Ralph loop (`PROMPT.md`).

Rules:

1. Keep it short.
2. Each item must link to the governing spec(s).
3. Each item must have a concrete "done when" and a verification command.

## Queue

1. Brain: SpecPack traceability (feature map + slices + evidence refs)
   - Spec: `specs/19-brain-specpack-traceability-and-feature-map.md`
   - Done when:
     - `specpack_finalize` requires `specpack/analysis/feature_map.json`
     - `specpack_finalize` validates capability ids (feature map ↔ ledger) and artifact refs exist
     - Offline tests cover: missing feature map, unknown capability id, missing artifact ref
   - Verify: `nix develop -c cargo test -p brain`

3. Brain: job groups (fan-out + merge) for large landscapes
   - Spec: `specs/15-brain-research-backend.md`
   - Done when:
     - Brain can start a group job and produce per-target bundles
     - Brain can merge bundles into a single “landscape” bundle + coverage gaps
     - Offline tests cover deterministic merge from fixtures
   - Verify: `nix develop -c cargo test -p brain`


## Completed

- 2026-02-27: Added artifact_import and artifact_write tools (source-first ingest)
  - Spec: `specs/17-brain-artifact-ingest-and-write.md`
  - Tools: `artifact_import`, `artifact_write` (traversal-safe, sha256-hashed)

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

- 2026-02-27: Brain SpecPack outputs added (specpack init/write/finalize)
  - Spec: `specs/16-brain-specpack-output-and-queue.md`
  - Tools: `specpack_init`, `specpack_write_file`, `specpack_finalize`

- 2026-02-27: Brain SpecPack finalize enforces quality/backpressure artifacts
  - Spec: `specs/18-brain-specpack-quality-and-backpressure.md`
  - Adds: `ledger.json` + `conformance/` validation, plus impl-task verify enforcement
