# neuron

## Philosophy

Building blocks, not a framework. Each block is an independent Rust crate with
its own repo, versioned and published separately. Anyone can pull one block
without buying the whole stack.

We are building the foundation layer that agent frameworks should be built on.
Everyone else jumped straight to the framework. We're filling the gap underneath.

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

### What we are NOT building

- A CLI, TUI, or GUI (compose those from blocks)
- An opinionated agent framework (compose that from blocks)
- Embedding/RAG infrastructure (that's a tool or context strategy)
- A workflow engine (Temporal integration is an adapter)
- A graph/DAG orchestrator (not our concern)

---

## Landscape: why this doesn't exist yet

We validated against every Rust and Python agent framework. Nobody ships truly
independent composable building blocks. Here is why each alternative falls
short for our goals.

### Rig (280K downloads) — leading Rust LLM library

Monolithic `rig-core` with satellite vector stores. 18 providers compiled into
one crate. Well-designed traits (`CompletionModel`, `PromptHook`, multimodal
`Message` types) worth studying — but no decomposition, no durability, no
context management, no tool middleware, no permissions. You can't use the types
without pulling everything.

**What we take from Rig:** Native async traits (Rust 2024), `ToolDyn` type
erasure pattern, `IntoFuture` builder ergonomics, `WasmCompatSend/Sync` for
zero-cost WASM insurance, hook control flow (Continue/Skip/Terminate),
`schemars` for JSON Schema derivation.

**What we leave:** `Clone` bound on model trait, `OneOrMany<T>` (500 lines of
boilerplate for a non-empty guarantee), `ToolServer` background task with mpsc
channels (over-engineered), variant-per-role message types (~300 lines of
conversion per provider), definition-time prompt parameter on tools.

### ADK-Rust (~1K downloads) — closest to our architecture

25 crates, Rust 2024 edition. Separate crates for agent, model, tool, runner,
graph, session, telemetry, server, CLI, eval, browser, UI. The most decomposed
Rust agent framework.

**Why it's not the answer:**

| Concern | ADK-Rust | neuron |
|---------|---------|-------------------|
| **Provider independence** | All 14+ providers in one `adk-model` crate behind feature flags. Version conflict in one blocks all. | One crate per provider, independent versioning |
| **Core bloat** | `adk-core` has 11 files: Agent, Llm, Tool, Session, State, Memory, Artifacts, Events, 6 callback types, context hierarchy. Any change cascades everywhere. | `neuron-types` is lean: messages, Provider, Tool, errors. Logic lives in owning crates. |
| **Coupling** | `adk-agent` hard-depends on `adk-model` (implementation, not just trait). `adk-server` depends on 7 crates. Independence is partial. | Arrows only point up. Each block depends on `neuron-types` and the blocks directly below it. |
| **Durability** | None. No crash recovery, no Temporal mapping, no replay. | `DurableContext` wraps side effects. Implementations for Temporal, Restate, local passthrough. |
| **Context management** | `EventsCompactionConfig` stub. No token counting, no strategies. | First-class crate: 4 compaction strategies, token estimation, persistent context, system injection. |
| **Tool middleware** | Direct call with `Box<dyn Fn>` callbacks. | Composable middleware chain (validate, permissions, hooks, format). Same pattern as axum's `from_fn`. |
| **MCP** | Partial (tools only via rmcp). | Full spec: wraps rmcp, Streamable HTTP, lifecycle, sampling, resources, prompts. |

**What we take from ADK-Rust:** Telemetry as a cross-cutting concern available
at every layer (not buried in runtime). Umbrella crate with feature flags for
DX (build last). Separation of agent definition from agent execution as a
concept.

### Other Rust frameworks

| Framework | Downloads | Architecture | Why not |
|-----------|-----------|--------------|---------|
| **langchain-rust** | 129K | Single crate, feature flags | Monolithic, no agent loop abstraction |
| **genai** | 122K | Single crate, multi-provider | Provider abstraction only, no agent concerns |
| **Floxide** | ~10K | Composable workflow nodes | Workflow engine, not agent-specific. Closest pattern for composability. |
| **llm-chain** | 82K | Multi-crate (core + providers) | Dead project, unmaintained |
| **AutoAgents** | ~5K | Core + protocol + LLM + tools | Small adoption, all in one workspace |

### Python frameworks (what we're porting the patterns from)

| Framework | What we take | What we leave |
|-----------|-------------|---------------|
| **Claude Code** | Loop pattern, context compaction at ~92-95% capacity, tool middleware pipeline, sub-agent isolation, system reminder injection | TypeScript monolith, opaque internals |
| **Pydantic AI** | `Agent[Deps, Output]` generics, `ModelRetry`, step-by-step `iter()` | `pydantic-graph` FSM (over-engineered for most agents) |
| **OpenAI Agents SDK** | Handoff protocol, guardrails with tripwires, 3-tier streaming events | Single-package, Python-only |
| **OpenHands** | Observe-Think-Act cycle, pluggable condensers, event stream architecture | Research-oriented, heavy runtime |

### Full feature matrix

What exists vs what we're building:

| Capability | Rig | ADK-Rust | Us |
|-----------|-----|---------|-----|
| True crate independence | No | Partial (coupling leaks) | **Yes** |
| Provider-per-crate | No (all in core) | No (one crate, features) | **Yes** |
| Tool middleware pipeline | No | No | **Yes** |
| Context compaction strategies | No | Stub | **Yes** |
| Durable execution (`DurableContext`) | No | No | **Yes** |
| MCP full spec (via rmcp) | Thin wrapper | Partial (tools only) | **Yes** |
| Thinking/reasoning support | Partial | Unclear | **Yes** |
| Structured output (JSON Schema) | Via schemars | Unclear | **Yes** |
| Prompt caching (`CacheControl`) | No | No | **Yes** |
| Declarative permissions | No | Scope-based | **Yes** |
| Native async (Rust 2024) | Yes | Yes | **Yes** |
| WASM compatibility | Yes | No | **Yes** |
| Guardrails | No | Yes | Yes (in runtime) |
| Graph/DAG workflows | Op trait | Yes (LangGraph port) | No (not our concern) |
| Server/deployment | No | Yes | Later (optional) |
| Umbrella crate | No | Yes | Last (after blocks stabilize) |

---

## Validated design decisions (Wave 1 research)

These decisions were validated against real API docs, source code, and specs.

### Confirmed — keep as designed

- **Block decomposition** into independent crates per concern
- **Provider-per-crate** following the serde pattern
- **`ToolMiddleware` with `next` callback** — identical to axum's `from_fn`,
  validated by the tokio team. Do NOT adopt tower's `Service`/`Layer`.
- **`Message { role, content: Vec<ContentBlock> }`** — Rig's variant-per-role
  creates ~300 lines of conversion per provider. Flat is better.
- **Direct tool execution with middleware** — Rig's background task + mpsc
  channels is over-engineered for tool call frequency
- **No graph/DAG layer** — ADK-Rust's `adk-graph` is a LangGraph port, niche
- **Single `Provider` trait** works as lossy abstraction over both OpenAI Chat
  Completions and Responses API

### Changed after validation

- **`DurabilityHook` → `DurableContext` + `ObservabilityHook`.**
  Observation-only hooks cannot participate in Temporal replay. Durable
  execution requires wrapping side effects, not observing them. Every engine
  (Temporal, Restate, Inngest) needs the same thing: `ctx.execute(...)`.
- **`neuron-mcp` wraps rmcp** (official Rust MCP SDK, 3.8M downloads). Do not
  reimplement the protocol. Our `connect_sse` targeted a deprecated transport;
  Streamable HTTP is the current spec.
- **Rust 2024 edition, native async traits.** Drop `#[async_trait]`. Use
  `-> impl Future<Output = T> + Send`. Add `WasmCompatSend`/`WasmCompatSync`.
- **`HookAction` gains `Skip` variant** (Continue/Skip/Terminate) from Rig.
  Skip returns rejection as tool result so the model can adapt.

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
        neuron-runtime          (sub-agents, sessions, DurableContext, guardrails)
            ^
        neuron                  (umbrella re-export, build LAST)
            ^
        YOUR PROJECTS           (sdk, cli, tui, gui, gh-aw)
```

Arrows only point up. No circular dependencies. Each block knows only about
`neuron-types` and the blocks directly below it.

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
