# Dependency Graph

neuron's crates form a strict upward-pointing dependency tree. Every arrow
points toward the foundation (`neuron-types`), never downward. There are no
circular dependencies.

## The graph

```text
neuron-types                    (zero deps, the foundation)
neuron-tool-macros              (zero deps, proc macro)
    ^
    |-- neuron-provider-*       (each implements Provider trait)
    |-- neuron-otel             (OTel instrumentation, GenAI semantic conventions)
    |-- neuron-context          (compaction strategies, token counting)
    +-- neuron-tool             (Tool trait, registry, middleware; optional dep on neuron-tool-macros)
            ^
            |-- neuron-mcp      (wraps rmcp, bridges to Tool trait)
            |-- neuron-loop     (provider loop with tool dispatch)
            +-- neuron-runtime  (sessions, DurableContext, guardrails, sandbox)
                    ^
                neuron          (umbrella re-export)
                    ^
                YOUR PROJECT    (SDK, CLI, TUI, GUI)
```

## Layer by layer

### neuron-types (foundation)

Zero dependencies on other neuron crates. Contains all types and trait
definitions:

- **Types:** `Message`, `CompletionRequest`, `CompletionResponse`, `TokenUsage`,
  `ToolDefinition`, `ToolOutput`, `ContentBlock`, `StopReason`
- **Traits:** `Provider`, `EmbeddingProvider`, `Tool`, `ToolDyn`,
  `ContextStrategy`, `ObservabilityHook`, `DurableContext`, `PermissionPolicy`
- **Errors:** `ProviderError`, `ToolError`, `LoopError`, `ContextError`,
  `DurableError`, `HookError`, `McpError`, `EmbeddingError`, `StorageError`,
  `SandboxError`

Every other crate depends on `neuron-types`. Nothing else.

### Provider crates (leaf nodes)

Each provider crate implements the `Provider` trait for one API:

| Crate | Provider |
|-------|----------|
| `neuron-provider-anthropic` | Anthropic Messages API |
| `neuron-provider-openai` | OpenAI Chat Completions / Responses API |
| `neuron-provider-ollama` | Ollama local inference |

Provider crates depend **only** on `neuron-types` (plus their HTTP client and
API-specific serialization). They never depend on each other or on higher-level
neuron crates.

Adding a new provider means creating a new crate that implements `Provider`.
No existing code changes.

### neuron-otel (leaf node)

Implements the `ObservabilityHook` trait using OpenTelemetry tracing spans with
`gen_ai.*` GenAI semantic conventions. Emits structured spans for LLM calls,
tool executions, and loop iterations following the emerging OpenTelemetry GenAI
semantic conventions specification.

Depends only on `neuron-types` (plus `tracing` and `opentelemetry` for span
emission). Like provider crates, it is a leaf node with no knowledge of other
neuron crates.

### neuron-tool-macros (leaf node)

Proc macro crate providing `#[neuron_tool]` for deriving `Tool` implementations
from annotated async functions. Zero workspace dependencies.

### neuron-tool (leaf node)

Implements the tool system:

- `ToolRegistry` -- stores `Arc<dyn ToolDyn>` for dynamic dispatch
- Tool middleware pipeline (axum-style `from_fn`)
- Type erasure via the `ToolDyn` blanket impl

Depends on `neuron-types` and optionally on `neuron-tool-macros` (via `macros`
feature flag).

### neuron-mcp

Wraps the `rmcp` crate (the official Rust MCP SDK) and bridges MCP tools into
neuron's `ToolDyn` trait. Depends on `neuron-types`, `neuron-tool`, and `rmcp`.

### neuron-context (leaf node)

Implements `ContextStrategy` for client-side context compaction. Some strategies
(like summarization) optionally use a `Provider` for LLM calls, but the
dependency is on the trait, not on any concrete provider crate.

### neuron-loop

The agentic while-loop that composes a provider and tool registry. This is the
~300-line commodity loop that every agent framework converges on. It depends on:

- `neuron-types` (for trait definitions)
- `neuron-tool` (for `ToolRegistry`)

The loop is generic over `<P: Provider, C: ContextStrategy>` and accepts a
`ToolRegistry`. `neuron-context` is a dev-dependency only (for tests).

### neuron-runtime

Adds cross-cutting runtime concerns:

- **Sessions** -- persistent conversation state via `StorageError`-aware backends
- **DurableContext** -- wraps side effects for Temporal/Restate replay
- **ObservabilityHook** -- logging, metrics, telemetry
- **Guardrails** -- input/output validation
- **PermissionPolicy** -- tool call authorization
- **Sandbox** -- isolated tool execution environments

Depends on `neuron-types` and `neuron-tool`. `neuron-loop` and `neuron-context`
are dev-dependencies only (for tests).

### neuron (umbrella)

Re-exports public items from all crates under a single `neuron` dependency.
Feature flags control which provider crates are included:

```toml
[dependencies]
neuron = { version = "0.2", features = ["anthropic", "openai"] }
```

## Design rules

**Arrows only point up.** A crate at layer N may depend on crates at layer N-1
or below, never at layer N or above. This is enforced by `Cargo.toml`
dependencies -- circular dependencies are a compile error in Rust.

**Each block knows only about `neuron-types` and the blocks it directly depends on.**
`neuron-tool` has no idea that `neuron-loop` exists. `neuron-provider-anthropic`
has no idea that `neuron-runtime` exists. This means you can use any block
independently.

**Provider crates are fully independent.** Provider crates do not depend on the
tool crate, the MCP crate, or each other. `neuron-mcp`, `neuron-loop`, and
`neuron-runtime` share a dependency on `neuron-tool` but are independent of
each other.

## Practical implications

**Using just the tool system:**

```toml
[dependencies]
neuron-types = "0.2"
neuron-tool = "0.2"
```

**Using just a provider for raw LLM calls:**

```toml
[dependencies]
neuron-types = "0.2"
neuron-provider-anthropic = "0.2"
```

**Using the full stack:**

```toml
[dependencies]
neuron = { version = "0.2", features = ["anthropic", "openai", "mcp"] }
```

The dependency graph ensures that pulling in one block never forces you to
compile unrelated blocks.
