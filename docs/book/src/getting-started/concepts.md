# Core Concepts

neuron is built around five core abstractions. Each is a trait defined in
`neuron-types` with one or more implementations in satellite crates.

## Provider

The `Provider` trait abstracts LLM API calls. Each provider is its own crate.

```rust,ignore
pub trait Provider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamHandle, ProviderError>;
}
```

Implementations: `Anthropic`, `OpenAi`, `Ollama`. All support `from_env()` for
credential loading.

[Providers Guide](../guides/providers.md)

## Tool

The `Tool` trait defines a function the model can call. Tools have typed
arguments (via `schemars` for JSON Schema) and typed outputs.

```rust,ignore
pub trait Tool: Send + Sync {
    const NAME: &'static str;
    type Args: DeserializeOwned + JsonSchema;
    type Output: Serialize;
    type Error: std::error::Error;

    fn definition(&self) -> ToolDefinition;
    async fn call(&self, args: Self::Args, ctx: &ToolContext) -> Result<Self::Output, Self::Error>;
}
```

The `ToolRegistry` stores tools with type erasure (`ToolDyn`) and runs them
through a composable middleware pipeline.

[Tools Guide](../guides/tools.md)

## ContextStrategy

The `ContextStrategy` trait manages conversation history to stay within token
limits.

```rust,ignore
pub trait ContextStrategy: Send + Sync {
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool;
    async fn compact(&self, messages: Vec<Message>) -> Result<Vec<Message>, ContextError>;
    fn token_estimate(&self, messages: &[Message]) -> usize;
}
```

Implementations: `SlidingWindowStrategy` (drop oldest messages),
`ToolResultClearingStrategy` (clear tool outputs), `CompositeStrategy` (chain
multiple strategies).

[Context Management Guide](../guides/context.md)

## ObservabilityHook

Hooks observe the agent loop lifecycle without altering it (unless they
terminate).

```rust,ignore
pub trait ObservabilityHook: Send + Sync {
    async fn on_event(&self, event: HookEvent<'_>) -> HookAction;
}
```

`HookAction` is `Continue`, `Skip`, or `Terminate(String)`. Implementations:
`TracingHook` (structured `tracing` spans), `GuardrailHook` (input/output
guardrails as hooks).

[Runtime Guide](../guides/runtime.md)

## DurableContext

Wraps side effects (LLM calls, tool execution) for durable engines like
Temporal or Restate.

```rust,ignore
pub trait DurableContext: Send + Sync {
    fn execute_llm_call(&self, request: CompletionRequest, options: ActivityOptions)
        -> impl Future<Output = Result<CompletionResponse, DurableError>> + Send;

    fn execute_tool(&self, tool_name: &str, input: Value, ctx: &ToolContext, options: ActivityOptions)
        -> impl Future<Output = Result<ToolOutput, DurableError>> + Send;
}
```

This enables journal-based replay and recovery for long-running agents.

[Runtime Guide](../guides/runtime.md)
