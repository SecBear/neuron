# Brain SpecPack Quality (v0) — Parity Ledger + Conformance Backpressure

This spec defines the minimum **quality/backpressure** artifacts required for a SpecPack to be
useful for building high-quality software via automated worker loops.

The core idea: **a “perfect spec” is enforced by executable backpressure**, not prose alone.

SpecPacks MUST therefore include:

1. a **parity ledger** (coverage + gaps + status)
2. a **conformance/backpressure bundle** (tests, golden artifacts, and verify commands)

This spec intentionally does not mandate a specific language/framework for tests. It defines the
artifact contract and validation rules.

## Goals

Brain MUST support SpecPacks that:

1. Are decomposable into parallel tasks with explicit verification commands.
2. Make coverage/gaps visible and machine-actionable.
3. Treat tests and traces as first-class specification artifacts.

## Non-Goals

- Brain does not need to execute the factory tasks itself.
- Brain does not need to ship a full compatibility lab.

## Key Principles (Reverse Engineering Spec Quality)

1. **Normative requirements must be testable.**
2. **The test suite is part of the specification.**
3. **Observed behavior must be separated from designed behavior.**
4. **Coverage must be tracked explicitly (ledger), not inferred.**

## Required SpecPack Additions

Under `specpack/` the following files MUST exist:

1. `ledger.json` — Parity ledger (machine-readable)
2. `conformance/` — Conformance/backpressure bundle
3. `specs/05-edge-cases.md` (or equivalent) — explicit edge-case matrix
4. `specs/06-testing-and-backpressure.md` (or equivalent) — backpressure philosophy + how to run

Brain MUST treat the filenames above as defaults; alternate paths are allowed if the manifest
declares them (see below).

## `ledger.json` (Parity Ledger)

`ledger.json` MUST include:

- `ledger_version`: string (semver-like)
- `job_id`: string
- `created_at`: RFC3339 UTC string
- `targets[]`: list of target systems/components (can be empty for “greenfield”)
- `capabilities[]`:
  - `id`: string (stable)
  - `domain`: string (e.g. `cli`, `api`, `state`, `auth`, `providers`)
  - `title`: string
  - `status`: `"unknown" | "specified" | "implemented" | "verified"`
  - `priority`: integer
  - `spec_refs[]`: references into canonical spec files (path + anchor)
  - `evidence[]`: optional references to job-local research artifacts/traces
- `gaps[]`: list of known missing areas, each with:
  - `id`: string
  - `description`: string
  - `next_probe`: string (what to test/fetch next)

Ledger invariants:

1. `capabilities[].id` MUST be unique.
2. `spec_refs[]` paths MUST exist in the SpecPack manifest.
3. `status == "verified"` MUST require at least one conformance reference (see below).

## Conformance / Backpressure Bundle

`specpack/conformance/` MUST contain:

- `README.md` — how to run conformance checks
- `verify` — recommended single entrypoint (script or instructions)

Brain SHOULD encourage (but not require) the following structure:

- `conformance/golden/` — golden outputs / fixtures
- `conformance/tests/` — test code
- `conformance/traces/` — captured traces from observed behavior
- `conformance/matrices/` — compatibility matrices across versions/platforms

### Conformance References

The work queue (`queue.json`) and ledger (`ledger.json`) SHOULD reference conformance artifacts:

- `capabilities[].conformance_refs[]`:
  - `path`: string (specpack-relative)
  - `kind`: `"golden" | "test" | "trace" | "matrix"`

## Manifest Extensions

`specpack/manifest.json` MUST declare:

- `quality`:
  - `ledger_path`: string (default `ledger.json`)
  - `conformance_root`: string (default `conformance/`)
  - `required_spec_files[]`: list of canonical spec files required for completeness

Brain MUST validate these exist and hashes match.

## Queue Integration

Every `queue.json` task that has `kind == "impl"` MUST include at least one `backpressure.verify`
command.

Brain SHOULD prefer verify commands that directly exercise conformance artifacts (not just `cargo
test`).

## Tests

Brain MUST have offline tests proving:

1. `specpack_finalize` rejects SpecPacks missing `ledger.json`.
2. `specpack_finalize` rejects SpecPacks whose ledger references missing spec files.
3. `specpack_finalize` rejects `queue.json` tasks missing verify commands for `impl` tasks.

