# neuron-types

[![crates.io](https://img.shields.io/crates/v/neuron-types.svg)](https://crates.io/crates/neuron-types)
[![docs.rs](https://docs.rs/neuron-types/badge.svg)](https://docs.rs/neuron-types)
[![license](https://img.shields.io/crates/l/neuron-types.svg)](LICENSE-MIT)

Foundation crate for the neuron ecosystem. Defines the core types, traits, and
error enums that every other neuron crate depends on. Contains zero logic --
only data structures, trait definitions, and serde implementations. This is the
equivalent of `serde`'s core: traits live here, implementations live in
satellite crates.

## Installation

```sh
cargo add neuron-types
```

## Key Types

- `Message` -- a conversation message with a `Role` and `Vec<ContentBlock>`.
  Convenience constructors: `Message::user()`, `Message::assistant()`, `Message::system()`.
- `Role` -- `User`, `Assistant`, or `System`
- `ContentBlock` -- text, thinking, tool use/result, image, document, or server-side compaction summary
- `CompletionRequest` -- full LLM request: model, messages, system prompt, tools, temperature, thinking config, context management
- `CompletionResponse` -- LLM response: message, token usage, stop reason
- `ToolDefinition` -- tool name, description, and JSON Schema for input
- `ToolOutput` -- tool execution result with content items and optional structured JSON
- `ToolContext` -- runtime context (cwd, session ID, environment, cancellation token).
  Implements `Default` for zero-config construction.
- `TokenUsage` -- input/output/cache/reasoning token counts
- `UsageIteration` -- per-iteration token breakdown (during server-side compaction)
- `ContextManagement`, `ContextEdit` -- server-side context management configuration
- `StopReason` -- why the model stopped: `EndTurn`, `ToolUse`, `MaxTokens`, `StopSequence`, `ContentFilter`, `Compaction`
- `EmbeddingRequest` -- embedding model request: model, input texts, optional dimensions
- `EmbeddingResponse` -- embedding model response: vectors, usage
- `EmbeddingUsage` -- token counts for an embedding request
- `UsageLimits` -- token usage budget constraints (request tokens, response tokens, total tokens) enforced by the agentic loop

## Key Traits

- `Provider` -- LLM provider with `complete()` and `complete_stream()` (RPITIT, not object-safe)
- `EmbeddingProvider` -- embedding model provider with `embed()` (RPITIT, separate from Provider)
- `Tool` -- strongly typed tool with `NAME`, `Args`, `Output`, `Error` associated types
- `ToolDyn` -- type-erased tool for heterogeneous registries (blanket-implemented for all `Tool` impls)
- `ContextStrategy` -- context compaction: `should_compact()`, `compact()`, `token_estimate()`
- `ObservabilityHook` -- logging/metrics/telemetry hooks returning a `HookAction`:
  - `Continue` — proceed normally
  - `Skip` — reject the tool call and return the reason as a tool result so the model can adapt
  - `Terminate` — halt the loop immediately
- `DurableContext` -- wraps side effects for durable execution engines (Temporal, Restate)
- `PermissionPolicy` -- tool call permission checks returning:
  - `Allow` — permit the call
  - `Deny(reason)` — block the call
  - `Ask(question)` — prompt the user for confirmation

## Usage

```rust,no_run
use neuron_types::{Message, CompletionRequest};

// Construct a message with convenience constructor
let message = Message::user("What is 2 + 2?");

// Build a completion request (only specify what you need)
let request = CompletionRequest {
    model: "claude-sonnet-4-20250514".into(),
    messages: vec![message],
    system: Some("You are a calculator.".into()),
    max_tokens: Some(1024),
    temperature: Some(0.0),
    ..Default::default()
};
```

Implementing the `Provider` trait (Rust 2024 native async, no `#[async_trait]`).
This example is marked `ignore` because it's an abstract skeleton — a real
implementation would make HTTP calls to an LLM API:

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

This crate is part of [neuron](https://github.com/secbear/neuron), a
composable building-blocks library for AI agents in Rust.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
