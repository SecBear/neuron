# neuron-provider-ollama

[![crates.io](https://img.shields.io/crates/v/neuron-provider-ollama.svg)](https://crates.io/crates/neuron-provider-ollama)
[![docs.rs](https://docs.rs/neuron-provider-ollama/badge.svg)](https://docs.rs/neuron-provider-ollama)
[![license](https://img.shields.io/crates/l/neuron-provider-ollama.svg)](LICENSE-MIT)

Ollama provider for the neuron agent blocks ecosystem. Implements the
[`Provider`](https://docs.rs/neuron-types/latest/neuron_types/trait.Provider.html)
trait from `neuron-types` against the Ollama Chat API, supporting both
synchronous completions and newline-delimited JSON (NDJSON) streaming.

Ollama runs locally and requires no API key or authentication. The default model
is `llama3.2`. The default base URL is `http://localhost:11434`.

## Installation

```sh
cargo add neuron-provider-ollama
```

## Key Types

- `Ollama` -- client struct with builder methods (`new`, `from_env`, `model`,
  `base_url`, `keep_alive`). Implements `Provider` from `neuron-types` and `Default`.
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
- **Tool support varies by model** — not all Ollama models support function
  calling. Check the [Ollama model library](https://ollama.com/library) for
  which models support tools.

## Usage

```rust,no_run
use neuron_provider_ollama::Ollama;
use neuron_types::{CompletionRequest, Message, Provider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Default constructor (no auth needed for local Ollama)
    let provider = Ollama::new()
        .model("llama3.2")
        .keep_alive("5m");

    // Or from environment (reads OLLAMA_HOST if set, defaults to localhost:11434)
    let provider = Ollama::from_env()?;

    let request = CompletionRequest {
        messages: vec![Message::user("What is the capital of France?")],
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
