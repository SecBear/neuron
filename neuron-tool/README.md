# neuron-tool

[![crates.io](https://img.shields.io/crates/v/neuron-tool.svg)](https://crates.io/crates/neuron-tool)
[![docs.rs](https://docs.rs/neuron-tool/badge.svg)](https://docs.rs/neuron-tool)
[![license](https://img.shields.io/crates/l/neuron-tool.svg)](LICENSE-MIT)

Tool registry and middleware pipeline for the neuron ecosystem. Provides
`ToolRegistry` for registering, looking up, and executing tools through a
composable middleware chain. The middleware pattern is identical to axum's
`from_fn` -- each middleware receives a `Next` that it calls to continue the
chain or skips to short-circuit.

## Installation

```sh
cargo add neuron-tool
```

## Key Types

- `ToolRegistry` -- stores type-erased `ToolDyn` trait objects, dispatches calls through middleware
- `ToolMiddleware` -- trait for middleware that wraps tool execution (validate, log, permission check)
- `ToolCall` -- a tool call in flight: `id`, `name`, and `input` JSON
- `Next` -- the remaining middleware chain plus the underlying tool. Moved (consumed) when called — each middleware must call `next.run()` exactly once or skip it to short-circuit
- `tool_middleware_fn()` -- creates middleware from a closure (like axum's `from_fn`)
- `PermissionChecker` -- middleware that checks tool calls against a `PermissionPolicy`
- `OutputFormatter` -- middleware that truncates tool output text to a maximum character length
- `SchemaValidator` -- middleware that validates tool input against JSON Schema (structural checks)
- `TimeoutMiddleware` -- middleware that enforces per-tool execution timeouts via `tokio::time::timeout`
- `StructuredOutputValidator` -- middleware that validates tool input against a JSON Schema, returning `ToolError::ModelRetry` on failure for self-correction
- `RetryLimitedValidator` -- wraps `StructuredOutputValidator` with a maximum retry count to prevent infinite correction loops

## Usage

```rust,ignore
use neuron_tool::{ToolRegistry, tool_middleware_fn};
use neuron_types::ToolContext;

// Create a registry and register tools
let mut registry = ToolRegistry::new();
registry.register(MyTool);

// Add global middleware (applies to all tool executions)
registry.add_middleware(tool_middleware_fn(|call, ctx, next| {
    Box::pin(async move {
        println!("calling tool: {}", call.name);
        let result = next.run(call, ctx).await;
        println!("tool {} finished", call.name);
        result
    })
}));

// Add per-tool middleware (applies only to "my_tool")
registry.add_tool_middleware("my_tool", tool_middleware_fn(|call, ctx, next| {
    Box::pin(async move {
        // Validate, transform, or reject before the tool runs
        next.run(call, ctx).await
    })
}));

// Execute a tool by name -- runs through global, then per-tool middleware
let input = serde_json::json!({ "query": "hello" });
let output = registry.execute("my_tool", input, &tool_ctx).await?;

// Get all tool definitions for passing to a CompletionRequest
let definitions = registry.definitions();
```

### Self-correction with `ModelRetry`

Tools can return `Err(ToolError::ModelRetry(hint))` when the model provides
invalid input. The agent loop converts the hint into an error tool result so the
model can retry with adjusted arguments — no hard failure, no manual reprompting:

```rust,ignore
if !is_valid_country_code(&code) {
    return Err(ToolError::ModelRetry(format!(
        "Expected 2-letter ISO code (e.g. \"US\"), got: \"{code}\""
    )));
}
```

Tools are registered as strongly-typed `Tool` impls and automatically erased
to `ToolDyn` for storage. Pre-erased tools can also be registered via
`register_dyn()` for tools that arrive as `Arc<dyn ToolDyn>` (e.g., from MCP).

The companion crate `neuron-tool-macros` provides the `#[neuron_tool]`
attribute macro to generate `Tool` implementations from annotated async
functions. It is re-exported as `neuron_tool::neuron_tool` when the `macros`
feature is enabled.

## Part of neuron

This crate is part of [neuron](https://github.com/secbear/neuron), a
composable building-blocks library for AI agents in Rust.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
