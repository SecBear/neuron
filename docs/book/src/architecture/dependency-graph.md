# Dependency Graph

neuron's crates form a strict upward-pointing dependency tree. Every arrow
points toward the foundation (`neuron-types`), never downward. There are no
circular dependencies.

## The graph

```text
neuron-types                    (zero deps, the foundation)
    ^
    |-- neuron-provider-*       (each implements Provider)
    |-- neuron-tool             (implements Tool, registry, middleware)
    |-- neuron-mcp              (wraps rmcp, bridges to Tool)
    +-- neuron-context          (+ optional Provider for summarization)
            ^
        neuron-loop             (composes provider + tool + context)
            ^
        neuron-runtime          (sessions, DurableContext, guardrails)
            ^
        neuron                  (umbrella re-export)
            ^
        YOUR PROJECT            (SDK, CLI, TUI, GUI)
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

### neuron-tool (leaf node)

Implements the tool system:

- `ToolRegistry` -- stores `Arc<dyn ToolDyn>` for dynamic dispatch
- Tool middleware pipeline (axum-style `from_fn`)
- Type erasure via the `ToolDyn` blanket impl

Depends only on `neuron-types`.

### neuron-mcp (leaf node)

Wraps the `rmcp` crate (the official Rust MCP SDK) and bridges MCP tools into
neuron's `ToolDyn` trait. Depends only on `neuron-types` and `rmcp`.

### neuron-context (leaf node)

Implements `ContextStrategy` for client-side context compaction. Some strategies
(like summarization) optionally use a `Provider` for LLM calls, but the
dependency is on the trait, not on any concrete provider crate.

### neuron-loop (composition layer)

The agentic while-loop that composes a provider, tool registry, and context
strategy. This is the ~300-line commodity loop that every agent framework
converges on. It depends on:

- `neuron-types` (for all trait definitions)

The loop is generic over `<P: Provider>` and accepts a `ToolRegistry` and
optional `ContextStrategy` implementation.

### neuron-runtime (runtime layer)

Adds cross-cutting runtime concerns on top of the loop:

- **Sessions** -- persistent conversation state via `StorageError`-aware backends
- **DurableContext** -- wraps side effects for Temporal/Restate replay
- **ObservabilityHook** -- logging, metrics, telemetry
- **Guardrails** -- input/output validation
- **PermissionPolicy** -- tool call authorization
- **Sandbox** -- isolated tool execution environments

Depends on `neuron-types` and `neuron-loop`.

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

**Each block knows only about `neuron-types` and the blocks directly below it.**
`neuron-tool` has no idea that `neuron-loop` exists. `neuron-provider-anthropic`
has no idea that `neuron-runtime` exists. This means you can use any block
independently.

**No cross-leaf dependencies.** Provider crates do not depend on the tool crate.
The MCP crate does not depend on any provider crate. Leaf nodes are fully
independent of each other.

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
