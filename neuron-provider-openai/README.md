# neuron-provider-openai

OpenAI provider for the neuron agent blocks ecosystem. Implements the `Provider`
and `EmbeddingProvider` traits from `neuron-types` against the OpenAI Chat
Completions and Embeddings APIs, supporting synchronous completions, server-sent
event (SSE) streaming, and text embeddings.

The default completion model is `gpt-4o`. The default embedding model is
`text-embedding-3-small`. The default base URL is `https://api.openai.com`.
All can be overridden with the builder API. The `base_url` override also makes
this client usable with Azure OpenAI and compatible third-party endpoints.

## Key Types

- `OpenAi` -- client struct with builder methods (`new`, `model`, `base_url`,
  `organization`). Implements `Provider` and `EmbeddingProvider` from `neuron-types`.
- `ProviderError` -- re-exported error type for provider failures.
- `EmbeddingError` -- re-exported error type for embedding failures.
- `EmbeddingRequest`, `EmbeddingResponse` -- re-exported embedding types.
- `StreamHandle` -- returned by `complete_stream`, yields `StreamEvent` items
  as the model generates tokens.

## Features

- Full message mapping: text, tool calls, tool results, images.
- Organization header support for multi-org OpenAI accounts.
- `ToolChoice` support: `Auto`, `Any`, `Required`, `Specific(name)`.
- SSE streaming parsed from raw byte stream with `data: [DONE]` sentinel
  handling.
- Usage statistics included in streaming responses via `stream_options`.
- `EmbeddingProvider` support for generating text embeddings with optional
  dimension control.

## Usage

### Completions

```rust,no_run
use neuron_provider_openai::OpenAi;
use neuron_types::{CompletionRequest, ContentBlock, Message, Provider, Role, SystemPrompt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = OpenAi::new("sk-...")
        .model("gpt-4o")
        .organization("org-...");

    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Explain Rust's ownership model.".into())],
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

### Embeddings

```rust,no_run
use neuron_provider_openai::OpenAi;
use neuron_types::{EmbeddingProvider, EmbeddingRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = OpenAi::new("sk-...");

    let request = EmbeddingRequest {
        input: vec!["Hello world".to_string(), "Rust is great".to_string()],
        dimensions: Some(256),
        ..Default::default()
    };

    let response = provider.embed(request).await?;

    println!("Model: {}", response.model);
    println!("Embeddings: {} vectors", response.embeddings.len());
    println!("Dimensions: {}", response.embeddings[0].len());
    println!("Usage: {} tokens", response.usage.total_tokens);
    Ok(())
}
```

## Part of neuron

This crate is one block in the [neuron](https://github.com/secbear/neuron)
composable agent toolkit. It depends only on `neuron-types`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
