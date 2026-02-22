# neuron-provider-anthropic

[![crates.io](https://img.shields.io/crates/v/neuron-provider-anthropic.svg)](https://crates.io/crates/neuron-provider-anthropic)
[![docs.rs](https://docs.rs/neuron-provider-anthropic/badge.svg)](https://docs.rs/neuron-provider-anthropic)
[![license](https://img.shields.io/crates/l/neuron-provider-anthropic.svg)](LICENSE-MIT)

Anthropic Claude provider for the neuron agent blocks ecosystem. Implements the
[`Provider`](https://docs.rs/neuron-types/latest/neuron_types/trait.Provider.html)
trait from `neuron-types` against the Anthropic Messages API,
supporting both synchronous completions and server-sent event (SSE) streaming.

The default model is `claude-sonnet-4-20250514`. The default base URL is
`https://api.anthropic.com`. Both can be overridden with the builder API.

## Installation

```sh
cargo add neuron-provider-anthropic
```

## Key Types

- `Anthropic` -- client struct with builder methods (`new`, `from_env`, `model`,
  `base_url`). Implements `Provider` from `neuron-types`.
- `ProviderError` -- re-exported error type for all provider failures (auth,
  rate limit, server errors).
- `StreamHandle` -- returned by `complete_stream`, yields `StreamEvent` items
  as the model generates content.

## Features

- Full content block mapping: text, tool use, tool results, images, thinking, compaction.
- Server-side context management via `ContextManagement` request field.
- Per-iteration token tracking via `UsageIteration` during compaction.
- Prompt caching via `CacheControl` on messages and system prompts.
- `ToolChoice` support: `Auto`, `Any`, `Required`, `Specific(name)`.
- SSE streaming parsed from raw byte stream (no external SSE library).

## Usage

```rust,no_run
use neuron_provider_anthropic::Anthropic;
use neuron_types::{CompletionRequest, Message, Provider, SystemPrompt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // From an explicit API key
    let provider = Anthropic::new("sk-ant-...")
        .model("claude-sonnet-4-20250514");

    // Or from the ANTHROPIC_API_KEY environment variable
    let provider = Anthropic::from_env()?;

    let request = CompletionRequest {
        messages: vec![Message::user("What is 2 + 2?")],
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

### Extended thinking

Enable Claude's extended thinking to let the model reason before responding:

```rust,ignore
use neuron_types::{CompletionRequest, ThinkingConfig};

let request = CompletionRequest {
    thinking: Some(ThinkingConfig::Enabled { budget_tokens: 10_000 }),
    ..Default::default()
};
// The response will include ContentBlock::Thinking blocks with the model's reasoning
```

### Server-side context management

Anthropic supports server-side context compaction. Set the `context_management`
field on `CompletionRequest` to enable it. When the server compacts, the response
includes `StopReason::Compaction` and `ContentBlock::Compaction` — the agent loop
continues automatically on the next iteration.

Per-iteration token usage is available in `TokenUsage.iterations`:

```rust,ignore
use neuron_types::{CompletionRequest, ContextManagement, ContextEdit};

let request = CompletionRequest {
    context_management: Some(ContextManagement {
        edits: vec![ContextEdit::Compact {
            strategy: "compact_20260112".into(),
        }],
    }),
    ..Default::default()
};
```

### Error handling

Each provider defines its own `ProviderError` type. If you're using multiple
providers, pattern-match on the specific provider's error rather than expecting
a shared error type across providers.

## Part of neuron

This crate is one block in the [neuron](https://github.com/secbear/neuron)
composable agent toolkit. It depends only on `neuron-types`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
