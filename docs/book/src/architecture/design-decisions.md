# Design Decisions

neuron's architecture reflects a set of deliberate trade-offs. This page
explains the key decisions and the reasoning behind them.

## "serde, not serde_json"

neuron is a library of building blocks, not a framework.

The `serde` crate defines the `Serialize` and `Deserialize` traits.
`serde_json` implements them for JSON. neuron follows the same pattern:
`neuron-types` defines the `Provider`, `Tool`, and `ContextStrategy` traits.
Provider crates (`neuron-provider-anthropic`, `neuron-provider-openai`, etc.)
implement them.

This means you can pull in a single block -- say, `neuron-tool` for the tool
registry and middleware pipeline -- without buying into an opinionated agent
framework. You compose the blocks yourself, or use a framework built on top.

**The scope test:** If removing a feature forces every user to reimplement 200+
lines of non-trivial code (type erasure, middleware chaining, protocol
handling), it belongs in neuron. If removing it forces 20-50 lines of
straightforward composition, it belongs in an SDK layer above.

## Block decomposition: one crate, one concern

Each crate owns exactly one concern:

| Crate | Concern |
|-------|---------|
| `neuron-types` | Types and trait definitions (zero logic) |
| `neuron-provider-anthropic` | Anthropic API implementation |
| `neuron-provider-openai` | OpenAI API implementation |
| `neuron-provider-ollama` | Ollama (local models) implementation |
| `neuron-tool` | Tool registry, type erasure, middleware |
| `neuron-mcp` | MCP protocol bridge (wraps rmcp) |
| `neuron-context` | Context compaction strategies |
| `neuron-loop` | The agentic while-loop |
| `neuron-runtime` | Sessions, guardrails, durability |
| `neuron` | Umbrella re-export |

Crates depend only on `neuron-types` and the crates directly below them in the
dependency graph. No circular dependencies. Adding a new provider never touches
the tool system. Adding a new compaction strategy never touches the loop.

## Provider-per-crate (the serde pattern)

The `Provider` trait lives in `neuron-types`. Each cloud API gets its own crate:

```rust,ignore
// neuron-types/src/traits.rs
pub trait Provider: Send + Sync {
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send;

    fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send;
}
```

The trait is intentionally not object-safe (it uses RPITIT). You compose with
generics (`fn run<P: Provider>(provider: &P)`), which gives the compiler full
visibility for optimization.

Why not a single provider crate with feature flags? Because provider APIs evolve
independently. An Anthropic-specific feature (prompt caching, extended thinking)
should not force a recompile of OpenAI code. Separate crates give you separate
version timelines.

## Message structure: flat struct over variant-per-role

neuron uses a flat `Message` struct:

```rust,ignore
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}
```

The alternative -- one enum variant per role (`UserMessage`, `AssistantMessage`,
`SystemMessage`) -- creates a combinatorial explosion of conversion code. Rig
uses the variant-per-role approach and needs roughly 300 lines of conversion
logic per provider. The flat struct maps naturally to every provider API we
studied (Anthropic, OpenAI, Ollama) with minimal translation.

## Tool middleware: axum's `from_fn`, not tower's Service/Layer

The tool middleware pipeline uses a callback-based pattern identical to axum's
`middleware::from_fn`:

```rust,ignore
async fn logging_middleware(
    tool_name: &str,
    input: serde_json::Value,
    ctx: &ToolContext,
    next: ToolMiddlewareNext<'_>,
) -> Result<ToolOutput, ToolError> {
    println!("calling {tool_name}");
    let result = next.run(tool_name, input, ctx).await;
    println!("result: {result:?}");
    result
}
```

tower's `Service` and `Layer` traits are designed for high-throughput
request/response pipelines where the overhead of trait objects and `Pin<Box<...>>`
matters. Tool calls happen at most a few times per LLM turn. The axum-style
callback is simpler to write, simpler to read, and validated by the tokio team
for exactly this kind of middleware.

## DurableContext wraps side effects, not just observes them

Early designs had a single `DurabilityHook` that observed LLM calls and tool
executions. This fails for Temporal replay: an observation hook cannot prevent
a side effect from re-executing during replay.

The solution is `DurableContext`, which **wraps** side effects:

```rust,ignore
pub trait DurableContext: Send + Sync {
    fn execute_llm_call(
        &self,
        request: CompletionRequest,
        options: ActivityOptions,
    ) -> impl Future<Output = Result<CompletionResponse, DurableError>> + Send;

    fn execute_tool(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
        options: ActivityOptions,
    ) -> impl Future<Output = Result<ToolOutput, DurableError>> + Send;
}
```

When a `DurableContext` is present, the agentic loop calls through it instead of
directly calling the provider or tools. The durable engine (Temporal, Restate,
Inngest) can journal the result, and on replay, return the journaled result
without re-executing the side effect.

A separate `ObservabilityHook` trait handles logging, metrics, and telemetry.
It returns `HookAction` (Continue, Skip, or Terminate) but does not wrap
execution.

## RPITIT native async traits

neuron uses Rust 2024 edition with native `impl Future` return types in traits
(RPITIT). There is no `#[async_trait]` anywhere in the codebase:

```rust,ignore
pub trait Provider: Send + Sync {
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send;
}
```

This avoids the heap allocation that `#[async_trait]` forces (one `Box::pin`
per call). The trade-off is that these traits are not object-safe -- you must
use generics, not `dyn Provider`. For type-erased dispatch, neuron provides
`ToolDyn` with an explicit `Box::pin` at the erasure boundary only.

## ToolError::ModelRetry for self-correction

Adopted from Pydantic AI's pattern, `ModelRetry` lets a tool tell the model
to try again with different arguments:

```rust,ignore
pub enum ToolError {
    NotFound(String),
    InvalidInput(String),
    ExecutionFailed(Box<dyn std::error::Error + Send + Sync>),
    PermissionDenied(String),
    Cancelled,
    ModelRetry(String),  // <-- hint for the model
}
```

When a tool returns `ModelRetry("date must be in YYYY-MM-DD format")`, the
loop does **not** propagate this as an error. Instead, it converts the hint into
an error tool result and sends it back to the model. The model sees the hint,
adjusts its arguments, and calls the tool again.

This keeps self-correction logic out of the tool implementation. The tool just
says "try again, here's why" and the loop handles the retry protocol.

## Server-side context compaction

The Anthropic API supports server-side context management: the client sends a
`context_management` field, and the server may respond with
`StopReason::Compaction` plus a `ContentBlock::Compaction` summary.

neuron models this with dedicated types:

```rust,ignore
pub struct ContextManagement {
    pub edits: Vec<ContextEdit>,
}

pub enum ContextEdit {
    Compact { strategy: String },
}

pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    ContentFilter,
    Compaction,  // <-- server compacted context
}

pub enum ContentBlock {
    // ...
    Compaction { content: String },
}
```

When the loop receives `StopReason::Compaction`, it continues automatically --
the server has already compacted the context, and the response contains the
compaction summary. Token usage during compaction is tracked per-iteration via
`UsageIteration`.

This is distinct from client-side compaction (the `ContextStrategy` trait),
which the loop manages locally. Both can coexist: the provider handles
server-side compaction transparently, while the context strategy handles
client-side compaction when needed.
