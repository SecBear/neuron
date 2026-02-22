# Contributing to neuron

Thank you for your interest in contributing to neuron! This document covers the
process for contributing and the standards we maintain.

## Project overview

neuron is a Rust workspace of independent crates that provide building blocks
for AI agent construction. Each crate is versioned and published separately.
See the root `Cargo.toml` for the full list of workspace members.

## Getting started

### Prerequisites

- **Rust 1.90+** (edition 2024)
- A working internet connection for downloading crate dependencies

### Fork and branch workflow

1. Fork the repository on GitHub.
2. Clone your fork locally:
   ```bash
   git clone https://github.com/<your-username>/rust-agent-blocks.git
   cd rust-agent-blocks
   ```
3. Create a feature branch from `main`:
   ```bash
   git checkout -b feat/my-feature main
   ```
4. Make your changes, following the conventions below.
5. Push your branch and open a Pull Request against `main`.

## Conventions

All coding conventions, architectural decisions, and design principles are
documented in [`CLAUDE.md`](./CLAUDE.md) at the repository root. Read it before
submitting your first PR. Key highlights:

### Rust standards

- **Edition 2024**, resolver 3, minimum Rust 1.90
- **Native async traits** -- use `-> impl Future<Output = T> + Send`. No
  `#[async_trait]`.
- **`thiserror`** for error types, two levels of nesting maximum.
- **`schemars`** for JSON Schema derivation on tool inputs.
- No `unwrap()` in library code.
- `#[must_use]` on Result-returning functions.

### Crate structure

Every crate follows a flat file layout:

```
neuron-{block}/
    Cargo.toml
    CLAUDE.md
    src/
        lib.rs          # Public API, re-exports, module docs
        types.rs        # All types in one place
        traits.rs       # All traits in one place
        {feature}.rs    # One file per feature
        error.rs        # Error types
    tests/
        integration.rs
    examples/
        basic.rs
```

No deep directory nesting. One concept per file, named obviously.

### Documentation

- Inline `///` doc comments on **every** public item.
- Every trait must have a doc example.
- When adding or changing public API, update all documentation surfaces in the
  same commit: source doc comments, crate `CLAUDE.md`, crate `README.md`,
  examples, `ROADMAP.md`, root `CLAUDE.md`, and `llms.txt` as applicable. See
  the "Documentation completeness checklist" in `CLAUDE.md` for the full list.

## Commit messages

We use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

Format:

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

Types: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `ci`, `perf`.

Scope is typically the crate name without the `neuron-` prefix (e.g., `types`,
`loop`, `runtime`, `provider-anthropic`). Use no scope for workspace-wide
changes.

Examples:

```
feat(types): add EmbeddingProvider trait and embedding types
fix(loop): handle compaction stop reason correctly
docs: update all doc surfaces for v0.2 features
chore: add release-please config and initial CHANGELOGs
```

## Running checks

Before submitting a PR, ensure all of the following pass:

```bash
cargo fmt --check
cargo clippy --workspace
cargo test --workspace
cargo doc --workspace --no-deps
```

Additionally, verify that examples compile:

```bash
cargo build --workspace --examples
```

## Pull request process

1. Fill out the PR template completely.
2. Ensure all CI checks pass.
3. Keep PRs focused -- one concern per PR.
4. If your change adds a public type or trait, confirm you have updated all
   documentation surfaces listed in the "Documentation completeness checklist"
   in `CLAUDE.md`.
5. Add or update tests for any behavioral changes.
6. Add a CHANGELOG entry in the affected crate(s).

## License

By contributing to neuron, you agree that your contributions will be dual
licensed under the [MIT License](./LICENSE-MIT) and the
[Apache License 2.0](./LICENSE-APACHE), at the user's option. This is the same
license used by the project itself.
