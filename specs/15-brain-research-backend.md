# Brain: Research Backend (v2) — Async Jobs + Grounded Bundles

This spec defines Brain v2 as a **structured research backend** intended to be used under an
external interactive harness (Claude Code, Codex, etc.) via MCP.

The purpose of Brain v2 is to produce **grounded, auditable research bundles** that can be used to
seed downstream specification and implementation workflows.

Brain v2 is not a “chat agent product UI”. It is a ResearchOps service.

## Goals

Brain v2 MUST:

1. Run as an MCP server, exposing a small set of tools for:
   - starting research jobs
   - polling job status
   - retrieving finished bundles
   - drilling into artifacts (list/read)
2. Execute research work asynchronously (job-based), supporting long-running tasks and polling.
3. Produce a **machine-readable bundle index** plus a **human-readable distillation** for every job.
4. Store raw artifacts (sources, extracts, intermediate notes) as **real files** with stable paths,
   so humans can inspect them outside the harness.
5. Enforce **groundedness**:
   - every factual claim that is not marked as an assumption MUST point to evidence in artifacts
   - evidence MUST include retrieval timestamps and stable references to source snapshots
6. Be testable offline (default test suite requires no network).
7. Support pluggable acquisition backends (Parallel, Firecrawl, BrightData, etc.) via MCP tool
   import + aliasing, without locking Brain to a specific vendor.

Brain v2 SHOULD:

1. Support job groups (fan-out research across many competitors/doc sets; later merge).
2. Support incremental refinement (re-run / refresh jobs against the same target, producing a new
   bundle version while preserving prior snapshots).
3. Support strict budget enforcement (max wall time, max tool calls, max cost estimates).

## Non-Goals

Brain v2 is NOT:

1. A web crawler/scraper implementation. It delegates acquisition to backends.
2. A general workflow engine (Temporal/Restate/etc.).
3. A patch applier or product implementation system.
4. A replacement for interactive harness UX.

Brain v2 MAY later support “bundle → spec” synthesis as an optional add-on (**TODO**, see below),
but v2 core is “research bundle A” first.

## Definitions

- **Job**: An asynchronous unit of work that produces exactly one bundle on success.
- **Bundle**: The durable, auditable output of a job: machine index + human distillation + artifacts.
- **Artifact**: A file written under a job’s artifact directory (sources, extracts, notes, indexes).
- **Claim**: A statement in the machine index that is either (a) grounded with evidence, or
  (b) explicitly marked as an assumption/design choice.
- **Evidence**: A pointer from a claim to one or more artifacts containing the supporting material.

## Determinism Policy (Pragmatic)

Brain v2 MUST target **snapshot determinism**:

- Given an immutable set of artifact files (byte-for-byte), Brain MUST produce the same bundle index
  and distillation outputs.

Brain v2 MUST NOT claim “internet determinism”:

- live web data changes
- backends change behavior
- harness models change

Brain v2 SHOULD record enough provenance to make “what changed” debuggable (timestamps + hashes).

## Bundle Contract (Outputs)

Every completed job MUST materialize a bundle under a job-local artifact root:

`{artifact_root}/{job_id}/`

At minimum the following files MUST exist:

1. `index.json` (machine-readable bundle index)
2. `findings.md` (human-readable distillation)

The bundle SHOULD include:

- `sources/` — raw snapshots (HTML, markdown, PDF text extracts, API responses, etc.)
- `notes/` — intermediate notes, scratch pads, tool outputs
- `tables/` — normalized tables (competitor matrix, feature map, etc.)

### `index.json` (Minimum Required Shape)

The exact schema can evolve, but v2 MUST include:

- `job`:
  - `id`: string
  - `created_at`: RFC3339 string
  - `status`: `"pending" | "running" | "succeeded" | "failed" | "canceled"`
  - `inputs`: object (original user intent, constraints, target set)
- `artifacts[]`:
  - `path`: string (job-relative, e.g. `sources/foo.md`)
  - `sha256`: string (hex)
  - `media_type`: string (e.g. `text/markdown`, `text/html`, `application/json`)
  - `retrieved_at`: RFC3339 string (when applicable)
  - `source_url`: string (when applicable)
- `claims[]`:
  - `id`: string
  - `kind`: `"fact" | "assumption" | "design_choice"`
  - `statement`: string
  - `evidence[]` (required when kind == `"fact"`):
    - `artifact_path`: string
    - `excerpt`: string (small quote/snippet; optional but recommended)
    - `locator`: object (optional; e.g. line numbers, selectors, offsets)
    - `retrieved_at`: RFC3339 string
    - `source_url`: string (optional if known)
- `coverage`:
  - `targets[]`: list of researched entities (competitors / docs / repos)
  - `gaps[]`: list of known missing areas
- `next_steps[]`: suggested follow-up research tasks, each grounded in coverage gaps

Brain v2 MUST ensure the bundle can be consumed by downstream systems without reading raw artifacts.

## Artifact Storage Policy

Brain v2 MUST write artifacts as **real files** in the repository-local `.brain/` directory by
default:

- `artifact_root`: `.brain/artifacts`

Brain v2 MUST allow overriding `artifact_root` via config and/or CLI.

Safety constraints:

1. All tool inputs that include file paths MUST be validated as **job-relative** paths.
2. Path traversal (`..`) and absolute paths MUST be rejected.
3. Artifact writes MUST create parent directories as needed.

## MCP Tool Surface (Brain v2)

Brain v2 MUST expose the following MCP tools (stable ids):

1. `research_job_start`
   - Input: `{ "intent": "...", "constraints": { ... }, "targets": [ ... ], "tool_policy": { ... } }`
   - Output: `{ "job_id": "...", "status": "pending" | "running" }`
2. `research_job_status`
   - Input: `{ "job_id": "..." }`
   - Output: `{ "job_id": "...", "status": "...", "progress": { ... } }`
3. `research_job_get`
   - Input: `{ "job_id": "..." }`
   - Output: `{ "job_id": "...", "status": "succeeded", "bundle": { "artifact_root": "...", "index_path": "index.json", "findings_path": "findings.md" } }`
4. `research_job_cancel`
   - Input: `{ "job_id": "..." }`
   - Output: `{ "job_id": "...", "status": "canceled" }`

Brain v2 MUST expose the following artifact inspection tools:

1. `artifact_list`
   - Input: `{ "job_id": "...", "prefix": "sources/" }`
   - Output: `{ "artifacts": [ { "path": "...", "sha256": "..." } ] }`
2. `artifact_read`
   - Input: `{ "job_id": "...", "path": "sources/foo.md" }`
   - Output: `{ "path": "...", "content": "...", "sha256": "..." }`

Brain v2 MAY add additional tools for:

- artifact search/grep (bounded)
- bundle diffing across job versions

## Acquisition Backends (Pluggable)

Brain v2 MUST support acquisition via imported MCP tools and aliasing.

Principle:

- Brain defines **canonical internal roles** (e.g. `web_search`, `web_fetch`, `deep_research_start`,
  `deep_research_get`) but does not hardcode vendor tool names.

Integration mechanism:

- `.mcp.json` `mcpServers` defines backend connections.
- `x-brain.aliases` maps vendor tool names to canonical tool ids used in Brain prompts/policies.

Parallel.ai is an example of a first-class preferred backend for deep research:

- Parallel Search MCP (web search / preview) can serve the `web_search` role.
- Parallel Task MCP (async deep research tasks) can serve the `deep_research_start/get` roles.

Brain MUST remain compatible with other backends that can satisfy those roles.

## Quality Gates (Groundedness)

Brain v2 MUST:

1. Reject bundles where `claims[].kind == "fact"` and `evidence[]` is empty.
2. Ensure evidence points to job-local artifacts, not external mutable links.
3. Record retrieval timestamps for web-derived evidence.

Brain v2 SHOULD:

1. Prefer multiple independent sources for high-stakes claims.
2. Track confidence and disagreement explicitly in the index.

## Tests

Brain v2 MUST include offline tests that prove:

1. Jobs can run with injected mock acquisition tools.
2. Bundles are written to disk with:
   - `index.json`
   - `findings.md`
   - at least one `sources/*` artifact
3. `artifact_list` and `artifact_read` work and enforce path safety.

## TODO: Bundle → Spec (B)

Future work may add a “specifier” mode that takes a bundle index and emits:

- a product spec for a “clean-room reproduction” or “synthesized improved product”

Hard constraint:

- no spec claim without evidence or explicit “assumption/design choice” markers.

