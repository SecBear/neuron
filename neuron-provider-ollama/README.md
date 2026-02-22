# neuron-provider-ollama

Ollama provider for the neuron agent blocks ecosystem. Implements the `Provider`
trait from `neuron-types` against the Ollama Chat API, supporting both
synchronous completions and newline-delimited JSON (NDJSON) streaming.

Ollama runs locally and requires no API key or authentication. The default model
is `llama3.2`. The default base URL is `http://localhost:11434`.

## Key Types

- `Ollama` -- client struct with builder methods (`new`, `model`, `base_url`,
  `keep_alive`). Implements `Provider` from `neuron-types` and `Default`.
- `ProviderError` -- re-exported error type for all provider failures.
- `StreamHandle` -- returned by `complete_stream`, yields `StreamEvent` items
  as the model generates tokens.

## Features

- No authentication required -- designed for local Ollama instances.
- `keep_alive` control for model memory residency (`"5m"`, `"0"` to unload
  immediately).
- Tool call support using the OpenAI-compatible format that Ollama adopted.
- NDJSON streaming parsed line-by-line from the response byte stream.
- Tool call IDs synthesized via `uuid::Uuid::new_v4()` since Ollama does not
  provide them natively.

## Usage

```rust,no_run
use neuron_provider_ollama::Ollama;
use neuron_types::{CompletionRequest, ContentBlock, Message, Provider, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Ollama::new()
        .model("llama3.2")
        .keep_alive("5m");

    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("What is the capital of France?".into())],
        }],
        max_tokens: Some(256),
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
