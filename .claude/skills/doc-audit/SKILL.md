---
name: doc-audit
description: Verify documentation-code consistency across the neuron workspace
---

# Doc Audit

Run this skill after any code change that touches public API, adds/removes
crates, or modifies examples. It systematically checks that all documentation
surfaces are consistent with the actual code.

## When to run

- After implementing a feature that changes public API
- After adding or removing a crate from the workspace
- After adding or removing examples
- Before any release
- When asked to "audit docs" or "check doc consistency"

## Checklist

Work through each check in order. Report findings as you go.

### 1. Workspace members vs doc listings

Parse `Cargo.toml` `[workspace].members` to get the canonical crate set. Verify
the same set appears in each of these files:

| File | Section to check |
|------|-----------------|
| `CLAUDE.md` | Dependency graph ASCII art |
| `llms.txt` | "Crates" section (~lines 43-55) |
| `README.md` | "Crates" table |
| `docs/book/src/introduction.md` | "What's Included" table |

Do NOT check for exact count — check that the **set** of crate names matches.
If a crate is missing from any surface, flag it.

### 2. Public re-exports vs crate CLAUDE.md

For each crate with a `CLAUDE.md`:

1. Read `src/lib.rs` and list all `pub use` re-exports
2. Read the crate's `CLAUDE.md` "Key types" or equivalent section
3. Flag any re-exported type not mentioned in the crate `CLAUDE.md`

### 3. Examples vs docs

For each `[[example]]` entry in any crate's `Cargo.toml`, or any `.rs` file in
an `examples/` directory:

1. Verify the example appears in `llms.txt` examples list
2. Verify the example appears in the parent crate's `README.md` if one exists

### 4. Feature flags vs docs

Parse `neuron/Cargo.toml` `[features]` section. Verify each feature appears in:

- `neuron/README.md` feature table
- `README.md` (root) feature flags table
- `docs/book/src/getting-started/installation.md` feature flags table

### 5. ROADMAP hygiene

Check `ROADMAP.md` for:

- No completion markers in "Later" section (no checkmarks, `[x]`, or "shipped")
- Section headers contain no version numbers (e.g., "Now" not "Now (v0.2)")
- Items listed under "Now" actually exist in the codebase

### 6. Volatile fact scan

Grep all `.md` files for patterns that suggest hardcoded counts:

```
\b\d+ (independent )?crates\b
\b\d+ providers\b
\b\d+ examples\b
\b\d+ guides\b
\b\d+ integrations\b
```

Exclude `docs/competitive-analysis.md` (point-in-time snapshot document).
Exclude code blocks (lines starting with spaces/tabs or inside triple backticks).

Flag any matches. Reference the "Documentation writing rules" section in
`CLAUDE.md` for how to fix them.

### 7. Dependency graph vs Cargo.toml

Verify the ASCII dependency graph in root `CLAUDE.md` matches actual
`[dependencies]` in each crate's `Cargo.toml`:

1. For each crate in the graph, read its `Cargo.toml` `[dependencies]`
2. Check that every inter-workspace dependency shown in the graph exists
3. Check that no inter-workspace dependency is missing from the graph

## Fixing issues

For each issue found, reference the "Documentation writing rules" section in
`CLAUDE.md` for guidance on what to write. Key principles:

- Name things, don't count them
- Omit version numbers from prose
- Link to canonical sources (Cargo.toml, examples/ directories) instead of
  maintaining parallel lists
- When a list must be exhaustive (llms.txt), keep it but remove count claims

## Output format

Report results as:

```
## Doc Audit Results

### Check 1: Workspace members - PASS/FAIL
[details if FAIL]

### Check 2: Public re-exports - PASS/FAIL
[details if FAIL]

...
```
