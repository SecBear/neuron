# Path to 10/10 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Bring neuron to 10/10 across documentation, discoverability, test
robustness, and polish — then launch publicly.

**Architecture:** Three sequential phases — trust signals (CI, community files,
badges, advanced tests), content (mdBook docs site, examples), and launch
(version bump, publish, announce). Within each phase, independent tasks can be
parallelized.

**Tech Stack:** cargo-tarpaulin, cargo-audit, cargo-deny, lychee, proptest,
criterion, cargo-fuzz, mdBook, GitHub Actions, GitHub Pages

---

## Phase 1: Trust Signals

### Task 1: Community files

**Files:**
- Create: `CONTRIBUTING.md`
- Create: `CODE_OF_CONDUCT.md`
- Create: `SECURITY.md`
- Create: `.github/ISSUE_TEMPLATE/bug_report.md`
- Create: `.github/ISSUE_TEMPLATE/feature_request.md`
- Create: `.github/pull_request_template.md`

**Step 1: Create CONTRIBUTING.md**

```markdown
# Contributing to neuron

Thank you for your interest in contributing to neuron! This guide will help you
get started.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/neuron.git`
3. Create a branch: `git checkout -b feature/your-feature`
4. Make your changes
5. Run the checks: `cargo fmt --check && cargo clippy --workspace && cargo test --workspace`
6. Commit using [Conventional Commits](https://www.conventionalcommits.org/):
   `git commit -m "feat(crate): short description"`
7. Push and open a Pull Request

## Development Setup

- Rust 1.90+ (edition 2024)
- Run `cargo build --workspace` to verify everything compiles
- Run `cargo test --workspace` to run all tests

## Project Structure

neuron is a workspace of 11 independent crates. See
[CLAUDE.md](CLAUDE.md) for architecture, conventions, and design decisions.

Each crate follows the same layout:

```
neuron-{crate}/
    Cargo.toml
    src/
        lib.rs          # Public API, re-exports
        {feature}.rs    # One file per feature
    tests/
        integration.rs  # Integration tests
    examples/
        basic.rs        # Usage examples
```

## Coding Conventions

- **No `unwrap()` in library code** — use `?` or return `Result`
- **Doc comments on every public item** — `///` with usage examples for traits
- **`#[must_use]` on Result-returning functions**
- **Flat file structure** — one concept per file, no deep nesting
- **thiserror for errors** — 2 levels max

## Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/) for
automatic changelog generation:

- `feat(crate): add new feature`
- `fix(crate): fix bug description`
- `docs(crate): update documentation`
- `chore(crate): maintenance task`
- `refactor(crate): restructure without behavior change`

## Testing

- Run all tests: `cargo test --workspace`
- Run specific crate: `cargo test -p neuron-types`
- Run with output: `cargo test --workspace -- --nocapture`
- Clippy: `cargo clippy --workspace --all-features`
- Format: `cargo fmt --check`

Every PR must pass: `cargo fmt --check && cargo clippy --workspace && cargo test --workspace && cargo doc --workspace --no-deps`

## Documentation Updates

Code changes that affect public API must update documentation in the same
commit. See the "Documentation completeness checklist" in
[CLAUDE.md](CLAUDE.md#documentation-completeness-checklist) for the full list.

## License

By contributing, you agree that your contributions will be dual-licensed under
MIT and Apache-2.0, matching the project's existing license.
```

**Step 2: Create CODE_OF_CONDUCT.md**

Use the Contributor Covenant v2.1. Full text at
https://www.contributor-covenant.org/version/2/1/code_of_conduct/

Set contact method to: the repository's GitHub Issues (or a project email if
you have one).

**Step 3: Create SECURITY.md**

```markdown
# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.2.x   | ✅ Current |
| < 0.2   | ❌ No     |

## Reporting a Vulnerability

**Do not report security vulnerabilities through public GitHub issues.**

Instead, please email **security@secbear.com** (or your preferred email) with:

- Description of the vulnerability
- Steps to reproduce
- Affected crate(s) and version(s)
- Any potential impact assessment

You should receive an acknowledgment within 48 hours. We will work with you to
understand and address the issue before any public disclosure.

## Scope

This policy covers all crates in the neuron workspace:

- neuron-types, neuron-tool, neuron-tool-macros, neuron-context
- neuron-loop, neuron-provider-anthropic, neuron-provider-openai
- neuron-provider-ollama, neuron-mcp, neuron-runtime, neuron
```

**Step 4: Create issue templates**

`.github/ISSUE_TEMPLATE/bug_report.md`:

```markdown
---
name: Bug Report
about: Report a bug in neuron
labels: bug
---

## Crate

Which crate is affected? (e.g., neuron-types, neuron-loop)

## Version

What version are you using? (`cargo tree -p neuron-types` to check)

## Description

What happened?

## Steps to Reproduce

1.
2.
3.

## Expected Behavior

What should have happened?

## Actual Behavior

What happened instead?

## Environment

- Rust version: (`rustc --version`)
- OS:
- Additional context:
```

`.github/ISSUE_TEMPLATE/feature_request.md`:

```markdown
---
name: Feature Request
about: Suggest a new feature for neuron
labels: enhancement
---

## Use Case

What problem does this solve?

## Proposed API

What would the code look like?

```rust
// Example usage
```

## Alternatives Considered

What other approaches did you consider?

## Which Crate?

Where should this live? (e.g., neuron-types, neuron-tool, new crate)
```

**Step 5: Create PR template**

`.github/pull_request_template.md`:

```markdown
## Summary

What does this PR do?

## Checklist

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo doc --workspace --no-deps` has no warnings
- [ ] Doc comments added/updated for public API changes
- [ ] CHANGELOG.md entry added (if user-facing change)
- [ ] Examples added/updated (if new feature)
```

**Step 6: Commit**

```bash
git add CONTRIBUTING.md CODE_OF_CONDUCT.md SECURITY.md .github/ISSUE_TEMPLATE/ .github/pull_request_template.md
git commit -m "docs: add community files (CONTRIBUTING, COC, SECURITY, templates)"
```

---

### Task 2: CI hardening

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `deny.toml`

**Step 1: Create deny.toml for cargo-deny**

```toml
[advisories]
vulnerability = "deny"
unmaintained = "warn"

[licenses]
allow = [
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "OpenSSL",
    "Zlib",
]
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

**Step 2: Expand ci.yml**

Replace the entire file with:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -Dwarnings

jobs:
  check:
    name: Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - name: Format
        run: cargo fmt --check
      - name: Clippy
        run: cargo clippy --workspace --all-features -- -D warnings
      - name: Test
        run: cargo test --workspace
      - name: Doc
        run: cargo doc --workspace --no-deps

  coverage:
    name: Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install tarpaulin
        run: cargo install cargo-tarpaulin
      - name: Generate coverage
        run: cargo tarpaulin --workspace --out xml --output-dir coverage/
      - name: Upload to Codecov
        uses: codecov/codecov-action@v4
        with:
          files: coverage/cobertura.xml
          fail_ci_if_error: false
        env:
          CODECOV_TOKEN: ${{ secrets.CODECOV_TOKEN }}

  security:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}

  msrv:
    name: MSRV (1.90)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: "1.90"
      - uses: Swatinem/rust-cache@v2
      - name: Check MSRV
        run: cargo check --workspace

  deny:
    name: Dependency Review
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v1

  links:
    name: Link Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check links
        uses: lycheeverse/lychee-action@v1
        with:
          args: --verbose --no-progress '**/*.md'
          fail: true
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

**Step 3: Verify locally**

```bash
cargo fmt --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

**Step 4: Commit**

```bash
git add .github/workflows/ci.yml deny.toml
git commit -m "ci: add coverage, security audit, MSRV, cargo-deny, link check"
```

---

### Task 3: Update README badges

**Files:**
- Modify: `README.md` (lines 3-5, the badge section)

**Step 1: Replace badge section**

Replace the existing three badges:

```markdown
[![crates.io](https://img.shields.io/crates/v/neuron.svg)](https://crates.io/crates/neuron)
[![docs.rs](https://docs.rs/neuron/badge.svg)](https://docs.rs/neuron)
[![license](https://img.shields.io/crates/l/neuron.svg)](LICENSE-MIT)
```

With six badges:

```markdown
[![CI](https://github.com/secbear/neuron/actions/workflows/ci.yml/badge.svg)](https://github.com/secbear/neuron/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/secbear/neuron/graph/badge.svg)](https://codecov.io/gh/secbear/neuron)
[![crates.io](https://img.shields.io/crates/v/neuron.svg)](https://crates.io/crates/neuron)
[![docs.rs](https://docs.rs/neuron/badge.svg)](https://docs.rs/neuron)
[![MSRV](https://img.shields.io/badge/MSRV-1.90-blue.svg)](https://blog.rust-lang.org/)
[![license](https://img.shields.io/crates/l/neuron.svg)](LICENSE-MIT)
```

**Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add CI, coverage, and MSRV badges to README"
```

---

### Task 4: Property-based tests

**Files:**
- Modify: `Cargo.toml` (workspace dev-dependencies, add proptest)
- Modify: `neuron-types/Cargo.toml` (add proptest dev-dep)
- Create: `neuron-types/tests/proptest_serde.rs`
- Create: `neuron-types/tests/proptest_errors.rs`
- Modify: `neuron-context/Cargo.toml` (add proptest dev-dep)
- Create: `neuron-context/tests/proptest_strategies.rs`
- Modify: `neuron-tool/Cargo.toml` (add proptest dev-dep)
- Create: `neuron-tool/tests/proptest_middleware.rs`

**Step 1: Add proptest to workspace dependencies**

In root `Cargo.toml`, under `[workspace.dependencies]` after the `# Dev-only`
section, add:

```toml
proptest = "1"
```

**Step 2: Add proptest to crate dev-dependencies**

In each of `neuron-types/Cargo.toml`, `neuron-context/Cargo.toml`,
`neuron-tool/Cargo.toml`, add under `[dev-dependencies]`:

```toml
proptest.workspace = true
```

**Step 3: Create neuron-types serde roundtrip property tests**

`neuron-types/tests/proptest_serde.rs`:

```rust
//! Property-based tests: serde roundtrip for all public types.

use proptest::prelude::*;
use neuron_types::*;

fn arb_role() -> impl Strategy<Value = Role> {
    prop_oneof![
        Just(Role::User),
        Just(Role::Assistant),
        Just(Role::System),
    ]
}

fn arb_content_block() -> impl Strategy<Value = ContentBlock> {
    prop_oneof![
        any::<String>().prop_map(ContentBlock::Text),
        (any::<String>(), any::<String>()).prop_map(|(t, s)| ContentBlock::Thinking {
            thinking: t,
            signature: s,
        }),
        any::<String>().prop_map(|d| ContentBlock::RedactedThinking { data: d }),
        any::<String>().prop_map(|c| ContentBlock::Compaction { content: c }),
    ]
}

fn arb_message() -> impl Strategy<Value = Message> {
    (arb_role(), proptest::collection::vec(arb_content_block(), 0..5))
        .prop_map(|(role, content)| Message { role, content })
}

fn arb_stop_reason() -> impl Strategy<Value = StopReason> {
    prop_oneof![
        Just(StopReason::EndTurn),
        Just(StopReason::ToolUse),
        Just(StopReason::MaxTokens),
        Just(StopReason::StopSequence),
        Just(StopReason::ContentFilter),
        Just(StopReason::Compaction),
    ]
}

proptest! {
    #[test]
    fn message_serde_roundtrip(msg in arb_message()) {
        let json = serde_json::to_string(&msg).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        // Compare role (PartialEq)
        prop_assert_eq!(msg.role, back.role);
        prop_assert_eq!(msg.content.len(), back.content.len());
    }

    #[test]
    fn stop_reason_serde_roundtrip(sr in arb_stop_reason()) {
        let json = serde_json::to_string(&sr).unwrap();
        let back: StopReason = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(sr, back);
    }

    #[test]
    fn embedding_request_default_fields(
        model in ".*",
        dims in proptest::option::of(1usize..4096),
    ) {
        let req = EmbeddingRequest {
            model: model.clone(),
            input: vec!["test".to_string()],
            dimensions: dims,
            ..Default::default()
        };
        prop_assert_eq!(req.model, model);
        prop_assert_eq!(req.dimensions, dims);
    }

    #[test]
    fn embedding_response_serde_roundtrip(
        model in "[a-z-]+",
        n_embeddings in 1usize..5,
        dim in 1usize..10,
        prompt_tokens in 0usize..10000,
        total_tokens in 0usize..10000,
    ) {
        let resp = EmbeddingResponse {
            model,
            embeddings: (0..n_embeddings).map(|_| vec![0.0f32; dim]).collect(),
            usage: EmbeddingUsage { prompt_tokens, total_tokens },
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: EmbeddingResponse = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(resp, back);
    }
}
```

**Step 4: Create neuron-types error classification property tests**

`neuron-types/tests/proptest_errors.rs`:

```rust
//! Property-based tests: error classification consistency.

use proptest::prelude::*;
use neuron_types::*;
use std::time::Duration;

fn arb_provider_error() -> impl Strategy<Value = ProviderError> {
    prop_oneof![
        any::<String>().prop_map(ProviderError::Authentication),
        proptest::option::of(0u64..3600)
            .prop_map(|secs| ProviderError::RateLimit {
                retry_after: secs.map(Duration::from_secs),
            }),
        any::<String>().prop_map(ProviderError::InvalidRequest),
        any::<String>().prop_map(ProviderError::ModelNotFound),
        any::<String>().prop_map(ProviderError::ServiceUnavailable),
    ]
}

fn arb_embedding_error() -> impl Strategy<Value = EmbeddingError> {
    prop_oneof![
        any::<String>().prop_map(EmbeddingError::Authentication),
        proptest::option::of(0u64..3600)
            .prop_map(|secs| EmbeddingError::RateLimit {
                retry_after: secs.map(Duration::from_secs),
            }),
        any::<String>().prop_map(EmbeddingError::InvalidRequest),
    ]
}

proptest! {
    #[test]
    fn provider_error_retryable_classification(err in arb_provider_error()) {
        let retryable = err.is_retryable();
        match &err {
            ProviderError::RateLimit { .. } => prop_assert!(retryable),
            ProviderError::ServiceUnavailable(_) => prop_assert!(retryable),
            ProviderError::Authentication(_) => prop_assert!(!retryable),
            ProviderError::InvalidRequest(_) => prop_assert!(!retryable),
            ProviderError::ModelNotFound(_) => prop_assert!(!retryable),
            _ => {} // Other variants
        }
    }

    #[test]
    fn embedding_error_retryable_classification(err in arb_embedding_error()) {
        let retryable = err.is_retryable();
        match &err {
            EmbeddingError::RateLimit { .. } => prop_assert!(retryable),
            EmbeddingError::Authentication(_) => prop_assert!(!retryable),
            EmbeddingError::InvalidRequest(_) => prop_assert!(!retryable),
            _ => {}
        }
    }
}
```

**Step 5: Create neuron-context property tests**

`neuron-context/tests/proptest_strategies.rs`:

```rust
//! Property-based tests: context strategy invariants.

use proptest::prelude::*;
use neuron_types::*;
use neuron_context::TokenCounter;

fn arb_text_message() -> impl Strategy<Value = Message> {
    ("[a-zA-Z ]{1,200}", prop_oneof![Just(Role::User), Just(Role::Assistant)])
        .prop_map(|(text, role)| Message {
            role,
            content: vec![ContentBlock::Text(text)],
        })
}

proptest! {
    #[test]
    fn token_count_monotonic(
        messages in proptest::collection::vec(arb_text_message(), 1..20),
    ) {
        let counter = TokenCounter::default();
        let mut prev_count = 0;
        for i in 1..=messages.len() {
            let count = counter.count_messages(&messages[..i]);
            prop_assert!(count >= prev_count,
                "Token count decreased: {} -> {} at message {}",
                prev_count, count, i);
            prev_count = count;
        }
    }

    #[test]
    fn token_count_non_negative(msg in arb_text_message()) {
        let counter = TokenCounter::default();
        let count = counter.count_messages(&[msg]);
        prop_assert!(count > 0, "Token count should be positive for non-empty message");
    }
}
```

**Step 6: Create neuron-tool middleware ordering property tests**

`neuron-tool/tests/proptest_middleware.rs`:

```rust
//! Property-based tests: middleware chain ordering.

use proptest::prelude::*;
use neuron_tool::*;
use neuron_types::*;
use std::sync::{Arc, Mutex};

proptest! {
    #[test]
    fn middleware_execution_order(n_middleware in 2usize..6) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let order = Arc::new(Mutex::new(Vec::new()));
            let mut registry = ToolRegistry::new();

            for i in 0..n_middleware {
                let order_clone = order.clone();
                registry.add_middleware(move |req, ctx, next| {
                    let order = order_clone.clone();
                    Box::pin(async move {
                        order.lock().unwrap().push(i);
                        next(req, ctx).await
                    })
                });
            }

            // The exact tool execution doesn't matter for ordering test —
            // we just need to verify middleware runs in order.
            // If no tool is registered, middleware still runs (returns NotFound).
            let input = serde_json::json!({});
            let ctx = ToolContext::default();
            let _ = registry.call("nonexistent", input, &ctx).await;

            let recorded = order.lock().unwrap().clone();
            // Verify sequential ordering
            for (idx, &val) in recorded.iter().enumerate() {
                prop_assert_eq!(idx, val,
                    "Middleware {} ran at position {}", val, idx);
            }
        });
    }
}
```

**Step 7: Run tests**

```bash
cargo test --workspace
```

**Step 8: Commit**

```bash
git add Cargo.toml neuron-types/Cargo.toml neuron-context/Cargo.toml neuron-tool/Cargo.toml \
  neuron-types/tests/proptest_serde.rs neuron-types/tests/proptest_errors.rs \
  neuron-context/tests/proptest_strategies.rs neuron-tool/tests/proptest_middleware.rs
git commit -m "test: add property-based tests for serde roundtrips, error classification, and middleware ordering"
```

---

### Task 5: Criterion benchmarks

**Files:**
- Modify: `Cargo.toml` (workspace dev-dependencies, add criterion)
- Modify: `neuron-types/Cargo.toml` (add criterion + bench target)
- Create: `neuron-types/benches/serialization.rs`
- Modify: `neuron-context/Cargo.toml` (add criterion + bench target)
- Create: `neuron-context/benches/compaction.rs`
- Modify: `neuron-loop/Cargo.toml` (add criterion + bench target)
- Create: `neuron-loop/benches/turn_latency.rs`

**Step 1: Add criterion to workspace dependencies**

In root `Cargo.toml` under `# Dev-only`:

```toml
criterion = { version = "0.5", features = ["html_reports"] }
```

**Step 2: Add criterion dev-dep and bench targets to each crate**

For each of `neuron-types`, `neuron-context`, `neuron-loop`, add to their
`Cargo.toml`:

```toml
criterion.workspace = true
```

under `[dev-dependencies]`, and add the bench target:

```toml
[[bench]]
name = "{bench_name}"
harness = false
```

Where `{bench_name}` is `serialization`, `compaction`, `turn_latency`
respectively.

**Step 3: Create neuron-types serialization benchmark**

`neuron-types/benches/serialization.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use neuron_types::*;

fn make_message(n_blocks: usize) -> Message {
    Message {
        role: Role::User,
        content: (0..n_blocks)
            .map(|i| ContentBlock::Text(format!("Block {i} with some content")))
            .collect(),
    }
}

fn make_request(n_messages: usize) -> CompletionRequest {
    CompletionRequest {
        model: "test-model".to_string(),
        messages: (0..n_messages).map(|_| make_message(3)).collect(),
        ..Default::default()
    }
}

fn bench_message_serialize(c: &mut Criterion) {
    let msg = make_message(5);
    c.bench_function("message_serialize_5_blocks", |b| {
        b.iter(|| serde_json::to_string(black_box(&msg)).unwrap())
    });
}

fn bench_request_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("completion_request_serialize");
    for n in [10, 100, 1000] {
        let req = make_request(n);
        group.bench_function(format!("{n}_messages"), |b| {
            b.iter(|| serde_json::to_string(black_box(&req)).unwrap())
        });
    }
    group.finish();
}

criterion_group!(benches, bench_message_serialize, bench_request_serialize);
criterion_main!(benches);
```

**Step 4: Create neuron-context compaction benchmark**

`neuron-context/benches/compaction.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use neuron_context::{SlidingWindowStrategy, TokenCounter};
use neuron_types::*;

fn make_conversation(n: usize) -> Vec<Message> {
    (0..n)
        .map(|i| Message {
            role: if i % 2 == 0 { Role::User } else { Role::Assistant },
            content: vec![ContentBlock::Text(format!(
                "Message {i}: This is a moderately sized message with enough content \
                 to be realistic for token counting benchmarks."
            ))],
        })
        .collect()
}

fn bench_token_counting(c: &mut Criterion) {
    let counter = TokenCounter::default();
    let mut group = c.benchmark_group("token_count");
    for n in [100, 1000, 10000] {
        let msgs = make_conversation(n);
        group.bench_function(format!("{n}_messages"), |b| {
            b.iter(|| counter.count_messages(black_box(&msgs)))
        });
    }
    group.finish();
}

fn bench_sliding_window(c: &mut Criterion) {
    let mut group = c.benchmark_group("sliding_window");
    for n in [100, 1000, 10000] {
        let msgs = make_conversation(n);
        let strategy = SlidingWindowStrategy::new(n / 2);
        group.bench_function(format!("{n}_messages"), |b| {
            b.iter(|| {
                let mut cloned = black_box(msgs.clone());
                strategy.compact(&mut cloned);
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_token_counting, bench_sliding_window);
criterion_main!(benches);
```

**Step 5: Create neuron-loop turn latency benchmark**

`neuron-loop/benches/turn_latency.rs`:

```rust
use criterion::{criterion_group, criterion_main, Criterion};
use neuron_loop::*;
use neuron_tool::ToolRegistry;
use neuron_types::*;
use std::sync::Arc;

/// A mock provider that returns immediately with a fixed response.
#[derive(Clone)]
struct InstantProvider;

impl Provider for InstantProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        async {
            Ok(CompletionResponse {
                id: "bench".to_string(),
                model: "mock".to_string(),
                message: Message::assistant("Done"),
                usage: TokenUsage::default(),
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> impl std::future::Future<Output = Result<StreamHandle, ProviderError>> + Send {
        async { unimplemented!() }
    }
}

fn bench_single_turn(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let registry = Arc::new(ToolRegistry::new());

    c.bench_function("single_turn_no_tools", |b| {
        b.iter(|| {
            rt.block_on(async {
                let loop_inst = AgentLoopBuilder::new()
                    .provider(InstantProvider)
                    .tool_registry(registry.clone())
                    .max_turns(1)
                    .build();
                let messages = vec![Message::user("Hello")];
                let ctx = ToolContext::default();
                let _ = loop_inst.run(messages, &ctx).await;
            })
        })
    });
}

criterion_group!(benches, bench_single_turn);
criterion_main!(benches);
```

**Step 6: Run benchmarks to verify they compile and execute**

```bash
cargo bench --workspace
```

**Step 7: Commit**

```bash
git add Cargo.toml neuron-types/Cargo.toml neuron-types/benches/ \
  neuron-context/Cargo.toml neuron-context/benches/ \
  neuron-loop/Cargo.toml neuron-loop/benches/
git commit -m "bench: add criterion benchmarks for serialization, compaction, and loop latency"
```

---

### Task 6: Fuzz targets

**Files:**
- Create: `neuron-provider-openai/fuzz/Cargo.toml`
- Create: `neuron-provider-openai/fuzz/fuzz_targets/parse_response.rs`
- Create: `neuron-provider-anthropic/fuzz/Cargo.toml`
- Create: `neuron-provider-anthropic/fuzz/fuzz_targets/parse_response.rs`
- Create: `neuron-provider-ollama/fuzz/Cargo.toml`
- Create: `neuron-provider-ollama/fuzz/fuzz_targets/parse_response.rs`

**Step 1: Create fuzz targets for each provider**

Each provider's `fuzz/Cargo.toml` follows this pattern (substitute crate name):

```toml
[package]
name = "{crate-name}-fuzz"
version = "0.0.0"
publish = false
edition = "2024"

[package.metadata]
cargo-fuzz = true

[dependencies]
libfuzzer-sys = "0.4"
serde_json = "1"
{crate-name} = { path = ".." }
neuron-types = { path = "../../neuron-types" }

[[bin]]
name = "parse_response"
path = "fuzz_targets/parse_response.rs"
test = false
doc = false
```

Each provider's `fuzz/fuzz_targets/parse_response.rs`:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(json_str) = std::str::from_utf8(data) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
            // Just verify it doesn't panic — we don't care about the result
            let _ = {crate}::mapping::from_api_response(&value);
        }
    }
});
```

Where `{crate}` is `neuron_provider_openai`, `neuron_provider_anthropic`, or
`neuron_provider_ollama`.

Note: `mapping::from_api_response` must be `pub` (not `pub(crate)`) for the
fuzz target to access it. Check visibility and adjust if needed.

**Step 2: Verify fuzz targets compile**

```bash
cd neuron-provider-openai && cargo fuzz check parse_response && cd ..
cd neuron-provider-anthropic && cargo fuzz check parse_response && cd ..
cd neuron-provider-ollama && cargo fuzz check parse_response && cd ..
```

If `cargo fuzz` is not installed: `cargo install cargo-fuzz`

**Step 3: Add fuzz directories to workspace exclude**

In root `Cargo.toml`, add after `members`:

```toml
exclude = [
    "neuron-provider-openai/fuzz",
    "neuron-provider-anthropic/fuzz",
    "neuron-provider-ollama/fuzz",
]
```

**Step 4: Commit**

```bash
git add neuron-provider-openai/fuzz/ neuron-provider-anthropic/fuzz/ \
  neuron-provider-ollama/fuzz/ Cargo.toml
git commit -m "test: add cargo-fuzz targets for provider response parsers"
```

---

## Phase 2: Content

### Task 7: mdBook docs site structure

**Files:**
- Create: `docs/book/book.toml`
- Create: `docs/book/src/SUMMARY.md`
- Create: `docs/book/src/introduction.md`
- Create: `.github/workflows/docs.yml`

**Step 1: Create book.toml**

```toml
[book]
title = "neuron — Building Blocks for AI Agents in Rust"
authors = ["SecBear"]
language = "en"
src = "src"

[build]
build-dir = "../../target/book"

[output.html]
git-repository-url = "https://github.com/secbear/neuron"
edit-url-template = "https://github.com/secbear/neuron/edit/main/docs/book/{path}"
additional-css = []
```

**Step 2: Create SUMMARY.md**

```markdown
# Summary

[Introduction](introduction.md)

# Getting Started

- [Installation](getting-started/installation.md)
- [Quickstart](getting-started/quickstart.md)
- [Core Concepts](getting-started/concepts.md)

# Guides

- [Tools](guides/tools.md)
- [Context Management](guides/context.md)
- [Providers](guides/providers.md)
- [The Agent Loop](guides/loop.md)
- [MCP Integration](guides/mcp.md)
- [Runtime](guides/runtime.md)
- [Embeddings](guides/embeddings.md)
- [Testing Agents](guides/testing.md)

# Architecture

- [Design Decisions](architecture/design-decisions.md)
- [Dependency Graph](architecture/dependency-graph.md)
- [Comparison](architecture/comparison.md)

# Reference

- [Error Handling](reference/error-handling.md)
```

**Step 3: Create introduction.md**

Write the introduction page covering: what neuron is, the "serde not serde_json"
philosophy, who it's for, and what makes it different. Pull from the existing
README and CLAUDE.md — condense for an external audience.

**Step 4: Create GitHub Actions workflow for docs deployment**

`.github/workflows/docs.yml`:

```yaml
name: Deploy Docs

on:
  push:
    branches: [main]
    paths:
      - 'docs/book/**'

permissions:
  pages: write
  id-token: write

jobs:
  deploy:
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install mdBook
        run: |
          curl -sSL https://github.com/rust-lang/mdBook/releases/latest/download/mdbook-v0.4.44-x86_64-unknown-linux-gnu.tar.gz | tar xz
          chmod +x mdbook
          sudo mv mdbook /usr/local/bin/
      - name: Build book
        run: mdbook build docs/book/
      - name: Setup Pages
        uses: actions/configure-pages@v4
      - name: Upload artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: target/book/
      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

**Step 5: Commit structure**

```bash
git add docs/book/ .github/workflows/docs.yml
git commit -m "docs: add mdBook structure and GitHub Pages deployment"
```

---

### Task 8: Write Getting Started pages

**Files:**
- Create: `docs/book/src/getting-started/installation.md`
- Create: `docs/book/src/getting-started/quickstart.md`
- Create: `docs/book/src/getting-started/concepts.md`

**Step 1: Write installation.md**

Cover: adding `neuron` to Cargo.toml, feature flags table (anthropic, openai,
ollama, mcp, runtime, full), individual crate usage pattern. Include actual
`cargo add` commands.

**Step 2: Write quickstart.md**

A complete working agent in ~50 lines:
- Set up an Anthropic or OpenAI provider with `from_env()`
- Define one tool with `#[neuron_tool]`
- Create `ToolRegistry`, register the tool
- Build `AgentLoop`, run with a user message
- Print the response

Must compile (test with `cargo build --example` or doc test).

**Step 3: Write concepts.md**

Cover the five core abstractions: Provider, Tool, ContextStrategy,
ObservabilityHook, DurableContext. One paragraph + code snippet each. Link to
the detailed guide for each.

**Step 4: Commit**

```bash
git add docs/book/src/getting-started/
git commit -m "docs: write Getting Started guide (installation, quickstart, concepts)"
```

---

### Task 9: Write Guide pages

**Files:**
- Create: `docs/book/src/guides/tools.md`
- Create: `docs/book/src/guides/context.md`
- Create: `docs/book/src/guides/providers.md`
- Create: `docs/book/src/guides/loop.md`
- Create: `docs/book/src/guides/mcp.md`
- Create: `docs/book/src/guides/runtime.md`
- Create: `docs/book/src/guides/embeddings.md`
- Create: `docs/book/src/guides/testing.md`

Each guide follows the same structure:
1. What it is (1-2 sentences)
2. Quick example (10-20 lines of code)
3. API walkthrough (key types, methods, configuration)
4. Advanced usage (if applicable)
5. Link to API docs on docs.rs

**Step 1-8:** Write each guide page. Pull content from existing crate READMEs
and doc comments — don't invent, condense and organize.

Largest guides: tools.md (covers `#[neuron_tool]`, `ToolRegistry`, middleware
pipeline), loop.md (covers `AgentLoop`, streaming, cancellation, parallel
tools), runtime.md (covers sessions, guardrails, GuardrailHook, TracingHook,
DurableContext).

Smallest guides: embeddings.md, testing.md.

**Step 9: Commit**

```bash
git add docs/book/src/guides/
git commit -m "docs: write guide pages (tools, context, providers, loop, mcp, runtime, embeddings, testing)"
```

---

### Task 10: Write Architecture and Reference pages

**Files:**
- Create: `docs/book/src/architecture/design-decisions.md`
- Create: `docs/book/src/architecture/dependency-graph.md`
- Create: `docs/book/src/architecture/comparison.md`
- Create: `docs/book/src/reference/error-handling.md`

**Step 1: Write design-decisions.md**

Condense CLAUDE.md "Validated design decisions" and "Decision framework" for an
external audience. Remove internal-only details. Keep the reasoning.

**Step 2: Write dependency-graph.md**

The ASCII dependency graph from CLAUDE.md + explanation of the "arrows only
point up" rule and what it means for users (you can use any block without
pulling the whole stack).

**Step 3: Write comparison.md**

Adapt from `llms.txt` feature matrix and `docs/competitive-analysis.md`.
Position honestly — what neuron does better, what it doesn't do.

**Step 4: Write error-handling.md**

All error enums (`ProviderError`, `ToolError`, `LoopError`, `EmbeddingError`,
etc.), the `is_retryable()` pattern, `ToolError::ModelRetry`, and how errors
flow through the loop.

**Step 5: Commit**

```bash
git add docs/book/src/architecture/ docs/book/src/reference/
git commit -m "docs: write architecture and reference pages"
```

---

### Task 11: Fill example gaps

**Files:**
- Create: `neuron-tool-macros/examples/derive_tool.rs`
- Create: `neuron-provider-openai/examples/embeddings.rs`
- Create: `neuron-runtime/examples/full_production.rs`
- Create: `neuron-loop/examples/cancellation.rs`
- Create: `neuron-loop/examples/parallel_tools.rs`
- Create: `neuron/examples/testing_agents.rs`

**Step 1-6:** Write each example. Every example must:
- Have a doc comment header explaining what it demonstrates
- Include a `// Run: cargo run --example {name}` comment
- Compile (use `no_run` patterns if it needs API keys)
- Be self-contained (no imports from test utilities)

**Step 7: Verify all examples compile**

```bash
cargo build --workspace --examples
```

**Step 8: Commit**

```bash
git add neuron-tool-macros/examples/ neuron-provider-openai/examples/ \
  neuron-runtime/examples/ neuron-loop/examples/ neuron/examples/
git commit -m "examples: add derive_tool, embeddings, full_production, cancellation, parallel_tools, testing_agents"
```

---

### Task 12: AI discoverability updates

**Files:**
- Modify: `llms.txt`
- Modify: `Cargo.toml` (homepage field for all crates)

**Step 1: Update llms.txt**

Add docs site URL to the Links section. Update example links if any paths
changed. Ensure all 25 examples are listed.

**Step 2: Update homepage in workspace Cargo.toml**

Change `homepage` from GitHub repo URL to docs site URL:

```toml
homepage = "https://secbear.github.io/neuron"
```

**Step 3: Verify Context7 submission**

Check if neuron is already listed at context7.com. If not, submit `llms.txt`.

**Step 4: Commit**

```bash
git add llms.txt Cargo.toml
git commit -m "docs: update llms.txt and homepage URL for docs site"
```

---

## Phase 3: Publish & Launch

### Task 13: Version bump and publish

**Step 1: Bump all versions from 0.1.0 to 0.2.0**

Update version in root `Cargo.toml` workspace dependencies and in each crate's
`Cargo.toml`. All 11 crates + all workspace dependency references.

**Step 2: Update CHANGELOGs**

Add a `## 0.2.0` section to each crate's `CHANGELOG.md` summarizing what's new
since 0.1.0.

**Step 3: Commit version bump**

```bash
git add -A
git commit -m "chore: bump all crates to 0.2.0"
```

**Step 4: Publish in dependency order**

```bash
cargo publish -p neuron-types
# Wait ~30s for crates.io to index
cargo publish -p neuron-tool-macros
cargo publish -p neuron-tool
cargo publish -p neuron-context
cargo publish -p neuron-provider-anthropic
cargo publish -p neuron-provider-openai
cargo publish -p neuron-provider-ollama
cargo publish -p neuron-mcp
cargo publish -p neuron-loop
cargo publish -p neuron-runtime
cargo publish -p neuron
```

**Step 5: Verify all 11 crates on crates.io and docs.rs**

---

### Task 14: Post-publish verification and launch

**Step 1: Verify from clean project**

```bash
cargo new /tmp/test-neuron && cd /tmp/test-neuron
cargo add neuron --features full
cargo build
```

**Step 2: Verify badges resolve**

Check README.md on GitHub — all 6 badges should render correctly.

**Step 3: Write and post r/rust announcement**

Title: "neuron — composable building blocks for AI agents in Rust"

**Step 4: Write and post Show HN**

Title: "Show HN: Neuron — Independent Rust crates for building AI agents"
Link to docs site.

**Step 5: Submit to awesome-rust**

PR to https://github.com/rust-unofficial/awesome-rust adding neuron under
the "Artificial Intelligence" section.

**Step 6: Verify Context7 listing**

Check context7.com for neuron. Submit if not present.

---

## Verification

After all tasks:

```bash
cargo build --workspace --examples
cargo test --workspace
cargo clippy --workspace --all-features
cargo doc --workspace --no-deps
cargo bench --workspace
mdbook build docs/book/
```

All must pass. All 6 badges must render on GitHub.
