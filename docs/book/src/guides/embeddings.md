# Embeddings

neuron provides a provider-agnostic `EmbeddingProvider` trait for generating
text embeddings, with an OpenAI implementation in `neuron-provider-openai`.

## Quick Example

```rust,ignore
use neuron_provider_openai::OpenAi;
use neuron_types::{EmbeddingProvider, EmbeddingRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = OpenAi::from_env()?;

    let response = client.embed(EmbeddingRequest {
        model: "text-embedding-3-small".to_string(),
        input: vec![
            "Rust is a systems programming language.".to_string(),
            "Python is great for scripting.".to_string(),
        ],
        dimensions: None,
        ..Default::default()
    }).await?;

    println!("Got {} embeddings", response.embeddings.len());
    println!("First vector has {} dimensions", response.embeddings[0].len());
    println!("Tokens used: {}", response.usage.total_tokens);
    Ok(())
}
```

## API Walkthrough

### EmbeddingProvider Trait

The trait is defined in `neuron-types` and kept separate from `Provider`
because not all embedding models support chat completions and not all chat
providers support embeddings. Implement both on a single struct when a provider
supports both capabilities.

```rust,ignore
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, EmbeddingError>;
}
```

The trait uses RPITIT (return position impl trait in trait) and is not
object-safe. Use generics `<E: EmbeddingProvider>` for composition.

### EmbeddingRequest

```rust,ignore
pub struct EmbeddingRequest {
    /// The embedding model (e.g. "text-embedding-3-small").
    pub model: String,
    /// Text inputs to embed. Multiple strings are batched into one API call.
    pub input: Vec<String>,
    /// Optional output dimensionality (not all models support this).
    pub dimensions: Option<usize>,
    /// Provider-specific extra fields forwarded verbatim.
    pub extra: HashMap<String, serde_json::Value>,
}
```

- **`model`** -- the model identifier. The OpenAI implementation defaults to
  `text-embedding-3-small` when this is empty.
- **`input`** -- a batch of strings. Each string produces one embedding vector
  in the response. Batching multiple inputs in a single request is more
  efficient than making separate calls.
- **`dimensions`** -- reduces the output dimensionality when supported by the
  model (e.g., OpenAI's `text-embedding-3-small` supports 256, 512, or 1536).
- **`extra`** -- a map of provider-specific fields merged directly into the
  request body. Useful for options not covered by the common fields.

### EmbeddingResponse

```rust,ignore
pub struct EmbeddingResponse {
    /// One embedding vector per input string, in the same order.
    pub embeddings: Vec<Vec<f32>>,
    /// The model that generated the embeddings.
    pub model: String,
    /// Token usage statistics.
    pub usage: EmbeddingUsage,
}
```

The `embeddings` vector is always in the same order as the `input` vector in
the request. Each inner `Vec<f32>` is a dense floating-point embedding.

### EmbeddingUsage

```rust,ignore
pub struct EmbeddingUsage {
    /// Number of tokens in the input.
    pub prompt_tokens: usize,
    /// Total tokens consumed.
    pub total_tokens: usize,
}
```

### EmbeddingError

All embedding operations return `Result<_, EmbeddingError>`. The variants are:

| Variant | Description | Retryable? |
|---------|-------------|------------|
| `Authentication(String)` | Invalid API key or forbidden | No |
| `RateLimit { retry_after }` | Provider rate limit hit | Yes |
| `InvalidRequest(String)` | Bad model name, empty input, etc. | No |
| `Network(source)` | Connection failure, DNS error | Yes |
| `Other(source)` | Catch-all for unexpected errors | Depends |

Use `error.is_retryable()` to decide whether to retry:

```rust,ignore
match client.embed(request).await {
    Ok(response) => { /* use embeddings */ }
    Err(e) if e.is_retryable() => { /* back off and retry */ }
    Err(e) => { /* terminal error, report to user */ }
}
```

## OpenAI Implementation

`neuron-provider-openai` implements `EmbeddingProvider` on the same `OpenAi`
struct that implements `Provider`. No additional setup is needed -- the
embedding calls reuse the same API key, base URL, and HTTP client.

```rust,ignore
use neuron_provider_openai::OpenAi;
use neuron_types::{EmbeddingProvider, EmbeddingRequest};

// Same client for both chat completions and embeddings
let client = OpenAi::new("sk-...")
    .base_url("https://api.openai.com");

// Chat completion
let chat_response = client.complete(completion_request).await?;

// Embedding
let embed_response = client.embed(EmbeddingRequest {
    model: "text-embedding-3-small".to_string(),
    input: vec!["Hello world".to_string()],
    ..Default::default()
}).await?;
```

### Default Model

When `EmbeddingRequest.model` is empty, the OpenAI implementation defaults to
`text-embedding-3-small`.

### Controlling Dimensions

Use the `dimensions` field to reduce output size. Smaller embeddings use less
storage and are faster to compare, at the cost of some accuracy:

```rust,ignore
let response = client.embed(EmbeddingRequest {
    model: "text-embedding-3-small".to_string(),
    input: vec!["hello".to_string()],
    dimensions: Some(256), // Default is 1536 for this model
    ..Default::default()
}).await?;

assert_eq!(response.embeddings[0].len(), 256);
```

### Provider-Specific Options

Pass extra fields that the OpenAI API supports but neuron does not model
explicitly:

```rust,ignore
use std::collections::HashMap;

let mut extra = HashMap::new();
extra.insert("user".to_string(), serde_json::json!("user-123"));

let response = client.embed(EmbeddingRequest {
    model: "text-embedding-3-large".to_string(),
    input: vec!["text to embed".to_string()],
    extra,
    ..Default::default()
}).await?;
```

## Implementing a Custom EmbeddingProvider

To add embedding support for a new provider, implement the trait in your
provider crate:

```rust,ignore
use std::future::Future;
use neuron_types::{EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, EmbeddingError};

struct MyEmbeddingProvider { /* ... */ }

impl EmbeddingProvider for MyEmbeddingProvider {
    fn embed(
        &self,
        request: EmbeddingRequest,
    ) -> impl Future<Output = Result<EmbeddingResponse, EmbeddingError>> + Send {
        async move {
            // Call your embedding API
            let vectors = call_my_api(&request.input).await?;

            Ok(EmbeddingResponse {
                embeddings: vectors,
                model: request.model,
                usage: EmbeddingUsage {
                    prompt_tokens: 0,
                    total_tokens: 0,
                },
            })
        }
    }
}
```

## API Docs

Full API documentation:
- Trait and types: [neuron-types on docs.rs](https://docs.rs/neuron-types)
- OpenAI implementation: [neuron-provider-openai on docs.rs](https://docs.rs/neuron-provider-openai)
