# Tools

The tool system in neuron lets you give LLMs the ability to call into your Rust
code. You define strongly-typed tools, register them in a `ToolRegistry`, and
optionally wrap execution with middleware for logging, validation, or permissions.

## Quick example

```rust,ignore
use neuron_tool::{neuron_tool, ToolRegistry};
use neuron_types::ToolContext;

#[neuron_tool(name = "lookup", description = "Look up a value by key")]
async fn lookup(
    /// The key to look up
    key: String,
    _ctx: &ToolContext,
) -> Result<String, std::io::Error> {
    Ok(format!("value for {key}"))
}

#[tokio::main]
async fn main() {
    let mut registry = ToolRegistry::new();
    registry.register(LookupTool);

    let ctx = ToolContext::default();
    let output = registry
        .execute("lookup", serde_json::json!({"key": "foo"}), &ctx)
        .await
        .unwrap();
    println!("{:?}", output.content);
}
```

## Core traits

### `Tool` -- strongly typed

The `Tool` trait is the primary way to define a tool. It uses Rust's type system
to enforce correct input/output handling at compile time.

```rust,ignore
pub trait Tool: Send + Sync {
    const NAME: &'static str;
    type Args: DeserializeOwned + JsonSchema + Send;
    type Output: Serialize;
    type Error: std::error::Error + Send + 'static;

    fn definition(&self) -> ToolDefinition;
    fn call(&self, args: Self::Args, ctx: &ToolContext) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send;
}
```

Key points:

- `NAME` -- a unique identifier the LLM uses to invoke the tool.
- `Args` -- must derive `Deserialize` and `schemars::JsonSchema` so the registry
  can generate a JSON Schema for the LLM and deserialize its input.
- `Output` -- must implement `Serialize`; the blanket `ToolDyn` impl serializes it
  to JSON automatically.
- `definition()` -- returns a `ToolDefinition` containing the name, description,
  and JSON Schema. The LLM sees this to decide when to call the tool.

### `ToolDyn` -- type-erased

Every `Tool` automatically implements `ToolDyn` via a blanket impl. `ToolDyn` is
the dyn-compatible version that the `ToolRegistry` stores internally:

```rust,ignore
pub trait ToolDyn: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    fn call_dyn(&self, input: serde_json::Value, ctx: &ToolContext) -> WasmBoxedFuture<'_, Result<ToolOutput, ToolError>>;
}
```

The blanket impl handles JSON deserialization of `Args`, calling `Tool::call`,
serializing the `Output` into `ToolOutput`, and mapping errors to `ToolError`.

## The `#[neuron_tool]` macro

For simple tools, the `neuron_tool` attribute macro reduces boilerplate. It
generates the `Args` struct, `Tool` struct, and `Tool` impl from a single
annotated async function:

```rust,ignore
use neuron_tool::neuron_tool;
use neuron_types::ToolContext;

#[derive(Debug, serde::Serialize)]
struct WeatherOutput { temperature: f64, conditions: String }

#[derive(Debug, thiserror::Error)]
#[error("weather error: {0}")]
struct WeatherError(String);

#[neuron_tool(name = "get_weather", description = "Get current weather for a city")]
async fn get_weather(
    /// City name (e.g. "San Francisco")
    city: String,
    _ctx: &ToolContext,
) -> Result<WeatherOutput, WeatherError> {
    // The macro generates GetWeatherTool and GetWeatherArgs automatically
    Ok(WeatherOutput { temperature: 72.0, conditions: "sunny".into() })
}
```

The macro generates:

- `GetWeatherArgs` -- a struct with `#[derive(Deserialize, JsonSchema)]`
- `GetWeatherTool` -- a unit struct implementing `Tool`
- Doc comments on function parameters become JSON Schema descriptions

Register the generated struct: `registry.register(GetWeatherTool)`.

## `ToolRegistry`

The registry stores tools and executes them through an optional middleware chain.

```rust,ignore
use neuron_tool::ToolRegistry;

let mut registry = ToolRegistry::new();

// Register a strongly-typed tool (auto-erased to ToolDyn)
registry.register(MyTool);

// Register a pre-erased tool (e.g. from MCP bridge)
registry.register_dyn(arc_tool_dyn);

// Get all definitions to send to the LLM
let defs: Vec<ToolDefinition> = registry.definitions();

// Execute a tool by name with JSON input
let output = registry.execute("my_tool", json_input, &tool_ctx).await?;

// Look up a specific tool
let tool: Option<Arc<dyn ToolDyn>> = registry.get("my_tool");
```

## `ToolContext`

Every tool call receives a `ToolContext` providing runtime information:

| Field | Type | Description |
|---|---|---|
| `cwd` | `PathBuf` | Current working directory |
| `session_id` | `String` | Session identifier |
| `environment` | `HashMap<String, String>` | Key-value environment |
| `cancellation_token` | `CancellationToken` | Cooperative cancellation |
| `progress_reporter` | `Option<Arc<dyn ProgressReporter>>` | Progress feedback for long-running tools |

`ToolContext` implements `Default` with the current directory, an empty session
ID, an empty environment, and a fresh cancellation token.

## Middleware

Middleware wraps tool execution with cross-cutting concerns. The pattern is
identical to axum's `from_fn` -- each middleware receives a `Next` that it can
call to continue the chain, or skip to short-circuit.

### Writing middleware with closures

```rust,ignore
use neuron_tool::{tool_middleware_fn, ToolRegistry};

let logging = tool_middleware_fn(|call, ctx, next| {
    Box::pin(async move {
        println!("calling tool: {}", call.name);
        let result = next.run(call, ctx).await;
        println!("tool completed: is_error={}", result.as_ref().map(|o| o.is_error).unwrap_or(true));
        result
    })
});

let mut registry = ToolRegistry::new();
registry.add_middleware(logging);
```

### Writing middleware as a struct

```rust,ignore
use neuron_tool::middleware::{ToolMiddleware, ToolCall, Next};
use neuron_types::{ToolContext, ToolError, ToolOutput, WasmBoxedFuture};

struct RateLimiter { /* ... */ }

impl ToolMiddleware for RateLimiter {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            // Check rate limit, then proceed
            next.run(call, ctx).await
        })
    }
}
```

### Input validation middleware

A common use case is intercepting tool calls to validate input arguments before
the tool executes. When validation fails, returning `ToolError::ModelRetry` gives
the model a hint so it can self-correct rather than crashing the loop.

Here is a closure-based validation middleware that checks URL and numeric range
arguments:

```rust,ignore
use neuron_tool::{tool_middleware_fn, ToolRegistry};
use neuron_types::ToolError;

let mut registry = ToolRegistry::new();

// Input validation middleware — rejects invalid arguments with a hint
// so the model can self-correct
registry.add_middleware(tool_middleware_fn(|call, ctx, next| {
    Box::pin(async move {
        // Validate URL arguments
        if let Some(url) = call.input.get("url").and_then(|v| v.as_str()) {
            if !url.starts_with("https://") {
                return Err(ToolError::ModelRetry(
                    format!("url must start with https://, got '{url}'")
                ));
            }
        }

        // Validate numeric ranges
        if let Some(count) = call.input.get("count").and_then(|v| v.as_u64()) {
            if count == 0 || count > 100 {
                return Err(ToolError::ModelRetry(
                    format!("count must be 1-100, got {count}")
                ));
            }
        }

        // Input is valid — proceed to the tool
        next.run(call, ctx).await
    })
}));
```

The middleware reads fields from `call.input` (a `serde_json::Value`) and returns
early with a validation hint when constraints are violated. Because it uses
`ToolError::ModelRetry`, the agentic loop converts the message into an error tool
result that the model sees as feedback -- it can then retry the call with
corrected arguments.

For the struct-based approach, implement `ToolMiddleware` the same way as the
`RateLimiter` example above, placing validation logic inside the `process` method
and returning `Err(ToolError::ModelRetry(hint))` on failure.

#### `ToolError` variants for validation

Choose the right error variant depending on whether the model can recover:

| Variant | Behavior | Use when |
|---|---|---|
| `ModelRetry(hint)` | Loop sends the hint back to the model as an error tool result. The model retries with corrected arguments. | Validation errors the model can fix: bad format, out-of-range values, missing optional fields |
| `InvalidInput(msg)` | Propagates as `LoopError::Tool` and stops the loop. | Unrecoverable issues: impossible argument combinations, security violations, malformed JSON |

Use `ModelRetry` as the default for input validation. Reserve `InvalidInput` for
cases where no amount of retrying will produce valid input.

#### Scoping validation to specific tools

Use per-tool middleware to apply validation only where it is needed:

```rust,ignore
// This validation runs only when the "fetch_page" tool is called
registry.add_tool_middleware("fetch_page", tool_middleware_fn(|call, ctx, next| {
    Box::pin(async move {
        if let Some(url) = call.input.get("url").and_then(|v| v.as_str()) {
            if !url.starts_with("https://") {
                return Err(ToolError::ModelRetry(
                    format!("fetch_page requires an https:// URL, got '{url}'")
                ));
            }
        }
        next.run(call, ctx).await
    })
}));
```

### Middleware execution order

Middleware executes in registration order, wrapping tool calls from outside in:

1. Global middleware (registered with `add_middleware`) runs first
2. Per-tool middleware (registered with `add_tool_middleware`) runs next
3. The actual tool executes last

```rust,ignore
registry.add_middleware(logging_middleware);       // Runs first for ALL tools
registry.add_tool_middleware("search", auth_mw);  // Runs second, only for "search"
// The tool itself runs last
```

### Built-in middleware

neuron-tool ships three built-in middleware implementations:

- **`PermissionChecker`** -- checks a `PermissionPolicy` before each tool call.
  Returns `ToolError::PermissionDenied` on `Deny` or `Ask` decisions.
- **`OutputFormatter`** -- truncates tool output exceeding a character limit.
  Useful to prevent large tool results from consuming the context window.
- **`SchemaValidator`** -- validates tool call inputs against their JSON Schema
  before execution. Catches missing required fields and type mismatches.

```rust,ignore
use neuron_tool::builtin::{PermissionChecker, OutputFormatter, SchemaValidator};

// Truncate outputs longer than 10,000 characters
registry.add_middleware(OutputFormatter::new(10_000));

// Validate inputs before execution
let validator = SchemaValidator::new(&registry);
registry.add_middleware(validator);
```

## `ToolError::ModelRetry`

The `ModelRetry` variant enables self-correction. When a tool returns
`Err(ToolError::ModelRetry(hint))`, the agentic loop converts the hint into
an error tool result and sends it back to the model. The model sees the hint
and can retry with corrected arguments.

```rust,ignore
use neuron_types::ToolError;

// Inside a tool's call() method:
if !is_valid_query(&args.query) {
    return Err(ToolError::ModelRetry(
        "Query must be a valid SQL SELECT statement. \
         You provided a DELETE statement.".to_string()
    ));
}
```

This does not propagate as a `LoopError` -- the loop continues with the model
receiving the hint as feedback.

## Implementing `Tool` manually

When you need full control (custom schemas, complex error types), implement `Tool`
directly instead of using the macro:

```rust,ignore
use neuron_types::{Tool, ToolContext, ToolDefinition};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchArgs {
    query: String,
    max_results: Option<usize>,
}

struct SearchTool { api_key: String }

impl Tool for SearchTool {
    const NAME: &'static str = "search";
    type Args = SearchArgs;
    type Output = Vec<String>;
    type Error = std::io::Error;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "search".into(),
            title: Some("Web Search".into()),
            description: "Search the web for information".into(),
            input_schema: serde_json::to_value(
                schemars::schema_for!(SearchArgs)
            ).unwrap(),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(&self, args: SearchArgs, _ctx: &ToolContext) -> Result<Vec<String>, std::io::Error> {
        let max = args.max_results.unwrap_or(5);
        Ok(vec![format!("Result for '{}' (max {})", args.query, max)])
    }
}
```

## API reference

- [`neuron_tool` on docs.rs](https://docs.rs/neuron-tool)
- [`neuron_tool_macros` on docs.rs](https://docs.rs/neuron-tool-macros)
- [`Tool` trait in `neuron_types`](https://docs.rs/neuron-types/latest/neuron_types/trait.Tool.html)
- [`ToolDyn` trait in `neuron_types`](https://docs.rs/neuron-types/latest/neuron_types/trait.ToolDyn.html)
