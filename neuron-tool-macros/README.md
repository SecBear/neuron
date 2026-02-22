# neuron-tool-macros

Procedural macro crate for the neuron tool system. Provides the `#[neuron_tool]`
attribute macro that generates a full `Tool` trait implementation from an
annotated async function, eliminating boilerplate for tool definitions.

## What It Generates

Given an async function, the macro generates:

- An `Args` struct (e.g., `CalculateArgs`) with `Deserialize` and `JsonSchema` derives
- A zero-sized `Tool` struct (e.g., `CalculateTool`)
- A `Tool` trait implementation with the function body as `call()`
- A `ToolDefinition` with the JSON Schema derived from the args struct via `schemars`

Field doc comments (`///`) on function parameters are preserved as schema descriptions.

## Key Attributes

- `name` (required) -- the tool name string used in LLM tool calls
- `description` (required) -- human-readable description of what the tool does

## Usage

```rust,ignore
use neuron_tool_macros::neuron_tool;
use neuron_types::ToolContext;

#[neuron_tool(name = "calculate", description = "Evaluate a math expression")]
async fn calculate(
    /// A mathematical expression like "2 + 2"
    expression: String,
    _ctx: &ToolContext,
) -> Result<CalculateOutput, CalculateError> {
    let result = eval(&expression)?;
    Ok(CalculateOutput { result })
}

// The macro generates:
// - `CalculateArgs { expression: String }` with Deserialize + JsonSchema
// - `CalculateTool` (zero-sized struct)
// - `impl Tool for CalculateTool` with NAME = "calculate"
//
// Register the generated tool:
// registry.register(CalculateTool);
```

The last parameter must be `&ToolContext` (or `_ctx: &ToolContext` if unused).
All preceding parameters become fields on the generated args struct. The return
type must be `Result<Output, Error>` where both types are concrete.

The struct names are derived from the function name converted to PascalCase:
`calculate` produces `CalculateTool` and `CalculateArgs`, `read_file` produces
`ReadFileTool` and `ReadFileArgs`.

This macro is re-exported from `neuron-tool` when the `macros` feature is
enabled: `neuron_tool::neuron_tool`.

## Part of neuron

This crate is part of [neuron](https://github.com/empathic-ai/neuron), a
composable building-blocks library for AI agents in Rust.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
