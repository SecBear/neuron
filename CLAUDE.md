# neuron

## Philosophy

Building blocks, not a framework. Each block is an independent Rust crate,
versioned and published separately. Anyone can pull one block without buying the
whole stack.

We studied every Rust and Python agent framework (Rig, ADK-Rust, genai, Claude
Code, Pydantic AI, OpenAI Agents SDK, OpenHands). They all converge on the same
~300-line while loop. The differentiation is never the loop — it's the blocks
around it: context management, tool pipelines, durability, runtime. Nobody ships
those blocks independently. That's the gap we fill.

### Core beliefs

1. **The agentic loop is commodity.** Every framework converges on the same
   ~300-line while loop. The value is in the blocks around it: context
   management, tool pipelines, durability, runtime.

2. **Composition over integration.** Blocks compose through traits defined in
   `neuron-types`. No block knows about any other block — only about the traits
   it depends on.

3. **Traits are the API.** `Provider`, `Tool`, `ContextStrategy`,
   `DurableContext` — these are the public surface. Implementations are
   satellites. Follow the serde pattern: trait in core, impls in their own
   crates.

4. **Provider-agnostic from the foundation.** The `Provider` trait lives in the
   types crate. Cloud API, local llama, mock for testing — same trait.

5. **Durability is wrapping, not observing.** `DurableContext` wraps side
   effects (LLM calls, tool execution) so durable engines (Temporal, Restate,
   Inngest) can journal, replay, and recover. Separate `ObservabilityHook` for
   logging/metrics/telemetry.

6. **Built for agents to work on.** Flat files. Obvious names. No macro magic
   hiding control flow. Every public item has doc comments. One concept per
   file.

7. **Invalid states are unrepresentable.** Use Rust's type system to prevent
   misuse at compile time, not runtime checks.

### What we are NOT building (and why)

- **CLI, TUI, or GUI** — compose those from blocks; neuron is the layer below
- **Opinionated agent framework** — that's the SDK layer built on top
- **Embedding/RAG pipeline** — that's a tool or context strategy impl, not a block
- **Workflow engine** — Temporal/Restate integration is an adapter, not a core concern
- **Graph/DAG orchestrator** — niche; ADK-Rust ported LangGraph and it's their least-used crate
- **Retry/resilience middleware** — tower exists for this; durable engines handle it at the journaling layer. neuron exposes `ProviderError::is_retryable()` for callers.
- **Sub-agent orchestration registry** — compose sub-agents directly from `AgentLoop` + `ToolRegistry`. A registry is ~50 lines of composition = SDK territory.

---

### Scope boundary: neuron vs SDK

neuron is **serde, not serde_json**. serde defines Serialize/Deserialize traits.
serde_json implements them. neuron defines Provider/Tool/ContextStrategy traits
and provides foundational implementations. An SDK layer composes them into
opinionated workflows.

**The scope test:** If removing it forces every user to reimplement 200+ lines
of non-trivial code (type erasure, middleware chaining, protocol handling), it
belongs in neuron. If removing it forces users to write 20-50 lines of
straightforward composition, it belongs in the SDK layer above.

This test was derived by evaluating `SubAgentManager`: removing it requires ~50
lines of `AgentLoop` + `ToolRegistry` composition = SDK. Contrast: removing
`ToolRegistry` would force 200+ lines of type erasure + middleware chaining =
building block.

| Belongs in neuron (building blocks) | Belongs in SDK layer (framework) |
|--------------------------------------|--------------------------------------|
| Trait definitions (Provider, Tool, ContextStrategy, etc.) | `Agent<Deps, Output>` typed generics |
| Reference implementations (1-2 per trait) | Handoff protocol between agents |
| The commodity ~300-line agent loop | Agent lifecycle management (registry, naming) |
| Sessions, guardrails, durability traits | Retry/resilience (use tower or durable engine) |
| Tool registry + middleware | Config-driven provider routing |
| Context compaction strategies | Parallel guardrail orchestration |
| MCP integration | `StopAtTools` declarative loop termination |
| Provider crates (one per provider) | `Agent.override()` testing DX |
| `from_env()`, `Message::user()` conveniences | Sub-agent orchestration registry |

When evaluating a new feature, apply this test before adding it to any crate.

---

## Validated design decisions

These were validated against real API docs, source code, and specs. The reasoning
is recorded here so future changes don't accidentally undo validated work.

### Confirmed — keep as designed

- **Block decomposition** into independent crates per concern
- **Provider-per-crate** following the serde pattern
- **`ToolMiddleware` with `next` callback** — identical to axum's `from_fn`,
  validated by the tokio team. Do NOT adopt tower's `Service`/`Layer`.
- **`Message { role, content: Vec<ContentBlock> }`** — Rig uses variant-per-role
  which creates ~300 lines of conversion per provider. Flat struct is better.
- **Direct tool execution with middleware** — Rig's background task + mpsc
  channels is over-engineered for tool call frequency
- **No graph/DAG layer** — niche; most agent use cases don't need it
- **Single `Provider` trait** works as lossy abstraction over both OpenAI Chat
  Completions and Responses API

### Changed after validation

- **`DurabilityHook` → `DurableContext` + `ObservabilityHook`.** Observation-only
  hooks cannot participate in Temporal replay. Durable execution requires
  wrapping side effects, not observing them.
- **`neuron-mcp` wraps rmcp** (official Rust MCP SDK). Do not reimplement the
  protocol. Streamable HTTP is the current spec.
- **Rust 2024 edition, native async traits.** Drop `#[async_trait]`. Use
  `-> impl Future<Output = T> + Send`. Add `WasmCompatSend`/`WasmCompatSync`.
- **`HookAction` gains `Skip` variant** (Continue/Skip/Terminate). Skip returns
  rejection as tool result so the model can adapt.
- **Server-side context management** via `ContextManagement` request field,
  `ContentBlock::Compaction`, and `StopReason::Compaction`. The loop continues
  automatically when the server compacts context.
- **`ToolError::ModelRetry`** for self-correction (from Pydantic AI). Tools
  return a hint string converted to an error tool result for retry with guidance.
- **`SubAgentManager` removed.** Sub-agent orchestration is SDK-layer composition.
  Compose directly from `AgentLoop` + `ToolRegistry` + `LoopConfig`.

---

## Decision framework

When making design decisions, apply these filters in order:

1. **Does Rust's type system enforce it?** If a constraint can be a compile
   error, make it a compile error. Don't write runtime checks for things the
   compiler can catch.

2. **Can it be a trait?** If a capability varies across implementations
   (providers, storage backends, compaction strategies), define a trait in
   `neuron-types` and let satellites implement it.

3. **Does it belong in `neuron-types`?** Types and traits go in `neuron-types`.
   Logic goes in the block that owns the concern. If you're adding logic to
   `neuron-types`, stop and reconsider.

4. **One block, one concern.** If a change touches two blocks, check whether
   you're introducing coupling. The fix is usually a new trait in `neuron-types`.

5. **Study the prior art first.** Before designing an abstraction, check how
   Rig, ADK-Rust, Claude Code, Pydantic AI, and OpenAI Agents SDK handle it.
   Adopt what works. Don't reinvent for the sake of it.

6. **YAGNI ruthlessly.** Don't add features, configuration, or abstractions
   until a real composition demands them. Three lines of duplicated code is
   better than a premature abstraction.

---

## Block dependency graph

```
neuron-types                    (zero deps, the foundation)
    ^
    |-- neuron-provider-*       (each implements Provider trait)
    |-- neuron-tool             (implements Tool trait, registry, middleware)
    |-- neuron-mcp              (wraps rmcp, bridges to Tool trait)
    +-- neuron-context          (+ optional Provider for summarization)
            ^
        neuron-loop             (composes provider + tool + context)
            ^
        neuron-runtime          (sessions, DurableContext, guardrails, sandbox)
            ^
        neuron                  (umbrella re-export, build LAST)
            ^
        YOUR PROJECTS           (sdk, cli, tui, gui)
```

Arrows only point up. No circular dependencies. Each block knows only about
`neuron-types` and the blocks directly below it.

**This graph must match actual Cargo.toml dependencies.** When adding or removing
a dependency between crates, update this graph in the same commit.

---

## Rust conventions

- **Edition 2024**, resolver 3, minimum Rust 1.90
- **Native async in traits** — `-> impl Future<Output = T> + WasmCompatSend`
- **No `#[async_trait]`** — obsolete as of Rust 2024
- **`schemars`** for JSON Schema derivation on tool inputs and structured output
- **`thiserror`** for all error types, 2 levels max (no deep nesting)
- **`ToolDyn`** type erasure for tool registry (strongly typed impl, erased storage)
- **`IntoFuture`** on builders so `.await` sends the request

## Per-block conventions

Every block follows the same layout:

```
neuron-{block}/
    CLAUDE.md              # Agent instructions for this crate
    Cargo.toml
    src/
        lib.rs             # Public API, re-exports, module docs
        types.rs           # All types in one place
        traits.rs          # All traits in one place
        {feature}.rs       # One file per feature, not nested dirs
        error.rs           # Error types
    tests/
        integration.rs
    examples/
        basic.rs
```

Rules:
- Flat file structure, no deep nesting
- One concept per file, named obviously
- All public types re-exported from `lib.rs`
- Inline doc comments on every public item
- Every trait has a doc example
- Error types are enums with descriptive variants
- No `unwrap()` in library code
- `#[must_use]` on Result-returning functions

## Documentation completeness checklist

Every change that adds, removes, or modifies public API **must** update all
documentation surfaces before the change is considered complete. Do not wait
for a separate pass — treat this as part of the implementation, not a follow-up.

For **every public type, trait, variant, or field** added or changed:

1. **Source doc comments** — `///` on every public item. Include a usage example
   for new traits.
2. **Crate `CLAUDE.md`** — update the "Key types" / "Key design decisions"
   sections in the affected crate's `CLAUDE.md`.
3. **Crate `README.md`** — update type listings, feature bullets, and code
   examples in the affected crate's `README.md`.
4. **Examples** — if a new feature or pattern is added, add or update an example
   in the crate's `examples/` directory showing real usage.
5. **`ROADMAP.md`** — if the feature was listed under "Next" or "Later", move it
   to "Now" once shipped.
6. **Root `CLAUDE.md`** — if the change affects the dependency graph, scope
   boundary table, or "Validated design decisions" section, update it.
7. **`llms.txt`** — if the change adds a crate, trait, or example, update
   `llms.txt` (the external-facing discovery doc for LLMs and humans evaluating
   neuron).
8. **Struct literal propagation** — when adding a field to a type, search the
   entire workspace for explicit struct literals of that type
   (`cargo build --workspace --examples && cargo test --workspace`). Fix all
   sites, including tests, examples, doc tests, and READMEs.

If you're unsure whether a surface needs updating, check it. The cost of
reading a file is near zero; the cost of stale docs is confusion.

---

## Doc-code coupling rules

Documentation and code must stay in sync. Every code change that affects the
public surface must include corresponding doc updates in the same commit.

### What counts as a doc-affecting code change

- Adding, removing, or renaming a public type, trait, function, or method
- Changing a function signature (parameters, return type, generics)
- Adding or removing a feature flag
- Adding or removing a dependency between crates
- Changing a crate's description or keywords in `Cargo.toml`
- Adding, removing, or renaming an example

### Required updates by change type

| Code change | Update these docs |
|-------------|-------------------|
| New public type/trait/function | Doc comment; crate `lib.rs` re-exports; crate README; crate CLAUDE.md; relevant mdBook guide page in `docs/book/src/` |
| Renamed or removed public item | All references in README, examples, doc comments, llms.txt, mdBook pages |
| New feature flag | Crate Cargo.toml; crate README; `neuron/README.md` feature table; `docs/book/src/getting-started/installation.md` |
| New example file | `Cargo.toml` `[[example]]` if needed; `neuron/README.md` Learning Path; llms.txt; relevant mdBook guide page |
| New or removed crate dependency | Root CLAUDE.md dependency graph; crate CLAUDE.md; `docs/book/src/architecture/dependency-graph.md` |
| New provider or major feature | Root README feature matrix; `neuron/README.md`; llms.txt; `docs/book/src/architecture/comparison.md` |
| Changed trait signature | All examples using that trait; trait doc example; README snippets; relevant mdBook guide page |
| New error type or variant | `docs/book/src/reference/error-handling.md`; crate CLAUDE.md |
| New benchmark | Ensure `cargo bench` still passes; update `docs/book/src/architecture/comparison.md` if performance claims change |

### Key files that must stay in sync

| File | Coupled to | Update when |
|------|-----------|-------------|
| Root `CLAUDE.md` dependency graph | `Cargo.toml` `[dependencies]` in each crate | Adding/removing inter-crate deps |
| Root `CLAUDE.md` scope boundary table | Actual crate contents | Moving features in/out of neuron |
| `ROADMAP.md` "Now" section | Shipped code | Shipping a feature listed under Next/Later |
| `llms.txt` crate list + trait list | Actual public API | Adding/removing crates or traits |
| `llms.txt` examples list | `examples/` directories in all crates | Adding/removing/renaming examples |
| `neuron/README.md` feature flags table | `neuron/Cargo.toml` `[features]` | Changing feature flags |
| `docs/book/src/` guide pages | Source code in corresponding crate | Changing public API, adding features |
| `docs/book/src/SUMMARY.md` | `docs/book/src/` directory structure | Adding/removing/renaming pages |
| `docs/book/src/reference/error-handling.md` | Error enums in `neuron-types/src/error.rs` | Adding/removing error types or variants |
| `docs/book/src/architecture/comparison.md` | Feature matrix reality | Adding features, new benchmarks |

### Verification checklist

Before completing any PR or commit that touches public API or docs:

1. `cargo doc --workspace --no-deps` — zero warnings
2. `cargo build --workspace --examples` — all examples compile
3. `cargo test --workspace` — doc tests pass (they test README snippets)
4. `mdbook build docs/book/` — docs site builds without errors
5. Every example file has a doc-comment header with `cargo run` command
6. Every public item has a doc comment
7. README code snippets compile (tested via doc tests or manual verification)
8. mdBook guide pages reflect current API (spot-check changed features)
