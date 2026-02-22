# neuron-types

Foundation crate for the neuron ecosystem. Defines the core types, traits, and
error enums that every other neuron crate depends on. Contains zero logic --
only data structures, trait definitions, and serde implementations. This is the
equivalent of `serde`'s core: traits live here, implementations live in
satellite crates.

## Key Types

- `Message` -- a conversation message with a `Role` and `Vec<ContentBlock>`
- `Role` -- `User`, `Assistant`, or `System`
- `ContentBlock` -- text, thinking, tool use/result, image, or document
- `CompletionRequest` -- full LLM request: model, messages, system prompt, tools, temperature, thinking config
- `CompletionResponse` -- LLM response: message, token usage, stop reason
- `ToolDefinition` -- tool name, description, and JSON Schema for input
- `ToolOutput` -- tool execution result with content items and optional structured JSON
- `ToolContext` -- runtime context (cwd, session ID, environment, cancellation token)
- `TokenUsage` -- input/output/cache/reasoning token counts
- `StopReason` -- why the model stopped: `EndTurn`, `ToolUse`, `MaxTokens`, `StopSequence`, `ContentFilter`

## Key Traits

- `Provider` -- LLM provider with `complete()` and `complete_stream()` (RPITIT, not object-safe)
- `Tool` -- strongly typed tool with `NAME`, `Args`, `Output`, `Error` associated types
- `ToolDyn` -- type-erased tool for heterogeneous registries (blanket-implemented for all `Tool` impls)
- `ContextStrategy` -- context compaction: `should_compact()`, `compact()`, `token_estimate()`
- `ObservabilityHook` -- logging/metrics/telemetry hooks with `Continue`/`Skip`/`Terminate` actions
- `DurableContext` -- wraps side effects for durable execution engines (Temporal, Restate)
- `PermissionPolicy` -- tool call permission checks returning `Allow`/`Deny`/`Ask`

## Usage

```rust,no_run
use neuron_types::{Message, Role, ContentBlock, CompletionRequest};

// Construct a message with text content
let message = Message {
    role: Role::User,
    content: vec![ContentBlock::Text("What is 2 + 2?".into())],
};

// Build a completion request
let request = CompletionRequest {
    model: "claude-sonnet-4-20250514".into(),
    messages: vec![message],
    system: Some("You are a calculator.".into()),
    tools: vec![],
    max_tokens: Some(1024),
    temperature: Some(0.0),
    top_p: None,
    stop_sequences: vec![],
    tool_choice: None,
    response_format: None,
    thinking: None,
    reasoning_effort: None,
    extra: None,
};
```

Implementing the `Provider` trait (Rust 2024 native async, no `#[async_trait]`):

```rust,ignore
use neuron_types::*;

struct MyProvider { /* ... */ }

impl Provider for MyProvider {
    fn complete(&self, request: CompletionRequest)
        -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send
    {
        async { todo!() }
    }

    fn complete_stream(&self, request: CompletionRequest)
        -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send
    {
        async { todo!() }
    }
}
```

## Part of neuron

This crate is part of [neuron](https://github.com/empathic-ai/neuron), a
composable building-blocks library for AI agents in Rust.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
