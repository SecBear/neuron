# Brain Artifacts (v0) — Ingest + Write API for Source-First Workflows

This spec defines a Brain v2 extension that enables **source-first** research/spec generation by
allowing an external harness to:

1. import source snapshots (code/docs/notes) as artifacts
2. write derived artifacts (spec files, tables, queues) progressively

This avoids requiring Brain to have its own model credentials or direct access to arbitrary
filesystems/network, while still enabling large-repo and large-doc workflows.

## Goals

Brain MUST:

1. Allow an external harness to ingest content into a job as artifacts.
2. Allow an external harness to write/update derived artifacts under a job directory.
3. Enforce traversal-safe, job-relative paths for all writes/reads.
4. Hash artifacts by raw bytes and record minimal provenance metadata.

## Non-Goals

- Brain is not a general-purpose file server for the whole repo.
- Brain does not need to support arbitrary large blobs without bounds; it MAY enforce size limits.

## Canonical Policy (Source Priority)

Brain SHOULD support workflows that prioritize:

1. source code snapshots (repo trees, vendored deps, configs)
2. official documentation snapshots
3. other open available information

Brain does not decide “what to ingest”; it provides the primitives.

## Artifact Write Contract

All write-like tools MUST:

1. Reject absolute paths and any `..` traversal.
2. Only allow writing under `{artifact_root}/{job_id}/`.
3. Create parent directories as needed.
4. Compute and return `sha256` over the raw bytes written.

## Tool Surface (Brain v2 Extension)

Brain v2 SHOULD expose:

### `artifact_import`

Import a new artifact into a job directory.

- Input:
  - `job_id`: string
  - `path`: string (job-relative, e.g. `sources/repo/src/main.rs`)
  - `encoding`: `"utf-8" | "base64"`
  - `content`: string
  - `media_type`: string
  - `provenance` (optional):
    - `source_url`: string|null
    - `retrieved_at`: RFC3339 UTC string|null
- Output:
  - `path`: string
  - `sha256`: string

`artifact_import` MUST NOT execute code. It stores bytes only.

### `artifact_write`

Write or overwrite a derived artifact (e.g. tables, normalized specs, queues).

- Input:
  - `job_id`: string
  - `path`: string (job-relative)
  - `encoding`: `"utf-8" | "base64"`
  - `content`: string
  - `media_type`: string
- Output:
  - `path`: string
  - `sha256`: string

### `artifact_delete` (optional)

Delete a job-local artifact path.

## Provenance and Indexing

If Brain v2 maintains an `index.json` artifact list, then:

1. `artifact_import` SHOULD append an entry to `artifacts[]` with provenance fields when present.
2. `artifact_write` MAY append or update entries depending on whether the file is new.
3. Hashes MUST be computed from the on-disk raw bytes.

## Tests

Brain MUST have offline tests proving:

1. `artifact_import` writes bytes and returns correct sha256.
2. Path traversal is rejected.
3. `artifact_write` can overwrite content deterministically (hash changes).

