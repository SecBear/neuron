# Path to 10/10: neuron Polish, Docs, and Launch

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Bring neuron from its current state (~6/10) to 10/10 across
documentation, discoverability, test robustness, and polish — then launch
publicly on r/rust and Hacker News.

**Strategy:** Infrastructure-First. Build trust signals first (CI, community
files, badges), then content (docs site, examples, advanced tests), then launch
(version bump, publish, announce). One big launch moment — everything lands
together.

**Audience:** Both human developers and AI agents equally.

**Constraints:**
- Crates already published at 0.1.0 on crates.io — publish 0.2.0 with all
  new features
- Context7 may already be submitted — verify before submitting
- mdBook on GitHub Pages for docs site
- Launch channels: r/rust + Hacker News

---

## Phase 1: Trust Signals

### 1a. CI Hardening

Expand `.github/workflows/ci.yml` with additional jobs (all on PR + push to
main):

| Job | Tool | Purpose |
|-----|------|---------|
| Coverage | `cargo-tarpaulin` + Codecov | Measure line coverage, upload report |
| Security audit | `cargo-audit` | Flag known vulnerabilities in deps |
| MSRV check | Rust 1.90 toolchain | Verify minimum supported version compiles |
| Dependency review | `cargo-deny` | License compliance + duplicate detection |
| Docs link check | `lychee` | Verify no broken links in markdown |

### 1b. Community Files

| File | Contents |
|------|----------|
| `CONTRIBUTING.md` | Fork/branch/PR workflow, coding conventions (reference CLAUDE.md), test expectations, Conventional Commits format |
| `CODE_OF_CONDUCT.md` | Contributor Covenant v2.1 |
| `SECURITY.md` | Vulnerability reporting process (email, not public issue), supported versions |
| `.github/ISSUE_TEMPLATE/bug_report.md` | Structured: crate, version, steps to reproduce, expected vs actual |
| `.github/ISSUE_TEMPLATE/feature_request.md` | Use case, proposed API, alternatives considered |
| `.github/pull_request_template.md` | Checklist: tests pass, docs updated, CHANGELOG entry |

### 1c. Badges

Update root `README.md` badge row to six badges:

- CI status (GitHub Actions)
- Coverage (Codecov)
- crates.io version
- docs.rs
- MSRV (Rust 1.90+)
- License

---

## Phase 1 continued: Test Infrastructure

### 1d. Property-Based Tests

Add `proptest` to dev-dependencies:

| Crate | Tests |
|-------|-------|
| `neuron-types` | Serde roundtrip for all public types (Message, CompletionRequest, CompletionResponse, EmbeddingRequest/Response, ContentBlock variants) — random valid inputs serialize then deserialize to equal value |
| `neuron-types` | `ProviderError::is_retryable()` and `EmbeddingError::is_retryable()` — generate random error variants, verify classification consistency |
| `neuron-context` | Token counting monotonicity — adding messages never decreases count; compaction reduces count |
| `neuron-tool` | Middleware chain ordering — N middlewares always execute in registration order |

### 1e. Benchmark Regression Tracking

`criterion` benchmarks in `benches/`:

| Crate | Benchmark |
|-------|-----------|
| `neuron-types` | Message and CompletionRequest serialization throughput |
| `neuron-context` | Sliding window compaction on 100/1000/10000 message conversations |
| `neuron-loop` | Single turn latency with mock provider (no network) |

CI job using `criterion-compare` or `bencher` to catch regressions on PRs
(warn, don't block).

### 1f. Fuzz Targets

`cargo-fuzz` targets for provider response parsers:

| Crate | Target |
|-------|--------|
| `neuron-provider-openai` | Fuzz `parse_response` with arbitrary JSON |
| `neuron-provider-anthropic` | Fuzz `parse_response` with arbitrary JSON |
| `neuron-provider-ollama` | Fuzz `parse_ndjson_line` with arbitrary bytes |

Not in CI (fuzz runs are long) — targets set up for local/nightly use only.

---

## Phase 2: Content

### 2a. mdBook on GitHub Pages

```
docs/book/
├── book.toml
└── src/
    ├── SUMMARY.md
    ├── introduction.md          — What neuron is, philosophy, "serde not serde_json"
    ├── getting-started/
    │   ├── installation.md      — Cargo.toml setup, feature flags
    │   ├── quickstart.md        — First agent in 50 lines
    │   └── concepts.md          — Provider, Tool, ContextStrategy, the loop
    ├── guides/
    │   ├── tools.md             — #[neuron_tool], middleware, ToolRegistry
    │   ├── context.md           — Compaction strategies, token counting, SystemInjector
    │   ├── providers.md         — Anthropic, OpenAI, Ollama, from_env(), switching
    │   ├── loop.md              — AgentLoop, streaming, cancellation, parallel tools
    │   ├── mcp.md               — McpClient, McpToolBridge, McpServer
    │   ├── runtime.md           — Sessions, guardrails, GuardrailHook, TracingHook, DurableContext
    │   ├── embeddings.md        — EmbeddingProvider, OpenAI implementation
    │   └── testing.md           — Mock providers, testing agents, wiremock patterns
    ├── architecture/
    │   ├── design-decisions.md  — Validated decisions (condensed for external audience)
    │   ├── dependency-graph.md  — Block diagram, why arrows only point up
    │   └── comparison.md        — Feature matrix vs Rig, ADK-Rust, Python frameworks
    └── reference/
        └── error-handling.md    — All error types, is_retryable(), ModelRetry
```

GitHub Actions deploys via `mdbook build` to `gh-pages` branch on push to main.

Homepage URL in Cargo.toml and GitHub repo updated to point to deployed site.

### 2b. Fill Example Gaps

| Crate | New example | What it shows |
|-------|------------|---------------|
| `neuron-tool-macros` | `derive_tool.rs` | `#[neuron_tool]` macro usage, custom input types |
| `neuron-provider-openai` | `embeddings.rs` | EmbeddingProvider usage with OpenAI |
| `neuron-runtime` | `full_production.rs` | Sessions + guardrails + GuardrailHook + TracingHook composed |
| `neuron-loop` | `cancellation.rs` | CancellationToken usage with timeout |
| `neuron-loop` | `parallel_tools.rs` | parallel_tool_execution flag demo |
| `neuron` | `testing_agents.rs` | Mock provider + mock tools for unit testing |

Total: 19 existing + 6 new = 25 examples.

### 2c. AI Discoverability

- Update `llms.txt` to link to docs site
- Verify Context7 submission status — submit if not already there
- Submit PR to `awesome-rust` under AI/ML section

---

## Phase 3: Publish & Launch

### 3a. Version Bump + Republish

Bump all 11 crates from 0.1.0 to 0.2.0 and publish in dependency order:

1. neuron-types
2. neuron-tool-macros
3. neuron-tool
4. neuron-context
5. neuron-provider-anthropic
6. neuron-provider-openai
7. neuron-provider-ollama
8. neuron-mcp
9. neuron-loop
10. neuron-runtime
11. neuron

Each `cargo publish` waits for the previous crate to be indexed.

### 3b. Post-Publish Verification

- All 11 crates visible on crates.io at 0.2.0
- docs.rs builds succeed for all 11
- `cargo add neuron` works from fresh project
- All badges resolve in README
- GitHub homepage URL points to docs site
- `llms.txt` links work

### 3c. Launch Posts

**r/rust:**
- Title: "neuron — composable building blocks for AI agents in Rust"
- Body: problem (monolithic frameworks), approach (independent crates),
  highlights (3 providers, tool middleware, context compaction, MCP, durability),
  links to docs site + repo
- Tone: technical, honest about v0.2 status, inviting feedback

**Hacker News:**
- "Show HN: Neuron — Independent Rust crates for building AI agents"
- Link to docs site (not GitHub)
- Be in comments to answer questions

### 3d. Directory Submissions

- `awesome-rust` PR under Artificial Intelligence
- Context7 `llms.txt` submission (verify first — may already exist)
- `are-we-learning-yet` if still active

### 3e. Post-Launch

- Monitor GitHub issues/discussions for first week
- Track crates.io downloads as baseline
- Write follow-up post if traction: "What I learned building neuron"

---

## Execution Order

```
Phase 1 (trust signals)     ~8-12 hours
  1a. CI hardening
  1b. Community files
  1c. Badges
  1d. Property-based tests
  1e. Benchmarks
  1f. Fuzz targets

Phase 2 (content)           ~10-14 hours
  2a. mdBook docs site
  2b. Fill example gaps
  2c. AI discoverability updates

Phase 3 (launch)            ~2-4 hours
  3a. Version bump + republish
  3b. Post-publish verification
  3c. Launch posts
  3d. Directory submissions
  3e. Post-launch monitoring
```

Phases are sequential. Within each phase, tasks can be parallelized where
they touch different files/crates.

---

## Success Criteria

A new developer or AI agent evaluating Rust agent SDKs should:

1. Find neuron through crates.io search, awesome-rust, or llms.txt
2. See green CI badges, coverage %, and MSRV on the README
3. Click through to a docs site with a quickstart that gets them running in
   5 minutes
4. Find 25 examples covering every major feature
5. See 400+ tests, property tests, benchmarks, and fuzz targets
6. Find CONTRIBUTING.md, issue templates, and a code of conduct
7. Rate neuron at 8-10/10 across all four dimensions
