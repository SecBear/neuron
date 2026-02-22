# neuron-provider-anthropic

Anthropic Claude provider for the neuron agent blocks ecosystem. Implements the
`Provider` trait from `neuron-types` against the Anthropic Messages API,
supporting both synchronous completions and server-sent event (SSE) streaming.

The default model is `claude-sonnet-4-20250514`. The default base URL is
`https://api.anthropic.com`. Both can be overridden with the builder API.

## Key Types

- `Anthropic` -- client struct with builder methods (`new`, `model`, `base_url`).
  Implements `Provider` from `neuron-types`.
- `ProviderError` -- re-exported error type for all provider failures (auth,
  rate limit, server errors).
- `StreamHandle` -- returned by `complete_stream`, yields `StreamEvent` items
  as the model generates content.

## Features

- Full content block mapping: text, tool use, tool results, images, thinking.
- Prompt caching via `CacheControl` on messages and system prompts.
- `ToolChoice` support: `Auto`, `Any`, `Required`, `Specific(name)`.
- SSE streaming parsed from raw byte stream (no external SSE library).

## Usage

```rust,no_run
use neuron_provider_anthropic::Anthropic;
use neuron_types::{CompletionRequest, ContentBlock, Message, Provider, Role, SystemPrompt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Anthropic::new("sk-ant-...")
        .model("claude-sonnet-4-20250514");

    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("What is 2 + 2?".into())],
        }],
        system: Some(SystemPrompt::Text("You are a helpful assistant.".into())),
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

## Part of neuron

This crate is one block in the [neuron](https://github.com/axiston/neuron)
composable agent toolkit. It depends only on `neuron-types`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
