# Brain SpecPack Traceability (v0) — Feature Map + Slices + Evidence

This spec defines additional **traceability artifacts** for Brain SpecPacks. The purpose is to
reduce “spec drift” and shallow/incorrect understanding by making the mapping between:

- **specs** (what we claim)
- **code/docs inputs** (what we observed)
- **traces/tests/golden artifacts** (how we verify)

…explicit and machine-actionable.

This spec borrows the mental model from program comprehension / feature location / slicing:
produce a durable “feature map” so downstream workers can operate in parallel without losing the
connection to evidence.

Brain is the backend: it does not perform static/dynamic analysis itself. It stores and validates
the artifacts produced by an external harness.

## Goals

Brain MUST support SpecPacks that:

1. Provide a **feature map** connecting ledger capabilities ↔ spec refs ↔ evidence/code refs.
2. Provide optional **slice artifacts** per capability to enable incremental, parallel work.
3. Make traceability **verifiable offline** (no network required at review time).

## Non-Goals

- Brain does not need to build a program analysis engine (CFG/callgraph/slicing) in this repo.
- Brain does not need to guarantee “perfect specs”; it enforces the artifact contract required to
  pursue that quality.

## Required SpecPack Additions

Under `specpack/` the following files MUST exist:

1. `analysis/feature_map.json` — Traceability index (machine-readable)

Brain MAY require additional analysis artifacts in future versions (call graphs, dependency
graphs, etc.). This spec keeps v0 minimal.

## `analysis/feature_map.json` (Traceability Index)

`specpack/analysis/feature_map.json` MUST be JSON with:

- `feature_map_version`: string (semver-like)
- `job_id`: string
- `produced_at`: RFC3339 UTC string
- `capabilities[]`:
  - `capability_id`: string (MUST match `ledger.json` `capabilities[].id`)
  - `spec_refs[]`: references into canonical SpecPack specs (path + anchor)
  - `code_refs[]`: list of observed/code references (can be empty)
    - `artifact_path`: string (job-relative, e.g. `sources/repo/src/lib.rs`)
    - `sha256` (optional): string (hex; if present, MUST match job artifact index)
    - `locator` (optional): object (opaque to Brain; e.g. line ranges, symbol ids)
    - `notes` (optional): string
  - `trace_refs[]` (optional): list of references to conformance artifacts (golden/tests/traces)
    - `path`: string (specpack-relative, under `conformance/`)
    - `kind`: `"golden" | "test" | "trace" | "matrix"`
  - `slice_refs[]` (optional): list of slice artifact paths (specpack-relative, under `analysis/`)

### Invariants

Brain MUST validate on `specpack_finalize`:

1. `analysis/feature_map.json` exists and is included in the SpecPack manifest.
2. Every `capabilities[].capability_id` exists in `ledger.json`.
3. Every `capabilities[].spec_refs[].path` exists in the SpecPack manifest.
4. Every `capabilities[].trace_refs[].path` exists in the SpecPack manifest.
5. Every `capabilities[].code_refs[].artifact_path` exists as a job artifact (in `index.json`).

Brain MUST NOT attempt to interpret `locator` semantics. It is a handle for downstream tools.

## Recommended Slice Artifacts (Optional)

If present, slice artifacts SHOULD live under `specpack/analysis/slices/` and be:

- `analysis/slices/{capability_id}.json`

These files can be used to store program slices / feature location results, e.g.:

- the minimal set of source files involved
- the key call paths
- dynamic trace identifiers

Brain may validate only existence + traversal-safety for v0.

## Tests

Brain MUST have offline tests proving:

1. `specpack_finalize` rejects SpecPacks missing `analysis/feature_map.json`.
2. `specpack_finalize` rejects feature maps referencing unknown ledger capability ids.
3. `specpack_finalize` rejects feature maps referencing missing specpack files or job artifacts.

