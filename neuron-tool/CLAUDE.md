# neuron-tool

Tool registry with composable middleware pipeline for LLM function calling.

## Key types

- `ToolRegistry` -- register, lookup, and execute type-erased tools. Stores `Arc<dyn ToolDyn>` with global and per-tool middleware chains.
- `ToolMiddleware` trait -- wraps tool execution with cross-cutting concerns (validation, permissions, logging). Uses boxed futures for dyn-compatibility.
- `ToolCall` -- a tool call in flight through the middleware pipeline (id, name, input).
- `Next` -- the remaining middleware chain plus the underlying tool. Consumed on call to prevent double-invoke.
- `tool_middleware_fn` -- create middleware from a closure (axum's `from_fn` pattern).
- `PermissionChecker` -- middleware that checks tool calls against a `PermissionPolicy`. Deny/Ask returns `ToolError::PermissionDenied`.
- `OutputFormatter` -- middleware that truncates tool output text to a maximum character length. UTF-8 boundary safe.
- `SchemaValidator` -- middleware that validates tool input against JSON Schema (structural checks: required fields, property types).
- `neuron_tool` (proc macro, `macros` feature) -- derive a `Tool` impl from an async function. Re-exported from `neuron-tool-macros`.

## Key design decisions

- **Middleware pattern is axum's `from_fn`**, validated by the tokio team. Each middleware receives `(&ToolCall, &ToolContext, Next)` and can inspect/modify/short-circuit. Do NOT adopt tower's `Service`/`Layer`.
- **Type erasure via `ToolDyn`** -- tools are registered as strongly-typed `Tool` impls, auto-erased to `Arc<dyn ToolDyn>` for heterogeneous storage. `ToolDyn` is defined in `neuron-types`.
- **Middleware chain order**: global middleware runs first, then per-tool middleware, then the actual tool.
- **Boxed futures** for `ToolMiddleware::process` -- required for dyn-compatibility (heterogeneous middleware collections in `Vec<Arc<dyn ToolMiddleware>>`).
- **`SchemaValidator` snapshots schemas at construction** -- tools registered after construction are not validated. This is intentional for simplicity.
- **`MiddlewareFn` is private** -- users call `tool_middleware_fn()` which returns `impl ToolMiddleware`.

## Dependencies

- `neuron-types` (workspace) -- `Tool`, `ToolDyn`, `ToolContext`, `ToolDefinition`, `ToolOutput`, `ToolError`, `PermissionPolicy`, `WasmBoxedFuture`
- `neuron-tool-macros` (workspace, optional, `macros` feature) -- `#[neuron_tool]` proc macro
- `serde`, `serde_json` (workspace) -- JSON handling for tool inputs
- `tokio` (workspace) -- async runtime
- `tracing` (workspace) -- instrumentation

## Structure

```
neuron-tool/
    CLAUDE.md
    Cargo.toml
    src/
        lib.rs             # Public API, re-exports from all modules
        registry.rs        # ToolRegistry -- register, lookup, execute with middleware
        middleware.rs       # ToolMiddleware trait, Next, ToolCall, tool_middleware_fn
        builtin.rs         # PermissionChecker, OutputFormatter, SchemaValidator
```

## Conventions

- Flat file structure, one concept per file
- All public types re-exported from `lib.rs` via glob re-exports (`pub use module::*`)
- `#[must_use]` on constructors and Result-returning functions
- Error types come from `neuron-types` (`ToolError`)
- No `unwrap()` in library code
- Doc comments on every public item
