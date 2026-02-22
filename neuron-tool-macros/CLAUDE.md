# neuron-tool-macros

Proc-macro crate that derives `Tool` implementations from annotated async functions.

## Exports

- `#[neuron_tool(name = "...", description = "...")]` — attribute macro applied
  to an async function. Generates a zero-sized tool struct, an args struct, and
  a `Tool` trait impl.

## What the macro generates

Given a function `calculate`, the macro emits:

- **`CalculateArgs`** — struct with `Deserialize` + `JsonSchema` derives. Fields
  come from the function parameters (all except the last `&ToolContext` param).
  Doc comments on parameters become doc comments on fields (which `schemars`
  picks up for JSON Schema descriptions).
- **`CalculateTool`** — unit struct implementing `neuron_types::Tool`. The impl
  provides `NAME`, `Args`/`Output`/`Error` associated types, `definition()`
  (builds `ToolDefinition` with schema from `schemars::schema_for!`), and
  `call()` (destructures args and runs the original function body).

Naming convention: function name is converted to PascalCase, then suffixed with
`Tool` or `Args`.

## Key design decisions

- **Attribute macro, not derive.** The input is a function, not a struct, so
  `#[proc_macro_attribute]` is the correct form.
- **Last parameter must be `&ToolContext`.** The macro strips it from the args
  struct and threads it through to `call()`. A `let _ = &ctx;` suppresses
  unused-variable warnings when the function uses `_ctx`.
- **Schema via `schemars`** on the generated args struct. The macro does not
  hand-build JSON Schema — it relies on `schemars::schema_for!` at the call
  site, so the user's crate must depend on `schemars` and `serde_json`.
- **No `ToolDyn` generation.** Type erasure is handled by `ToolRegistry` in
  `neuron-tool` when the tool is registered.
- **Return type must be `Result<Output, Error>`.** The macro extracts both type
  parameters from the return type at compile time.
- **Compile errors, not panics.** All validation failures return `syn::Error`
  for clear diagnostics.

## Dependencies

- `syn` — parsing Rust syntax
- `quote` — generating token streams
- `proc-macro2` — proc-macro utilities

No runtime dependencies. The generated code references `neuron_types`, `serde`,
`schemars`, and `serde_json`, but those are dependencies of the user's crate,
not of this macro crate.

## Structure

```
neuron-tool-macros/
    CLAUDE.md
    Cargo.toml
    src/
        lib.rs          # Everything: attribute parsing, code generation, helpers
```

Single-file crate. Internal helpers: `AgentToolArgs` (attribute parser),
`to_pascal_case`, `expand_neuron_tool` (main codegen), `extract_result_types`.
