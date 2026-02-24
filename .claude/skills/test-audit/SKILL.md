---
name: test-audit
description: Verify test coverage consistency across the neuron workspace
---

# Test Audit

Run this skill after adding features, changing public API, or adding new crates.
It systematically checks that test coverage is consistent with the codebase.

## When to run

- After implementing a new feature or public API change
- After adding or removing a crate from the workspace
- Before any release
- When asked to "audit tests" or "check test coverage"

## Discovery

All checks start by parsing the root `Cargo.toml` `[workspace].members` to get
the canonical crate set. No crate names are hardcoded — if a new crate is added,
these checks automatically cover it.

## Checklist

Work through each check in order. Report findings as you go.

### 1. Every crate has tests

For each workspace member, verify at least one of:

- A `tests/` directory containing `.rs` files
- A `#[cfg(test)]` module in any `src/*.rs` file

Flag any crate with zero test files.

### 2. Public API test coverage

For each crate:

1. Read `src/lib.rs` and extract all public names: `pub use`, `pub struct`,
   `pub enum`, `pub trait`, `pub fn`, `pub type`
2. For glob re-exports (`pub use module::*`), read the source module to expand
   the names
3. Grep the crate's `tests/` directory and inline `#[cfg(test)]` modules for
   each name
4. Flag any public type, trait, or function that appears in zero test files

This is a **heuristic** (name-based grep, not semantic analysis). False positives
are acceptable — it's better to over-flag and let the auditor assess than to
miss gaps.

### 3. Error variant coverage

For each crate that defines error enums (look for `#[derive(thiserror::Error)]`
or files named `error.rs`):

1. Extract each enum variant name
2. Grep the crate's test files for that variant name
3. Flag any variant that appears in zero tests

Error paths are where bugs hide — every error variant should have at least one
test that exercises it.

### 4. Streaming parity

This check is **scoped dynamically**: it only runs if the workspace contains a
crate whose `src/` files define both `pub async fn run(` and
`pub async fn run_stream(` methods (currently `neuron-loop`).

For each such crate:

1. Grep `tests/` for test function names containing `run(` or `.run(`
   (non-streaming tests)
2. Extract the feature keyword from the test name (e.g., `usage_limits`,
   `cancellation`, `model_retry`, `compaction`, `hooks`)
3. Check if a corresponding test exists with `run_stream` or `stream` in the
   name testing the same feature
4. Flag features that are tested only via the non-streaming path

The streaming path often has different control flow and error handling — features
need coverage in both paths.

### 5. Example compilation

Run:

```
cargo build --workspace --examples
```

Report any compilation failures. This is a binary pass/fail.

### 6. Test infrastructure consistency

For each crate's test files (`tests/*.rs` and inline `#[cfg(test)]` modules):

- **Missing async attribute**: Flag any `async fn test_*` or `async fn *_test`
  that lacks `#[tokio::test]` (likely a missing attribute — the test won't run)
- **Brittle panic tests**: Flag any `#[should_panic]` without an
  `expected = "..."` message (these pass on ANY panic, hiding real bugs)

Report as informational findings, not hard failures.

### 7. Test count summary

Run `cargo test --workspace` and parse the output to report per-crate test
counts. This is **informational only** — no PASS/FAIL. It establishes a
baseline so regressions are visible.

Report as a table:

```
| Crate | Tests |
|-------|-------|
| neuron-types | 160 |
| neuron-tool | 60 |
| ... | ... |
| **Total** | **N** |
```

Include both integration tests (from `tests/`) and inline tests (from
`#[cfg(test)]` modules) in the count.

## Fixing issues

For each issue found:

- **Missing test for public type**: Add at least one test that constructs or
  uses the type. For traits, add a test with a mock implementation.
- **Missing error variant test**: Add a test that triggers the error condition
  and asserts the variant.
- **Missing streaming test**: Clone the `run()` test, adapt it to use
  `run_stream()`, and verify the same behavior via stream events.
- **Infrastructure issues**: Add missing `#[tokio::test]` attributes, add
  `expected` messages to `#[should_panic]`.

## Output format

Report results as:

```
## Test Audit Results

### Check 1: Every crate has tests - PASS/FAIL
[details if FAIL]

### Check 2: Public API test coverage - PASS/FAIL
[details if FAIL, listing untested types per crate]

### Check 3: Error variant coverage - PASS/FAIL
[details if FAIL, listing untested variants]

### Check 4: Streaming parity - PASS/FAIL
[details if FAIL, listing features missing streaming tests]

### Check 5: Example compilation - PASS/FAIL
[details if FAIL]

### Check 6: Test infrastructure - INFO
[any findings]

### Check 7: Test count summary - INFO
[table of per-crate counts]
```
