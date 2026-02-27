# Brain SpecPack (v0) — File-Based Spec Library + Manifest + Factory Queue

This spec extends Brain v2 (the ResearchOps backend) with a **SpecPack** output format that is
optimized for a downstream “factory” to execute parallel Ralph loops with minimal conflicts.

Key design constraint: **one source of truth**.

- The canonical content is a **file-based spec library** (`specpack/specs/*.md`).
- A machine-readable manifest exists only as an **index** (hash-locked references), not a second
  editable copy of the spec content.

This enables:

- progressive loading (factory loads only the files required for a task)
- deterministic scheduling (factory uses `queue.json` for parallelization)
- drift prevention (manifest hashes must match the spec files)

## Goals

Brain MUST be able to produce a SpecPack that is “factory-ready”:

1. Canonical, human-readable spec library (domain-split files).
2. A hash-locked manifest for deterministic consumption.
3. A machine-readable parallel work queue for decomposition and scheduling.

## Non-Goals

- Brain does not need to implement a full “factory” in this repo.
- Brain does not need to perform model-driven synthesis internally.
  - The recommended architecture is: **external harness is the controller**; Brain is the backend
    that stores and validates the SpecPack artifacts.

## Definitions

- **SpecPack**: a job-local artifact subtree that contains:
  - a spec library (canonical)
  - a manifest (index)
  - a queue (tasks)
- **Factory**: an external orchestrator that consumes the SpecPack and drives parallel Ralph loops.
- **Task**: a unit of work that can be executed by a worker loop with explicit backpressure.

## SpecPack Contract (Filesystem Layout)

When present, a SpecPack MUST live under the job directory:

`{artifact_root}/{job_id}/specpack/`

Required files:

1. `specpack/SPECS.md` (spec index for the SpecPack)
2. `specpack/specs/` (canonical spec library)
3. `specpack/manifest.json` (hash-locked index of the SpecPack)
4. `specpack/queue.json` (parallelizable factory queue)

Recommended structure:

- `specpack/specs/00-overview.md`
- `specpack/specs/01-architecture.md`
- `specpack/specs/02-data-model.md`
- `specpack/specs/03-apis.md`
- `specpack/specs/04-cli.md`
- `specpack/specs/05-edge-cases.md`
- `specpack/specs/06-testing-and-backpressure.md`

Brain MUST NOT require these exact filenames; they are recommendations only.

## Canonical Truth

The canonical source of truth is the Markdown spec files under `specpack/specs/`.

The following MUST hold:

1. `manifest.json` MUST NOT duplicate spec content; it is references + hashes only.
2. `queue.json` MUST reference the canonical spec files by path + optional anchor.

## `manifest.json` (Index-Only, Hash-Locked)

`manifest.json` MUST include:

- `specpack_version`: string (semver-like, e.g. `"0.1"`)
- `brain_version`: string
- `job_id`: string
- `produced_at`: RFC3339 UTC string
- `files[]`:
  - `path`: string (specpack-relative, e.g. `specs/01-architecture.md`)
  - `sha256`: string (hex of raw bytes)
  - `media_type`: string
- `entrypoints[]`: list of `path` strings (recommended “start here” files)
- `roots`:
  - `specs_dir`: `"specs/"`
  - `queue_path`: `"queue.json"`
  - `index_path`: `"SPECS.md"`

Brain MUST validate on finalize/read:

- every `files[].path` is relative and traversal-safe
- every listed file exists
- every listed file hash matches the on-disk raw bytes
- `entrypoints[]` are present in `files[]`

## `queue.json` (Factory Task Graph)

`queue.json` MUST be machine-readable and optimized for parallelization.

Minimum required shape:

- `queue_version`: string (semver-like)
- `job_id`: string
- `created_at`: RFC3339 UTC string
- `tasks[]`:
  - `id`: string
  - `title`: string
  - `kind`: `"spec" | "impl" | "test" | "docs" | "research"`
  - `spec_refs[]`:
    - `path`: string (specpack-relative markdown file path)
    - `anchor`: string|null (markdown heading anchor or explicit id)
  - `depends_on[]`: list of task ids
  - `backpressure`:
    - `verify`: list of shell commands (strings) that define “done when green”
  - `file_ownership`:
    - `allow_globs[]`: list of globs this task may modify
    - `deny_globs[]`: list of globs this task must not modify
  - `concurrency`:
    - `group`: string|null (tasks in same group should not run concurrently)

Brain SHOULD include additional optional metadata:

- `priority`: integer
- `estimates`: tokens/time/cost
- `risk`: `"low" | "medium" | "high"`

Factory expectations:

- Tasks with disjoint `file_ownership.allow_globs` MAY be parallelized.
- Tasks sharing a `concurrency.group` MUST NOT run concurrently.
- Tasks MUST be executed only when dependencies are completed.

## Progressive Loading Policy (Factory Consumption)

To minimize context use, a factory SHOULD:

1. Load `manifest.json` + `queue.json` first.
2. For a given task, load only:
   - the referenced `spec_refs[]` files
   - any transitive “entrypoint” or dependency spec files required for coherence
3. Avoid loading raw research artifacts unless needed for dispute resolution.

## Tool Surface (Brain v2 Extension)

Brain v2 SHOULD expose tools to support external-harness-driven SpecPack creation:

1. `specpack_init`
   - Input: `{ "job_id": "...", "specpack_version": "0.1" }`
   - Output: `{ "job_id": "...", "specpack_root": "specpack/" }`
2. `specpack_write_file`
   - Input: `{ "job_id": "...", "path": "specpack/specs/01-architecture.md", "encoding": "utf-8" | "base64", "content": "...", "media_type": "text/markdown" }`
   - Output: `{ "path": "...", "sha256": "..." }`
3. `specpack_finalize`
   - Input: `{ "job_id": "...", "entrypoints": ["specpack/specs/00-overview.md"], "queue_path": "specpack/queue.json" }`
   - Output: `{ "manifest_path": "specpack/manifest.json" }`

Brain MUST validate the SpecPack per this spec before returning success from `specpack_finalize`.

## Tests

Brain MUST have offline tests proving:

1. A SpecPack can be created file-by-file via tools and finalized.
2. `manifest.json` rejects drift (hash mismatch).
3. `queue.json` tasks reference valid spec paths and reject traversal.
4. Factory-relevant parallelization metadata exists (at least one task with file ownership + verify commands).

