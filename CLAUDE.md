# rust-agent-blocks

## Philosophy

Building blocks, not a framework. Each block is an independent Rust crate with
its own repo, versioned and published separately. Anyone can pull one block
without buying the whole stack.

### Core beliefs

1. **The agentic loop is commodity.** Every framework converges on the same
   ~300-line while loop. The value is in the blocks around it: context
   management, tool pipelines, durability, runtime.

2. **Composition over integration.** Blocks compose through traits defined in
   `agent-types`. No block knows about any other block — only about the traits
   it depends on.

3. **Traits are the API.** `Provider`, `Tool`, `ContextStrategy`,
   `DurabilityHook` — these are the public surface. Implementations are
   satellites. Follow the serde pattern: trait in core, impls in their own
   crates.

4. **Provider-agnostic from the foundation.** The `Provider` trait lives in the
   types crate. Cloud API, local llama, mock for testing — same trait.

5. **Durability-ready, not durability-dependent.** `DurabilityHook` lets
   Temporal (or anything) observe and checkpoint the loop without the loop
   knowing about Temporal.

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

---

## Landscape comparison

### Why not just use Rig?

Rig (`rig-core` 0.31.0, ~280K downloads) is the leading Rust LLM library. We
studied it deeply. Here's the gap:

| Concern | Rig | rust-agent-blocks |
|---------|-----|-------------------|
| **Structure** | Monolithic `rig-core` (all types, all providers, all features in one crate) + satellite vector stores | Independent crate per concern |
| **Providers** | 18 providers compiled into core, no way to exclude | One crate per provider, depend on only what you use |
| **Tool middleware** | None — hooks observe but can't transform | Full middleware chain (validate, permissions, pre/post hooks, format) |
| **Context management** | None — no compaction, no token counting, no sliding window | First-class: 4 strategies, token estimation, persistent context |
| **Durability** | None — process crash = state lost | `DurabilityHook` trait, Temporal-ready |
| **Permissions** | Manual (write all logic in hooks) | Declarative `PermissionPolicy` trait: Allow / Deny / Ask |
| **Guardrails** | None | Input + output guardrails with tripwire/warn semantics |
| **MCP** | Thin tool-calling wrapper (no lifecycle, no resources/prompts) | Full client + server, discovery, lifecycle management |
| **Composability** | Can't use types without pulling all of `rig-core` | Use exactly the blocks you need |

**What Rig does well that we should respect:** Its `CompletionModel` trait,
multimodal `Message` types, and `PromptHook` system are well-designed. Study
them. Don't reinvent where they got it right. Consider compatibility where
practical.

### Other Rust frameworks

| Framework | Downloads | Architecture | Gap |
|-----------|-----------|--------------|-----|
| **Rig** | 280K | Hub-and-spoke monolith | No decomposition, no durability, no context mgmt |
| **langchain-rust** | 129K | Single crate, feature flags | Monolithic, no agent loop abstraction |
| **genai** | 122K | Single crate, multi-provider | Provider abstraction only, no agent concerns |
| **ADK-Rust** | ~1K | Highly decomposed (agent, model, tool, runner, graph) | Closest to our pattern but tiny, all in one workspace |
| **Floxide** | ~10K | Composable workflow nodes | Workflow engine, not agent-specific |
| **llm-chain** | 82K | Multi-crate (core + providers) | Dead project, unmaintained |
| **AutoAgents** | ~5K | Core + protocol + LLM + tools | Small adoption, all in one workspace |

**Nobody ships truly independent composable blocks.** ADK-Rust decomposes well
but is a single workspace. Floxide is the closest pattern (independent node
crates) but targets workflows, not agents. The tower/tower-http ecosystem in
web services is the real spiritual predecessor.

### Python frameworks (what we're porting the patterns from)

| Framework | What we take | What we leave |
|-----------|-------------|---------------|
| **Claude Code** | The loop pattern, context compaction triggers, tool middleware pipeline, sub-agent isolation, system reminder injection | TypeScript monolith, opaque internals |
| **Pydantic AI** | `Agent[Deps, Output]` generics, `ModelRetry`, step-by-step `iter()` | `pydantic-graph` FSM (over-engineered for most agents) |
| **OpenAI Agents SDK** | Handoff protocol, guardrails with tripwires, 3-tier streaming events | Single-package, Python-only |
| **OpenHands** | Observe-Think-Act cycle, pluggable condensers, event stream architecture | Research-oriented, heavy runtime |

---

## Decision framework

When making design decisions, apply these filters in order:

1. **Does Rust's type system enforce it?** If a constraint can be a compile
   error, make it a compile error. Don't write runtime checks for things the
   compiler can catch.

2. **Can it be a trait?** If a capability varies across implementations (providers,
   storage backends, compaction strategies), define a trait in `agent-types` and
   let satellites implement it.

3. **Does it belong in `agent-types`?** Types and traits go in `agent-types`.
   Logic goes in the block that owns the concern. If you're adding logic to
   `agent-types`, stop and reconsider.

4. **One block, one concern.** If a change touches two blocks, check whether
   you're introducing coupling. The fix is usually a new trait in `agent-types`.

5. **Study the prior art first.** Before designing an abstraction, check how
   Rig, Claude Code, Pydantic AI, and OpenAI Agents SDK handle it. Adopt what
   works. Don't reinvent for the sake of it.

6. **YAGNI ruthlessly.** Don't add features, configuration, or abstractions
   until a real composition demands them. Three lines of duplicated code is
   better than a premature abstraction.

---

## Block dependency graph

```
agent-types                     (zero deps, the foundation)
    ^
    |-- agent-provider-*        (each implements Provider trait)
    |-- agent-tool              (implements Tool trait, registry, pipeline)
    |-- agent-mcp               (implements Tool trait via MCP protocol)
    +-- agent-context           (+ optional Provider for summarization)
            ^
        agent-loop              (composes provider + tool + context)
            ^
        agent-runtime           (sub-agents, sessions, durability)
            ^
        YOUR PROJECTS           (sdk, cli, tui, gui, gh-aw)
```

Arrows only point up. No circular dependencies. Each block knows only about
`agent-types` and the blocks directly below it.

---

## Per-block conventions

Every block follows the same layout:

```
agent-{block}/
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
