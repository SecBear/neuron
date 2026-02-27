# Brain → Factory: “SpecPack” Design (2026-02-27)

## Summary

Brain’s next step (after v2 research bundles) is to output **factory-ready SpecPacks**:

- canonical spec library split across files (easy for humans + progressive load)
- a hash-locked manifest (index-only; prevents drift)
- a machine-readable work queue (enables parallel Ralph workers safely)

This design intentionally keeps **one source of truth** to avoid drift.

## One Source of Truth

Canonical truth:

- `specpack/specs/*.md`

Everything else is either:

- an index that references canonical files by hash (`manifest.json`)
- a scheduler input that references canonical files by path/anchor (`queue.json`)

No duplicated prose across JSON/MD.

## Why Files Are Canonical

Humans:

- review in git
- diff in PRs
- edit surgically

Factories:

- progressive load only the referenced spec files
- avoid reading raw research artifacts unless needed

## Parallelization Without Conflicts

Each `queue.json` task includes:

- `file_ownership.allow_globs[]` / `deny_globs[]`
- `concurrency.group` (mutual exclusion)
- explicit backpressure commands (`backpressure.verify[]`)

This lets a factory schedule many workers concurrently without stomping each other.

## Relationship to Brain v2 Bundles

The research bundle remains the durable evidence base:

- `index.json` + `findings.md` + raw artifacts

SpecPacks are derived artifacts under the same job directory:

- `specpack/…`

The external harness is expected to do the heavy synthesis work (using its own models/pricing),
while Brain provides storage + validation + contracts.

