# neuron-tool

> Tool interface and registry for neuron agents

[![crates.io](https://img.shields.io/crates/v/neuron-tool.svg)](https://crates.io/crates/neuron-tool)
[![docs.rs](https://docs.rs/neuron-tool/badge.svg)](https://docs.rs/neuron-tool)
[![license](https://img.shields.io/crates/l/neuron-tool.svg)](LICENSE-MIT)

## Overview

`neuron-tool` provides the `Tool` trait and `ToolRegistry` that operators use to expose callable
functions to LLMs. Tools are described via JSON Schema, invoked with JSON arguments, and return
JSON results.

```
ToolRegistry → serialized to tool_list → sent to model → model emits tool_call
             → ToolRegistry::call(name, args) → Tool::invoke → result back to model
```

## Usage

```toml
[dependencies]
neuron-tool = "0.4"
serde_json = "1"
```

### Defining a tool

```rust
use neuron_tool::{Tool, ToolError};
use serde_json::{json, Value};

pub struct UppercaseTool;

impl Tool for UppercaseTool {
    fn name(&self) -> &str { "uppercase" }

    fn description(&self) -> &str { "Convert text to uppercase" }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Text to convert" }
            },
            "required": ["text"]
        })
    }

    fn invoke(&self, input: Value) -> Result<Value, ToolError> {
        let text = input["text"].as_str().ok_or(ToolError::InvalidInput("missing text".into()))?;
        Ok(json!({ "result": text.to_uppercase() }))
    }
}
```

### Registering tools

```rust
use neuron_tool::ToolRegistry;

let mut registry = ToolRegistry::new();
registry.register(UppercaseTool);
```

## Part of the neuron workspace

[neuron](https://github.com/secbear/neuron) is a composable async agentic AI framework for Rust.
See the [book](https://secbear.github.io/neuron) for architecture and guides.
