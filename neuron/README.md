# neuron

Umbrella crate for the neuron composable agent blocks ecosystem. Re-exports all
neuron crates through a single dependency, with feature flags controlling which
provider and integration blocks are included. This crate contains no logic of
its own -- it exists purely for convenience.

## Feature Flags

| Feature | Enables | Default |
|-----------|------------------------------------------|---------|
| `anthropic` | `neuron::anthropic` (Anthropic Claude) | yes |
| `openai` | `neuron::openai` (OpenAI GPT) | no |
| `ollama` | `neuron::ollama` (Ollama local) | no |
| `mcp` | `neuron::mcp` (Model Context Protocol) | no |
| `runtime` | `neuron::runtime` (sessions, guardrails) | no |
| `full` | All of the above | no |

## Module Map

| Module | Underlying Crate | Contents |
|-----------------|---------------------------|------------------------------------------|
| `neuron::types` | `neuron-types` | Messages, traits, errors, streaming |
| `neuron::tool` | `neuron-tool` | ToolRegistry, middleware pipeline |
| `neuron::context`| `neuron-context` | Token counting, compaction strategies |
| `neuron::loop` | `neuron-loop` | AgentLoop, LoopConfig, AgentResult |
| `neuron::anthropic`| `neuron-provider-anthropic`| Anthropic client (feature-gated) |
| `neuron::openai`| `neuron-provider-openai` | OpenAi client (feature-gated) |
| `neuron::ollama`| `neuron-provider-ollama` | Ollama client (feature-gated) |
| `neuron::mcp` | `neuron-mcp` | McpClient, McpToolBridge (feature-gated) |
| `neuron::runtime`| `neuron-runtime` | Sessions, guardrails (feature-gated) |

## Usage

Add `neuron` to your `Cargo.toml` with the features you need:

```toml
[dependencies]
neuron = { version = "0.1", features = ["anthropic", "mcp"] }
```

Then use the prelude for common imports:

```rust,no_run
use neuron::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The prelude re-exports Provider, CompletionRequest, Message,
    // Role, ToolRegistry, AgentLoop, and other commonly used types.
    let provider = neuron::anthropic::Anthropic::new("sk-ant-...");

    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Hello!".into())],
        }],
        max_tokens: Some(1024),
        ..Default::default()
    };

    let response = provider.complete(request).await?;

    for block in &response.message.content {
        println!("{block:?}");
    }
    Ok(())
}
```

## Prelude Contents

The `neuron::prelude` module re-exports the most commonly used types:

- `CompletionRequest`, `CompletionResponse`, `Message`, `Role`, `ContentBlock`,
  `ContentItem`, `SystemPrompt`, `TokenUsage`, `StopReason` -- conversation
  primitives.
- `Provider` -- the LLM provider trait.
- `Tool`, `ToolDyn`, `ToolDefinition`, `ToolContext`, `ToolOutput`,
  `ToolError` -- tool system types.
- `ToolRegistry` -- tool registration and dispatch.
- `SlidingWindowStrategy` -- context compaction.
- `AgentLoop`, `AgentLoopBuilder`, `AgentResult`, `LoopConfig` -- the agentic
  loop.

## Part of neuron

This is the root crate of [neuron](https://github.com/secbear/neuron). For
maximum independence, depend on individual block crates (`neuron-types`,
`neuron-provider-anthropic`, etc.) directly.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
