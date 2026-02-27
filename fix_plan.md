# fix_plan.md

This file is the single "what next" queue used by the Ralph loop (`PROMPT.md`).

Rules:

1. Keep it short.
2. Each item must link to the governing spec(s).
3. Each item must have a concrete "done when" and a verification command.

## Queue


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
