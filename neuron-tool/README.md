# neuron-tool

Tool registry and middleware pipeline for the neuron ecosystem. Provides
`ToolRegistry` for registering, looking up, and executing tools through a
composable middleware chain. The middleware pattern is identical to axum's
`from_fn` -- each middleware receives a `Next` that it calls to continue the
chain or skips to short-circuit.

## Key Types

- `ToolRegistry` -- stores type-erased `ToolDyn` trait objects, dispatches calls through middleware
- `ToolMiddleware` -- trait for middleware that wraps tool execution (validate, log, permission check)
- `ToolCall` -- a tool call in flight: `id`, `name`, and `input` JSON
- `Next` -- the remaining middleware chain plus the underlying tool; consumed on call
- `tool_middleware_fn()` -- creates middleware from a closure (like axum's `from_fn`)

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

Tools are registered as strongly-typed `Tool` impls and automatically erased
to `ToolDyn` for storage. Pre-erased tools can also be registered via
`register_dyn()` for tools that arrive as `Arc<dyn ToolDyn>` (e.g., from MCP).

The companion crate `neuron-tool-macros` provides the `#[neuron_tool]`
attribute macro to generate `Tool` implementations from annotated async
functions. It is re-exported as `neuron_tool::neuron_tool` when the `macros`
feature is enabled.

## Part of neuron

This crate is part of [neuron](https://github.com/empathic-ai/neuron), a
composable building-blocks library for AI agents in Rust.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
