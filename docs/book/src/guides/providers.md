# Providers

Provider crates implement the `Provider` trait from `neuron-types`, giving you
a uniform interface to call any LLM. neuron ships three provider crates --
Anthropic, OpenAI, and Ollama -- each in its own crate following the serde
pattern: trait in core, implementation in a satellite.

## Quick example

```rust,ignore
use neuron_provider_anthropic::Anthropic;
use neuron_types::{CompletionRequest, Message, Provider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Anthropic::from_env()?;

    let request = CompletionRequest {
        messages: vec![Message::user("What is Rust?")],
        max_tokens: Some(256),
        ..Default::default()
    };

    let response = provider.complete(request).await?;
    println!("{}", response.message.content[0]); // ContentBlock::Text(...)
    println!("Tokens: {} in, {} out", response.usage.input_tokens, response.usage.output_tokens);
    Ok(())
}
```

## The `Provider` trait

```rust,ignore
pub trait Provider: Send + Sync {
    fn complete(&self, request: CompletionRequest)
        -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send;

    fn complete_stream(&self, request: CompletionRequest)
        -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send;
}
```

Key design points:

- Uses RPITIT (return position impl trait in trait) -- Rust 2024 native async.
  No `#[async_trait]` needed.
- **Not object-safe by design.** Use generics `<P: Provider>` to compose.
  This avoids the overhead of boxing futures while keeping the API clean.
- `complete()` returns a full `CompletionResponse` with the message, token
  usage, and stop reason.
- `complete_stream()` returns a `StreamHandle` whose `receiver` field is a
  `tokio::sync::mpsc::Receiver<StreamEvent>` that yields text deltas, tool
  use blocks, usage stats, and a final `MessageComplete` event.

## Anthropic (`neuron-provider-anthropic`)

Client for the Anthropic Messages API.

### Construction

```rust,ignore
use neuron_provider_anthropic::Anthropic;

// From environment variable (ANTHROPIC_API_KEY)
let provider = Anthropic::from_env()?;

// With explicit API key
let provider = Anthropic::new("sk-ant-...");

// Builder-style configuration
let provider = Anthropic::new("sk-ant-...")
    .model("claude-opus-4-5")
    .base_url("https://api.anthropic.com");
```

### Configuration

| Method | Default | Description |
|---|---|---|
| `new(api_key)` | -- | Create with explicit key |
| `from_env()` | -- | Read `ANTHROPIC_API_KEY` from environment |
| `.model(name)` | `claude-sonnet-4-20250514` | Default model when request has empty model field |
| `.base_url(url)` | `https://api.anthropic.com` | Override for proxies or testing |

### Features

- Full content block mapping: text, thinking, tool use/result, images, documents, compaction
- Server-side context management via `ContextManagement` request field
- SSE streaming with manual parser
- Cache control on system prompts and tool definitions
- `ToolChoice::Required` maps to Anthropic's `{"type": "any"}`

## OpenAI (`neuron-provider-openai`)

Client for the OpenAI Chat Completions API. Also implements `EmbeddingProvider`
for the Embeddings API.

### Construction

```rust,ignore
use neuron_provider_openai::OpenAi;

// From environment variable (OPENAI_API_KEY, optional OPENAI_ORG_ID)
let provider = OpenAi::from_env()?;

// With explicit API key
let provider = OpenAi::new("sk-...");

// Builder-style configuration
let provider = OpenAi::new("sk-...")
    .model("gpt-4o")
    .base_url("https://api.openai.com")
    .organization("org-...");
```

### Configuration

| Method | Default | Description |
|---|---|---|
| `new(api_key)` | -- | Create with explicit key |
| `from_env()` | -- | Read `OPENAI_API_KEY` (and optional `OPENAI_ORG_ID`) |
| `.model(name)` | `gpt-4o` | Default model |
| `.base_url(url)` | `https://api.openai.com` | Override for Azure, proxies, or testing |
| `.organization(org)` | None | Sent as `OpenAI-Organization` header |

### Embeddings

`OpenAi` also implements the `EmbeddingProvider` trait:

```rust,ignore
use neuron_types::{EmbeddingProvider, EmbeddingRequest};
use neuron_provider_openai::OpenAi;

let provider = OpenAi::from_env()?;

let request = EmbeddingRequest {
    model: "text-embedding-3-small".into(),
    input: vec!["Hello world".into(), "Goodbye world".into()],
    dimensions: Some(256),  // optional dimension reduction
    ..Default::default()
};

let response = provider.embed(request).await?;
// response.embeddings: Vec<Vec<f32>> -- one vector per input
// response.usage: EmbeddingUsage { prompt_tokens, total_tokens }
```

The `EmbeddingProvider` trait is separate from `Provider` because not all
embedding models support chat completion and vice versa. The `OpenAi` struct
implements both.

### Features

- SSE streaming with `data: [DONE]` sentinel
- System prompts mapped to `role: "developer"` (OpenAI convention)
- Tool calls in `choices[0].message.tool_calls` array format
- `ToolChoice::Required` maps to OpenAI's `"required"`
- Stream options include `include_usage: true` for token stats

## Ollama (`neuron-provider-ollama`)

Client for the Ollama Chat API. Designed for local models with no authentication
required by default.

### Construction

```rust,ignore
use neuron_provider_ollama::Ollama;

// Default: localhost:11434, no auth
let provider = Ollama::new();

// From environment (reads OLLAMA_HOST if set)
let provider = Ollama::from_env()?;

// Builder-style configuration
let provider = Ollama::new()
    .model("llama3.2")
    .base_url("http://remote-host:11434")
    .keep_alive("5m");
```

### Configuration

| Method | Default | Description |
|---|---|---|
| `new()` | -- | Create with defaults (no auth needed) |
| `from_env()` | -- | Read `OLLAMA_HOST` for base URL |
| `.model(name)` | `llama3.2` | Default model |
| `.base_url(url)` | `http://localhost:11434` | Override for remote instances |
| `.keep_alive(duration)` | None (server default) | Model memory residency (`"5m"`, `"0"` to unload) |

### Features

- NDJSON streaming (newline-delimited JSON, not SSE)
- No authentication by default (Ollama runs locally)
- Synthesizes tool call IDs with UUID (Ollama does not provide them natively)
- `keep_alive` controls how long the model stays in GPU memory
- Tool definitions use the same format as OpenAI (adopted by Ollama)

## Provider + AgentLoop integration

The most common use of a provider is plugging it into an `AgentLoop` -- the
commodity agentic while-loop that handles tool dispatch, context management, and
multi-turn conversation. Here is a complete, self-contained example using OpenAI:

```rust,ignore
use neuron_context::SlidingWindowStrategy;
use neuron_loop::AgentLoop;
use neuron_provider_openai::OpenAi;
use neuron_tool::ToolRegistry;
use neuron_types::ToolContext;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Build the provider
    let provider = OpenAi::from_env()?.model("gpt-4o");

    // 2. Choose a context strategy (keep last 20 messages, up to 100k tokens)
    let context = SlidingWindowStrategy::new(20, 100_000);

    // 3. Create a tool registry (empty here -- add tools as needed)
    let tools = ToolRegistry::new();

    // 4. Assemble the agent loop
    let mut agent = AgentLoop::builder(provider, context)
        .system_prompt("You are a helpful assistant.")
        .max_turns(10)
        .tools(tools)
        .build();

    // 5. Run with a plain text message
    let ctx = ToolContext::default();
    let result = agent.run_text("Hello!", &ctx).await?;
    println!("{}", result.response);
    println!("Turns: {}, Tokens: {} in / {} out",
        result.turns, result.usage.input_tokens, result.usage.output_tokens);

    Ok(())
}
```

This pattern is identical for every `Provider` implementation. Replace
`OpenAi::from_env()?` with `Anthropic::from_env()?` or `Ollama::from_env()?`
and nothing else changes -- the builder, context strategy, tool registry, and
run call all stay the same.

## Implementing a custom provider

If none of the built-in provider crates fit your needs -- for example, you want
to integrate a proprietary LLM service or a local inference engine -- you can
implement the `Provider` trait directly:

```rust,ignore
use std::future::Future;
use neuron_types::{
    CompletionRequest, CompletionResponse, ContentBlock, Message,
    Provider, ProviderError, Role, StopReason, StreamHandle, TokenUsage,
};

/// A minimal provider that calls a hypothetical LLM API.
pub struct MyProvider {
    api_key: String,
}

impl MyProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }
}

impl Provider for MyProvider {
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        let api_key = self.api_key.clone();
        async move {
            // In a real implementation, serialize `request` and send it
            // to your LLM API using reqwest, hyper, etc.
            let response_text = format!(
                "Echo: {}",
                request.messages.last().map(|m| m.content[0].to_string()).unwrap_or_default()
            );

            Ok(CompletionResponse {
                id: "resp-001".to_string(),
                model: "my-model-v1".to_string(),
                message: Message::assistant(response_text),
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: 20,
                    ..Default::default()
                },
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        async {
            Err(ProviderError::InvalidRequest(
                "streaming not supported".to_string(),
            ))
        }
    }
}
```

### `CompletionResponse` fields

| Field | Type | Meaning |
|---|---|---|
| `id` | `String` | Unique identifier from the LLM API (e.g., `"msg_01XFDUDYJgAACzvnptvVoYEL"`). Used for logging and deduplication. |
| `model` | `String` | The model name that processed the request (e.g., `"gpt-4o"`, `"claude-sonnet-4-20250514"`). |
| `message` | `Message` | The assistant response. Construct with `Message::assistant("text")` or manually as `Message { role: Role::Assistant, content: vec![ContentBlock::Text(...)] }`. |
| `usage` | `TokenUsage` | Token counts for the request. Use `..Default::default()` for optional fields (`cache_read_tokens`, `cache_creation_tokens`, `reasoning_tokens`, `iterations`) when your API does not report them. |
| `stop_reason` | `StopReason` | Why generation stopped. `EndTurn` for normal completion, `ToolUse` when the model wants to call tools, `MaxTokens` if the response was truncated by the token limit. |

The `Provider` trait requires `WasmCompatSend + WasmCompatSync`, which are
equivalent to `Send + Sync` on native targets. On WASM, these bounds are
automatically satisfied so your provider can compile for both environments.

## Error handling

All providers map errors to `ProviderError`, which classifies errors as retryable
or terminal:

```rust,ignore
use neuron_types::ProviderError;

match provider.complete(request).await {
    Ok(response) => { /* ... */ }
    Err(e) if e.is_retryable() => {
        // Network, RateLimit, ModelLoading, Timeout, ServiceUnavailable
        // Safe to retry with backoff
    }
    Err(e) => {
        // Authentication, InvalidRequest, ModelNotFound, InsufficientResources
        // Do not retry -- fix the root cause
    }
}
```

### `ProviderError` variants

| Variant | Retryable | Description |
|---|---|---|
| `Network(source)` | Yes | Connection reset, DNS failure |
| `RateLimit { retry_after }` | Yes | Provider rate limit hit |
| `ModelLoading(msg)` | Yes | Cold start, model still loading |
| `Timeout(duration)` | Yes | Request timed out |
| `ServiceUnavailable(msg)` | Yes | Temporary provider outage |
| `Authentication(msg)` | No | Bad API key or permissions |
| `InvalidRequest(msg)` | No | Malformed request |
| `ModelNotFound(msg)` | No | Requested model does not exist |
| `InsufficientResources(msg)` | No | Quota or limit exceeded |
| `StreamError(msg)` | No | Error during streaming |
| `Other(source)` | No | Catch-all |

neuron does not include built-in retry logic. Use `is_retryable()` with your
own retry strategy, `tower` middleware, or a durable execution engine.

## Streaming

All providers support streaming via `complete_stream()`, which returns a
`StreamHandle`:

```rust,ignore
use futures::StreamExt;
use neuron_types::StreamEvent;

let handle = provider.complete_stream(request).await?;
let mut stream = handle.receiver;

while let Some(event) = stream.recv().await {
    match event {
        StreamEvent::TextDelta(text) => print!("{text}"),
        StreamEvent::ToolUse { id, name, input } => { /* tool call */ }
        StreamEvent::Usage(usage) => { /* token stats */ }
        StreamEvent::MessageComplete(message) => { /* final assembled message */ }
        StreamEvent::Error(err) => { /* stream error */ }
        _ => {}
    }
}
```

The transport differs by provider:

| Provider | Transport | Format |
|---|---|---|
| Anthropic | Server-Sent Events (SSE) | `event:` + `data:` lines |
| OpenAI | Server-Sent Events (SSE) | `data:` lines, `data: [DONE]` sentinel |
| Ollama | NDJSON | One JSON object per line |

## Swapping providers

Because all providers implement the same `Provider` trait, swapping is a
one-line change:

```rust,ignore
use neuron_context::SlidingWindowStrategy;
use neuron_loop::AgentLoop;

// Switch from Anthropic...
// let provider = Anthropic::from_env()?;

// ...to OpenAI:
let provider = OpenAi::from_env()?;

// Everything else stays the same
let agent = AgentLoop::builder(provider, SlidingWindowStrategy::new(20, 100_000))
    .system_prompt("You are a helpful assistant.")
    .build();
```

The model field in `CompletionRequest` defaults to empty, which makes the
provider use its configured default model. Set it explicitly when you need
a specific model within a run.

## API reference

- [`neuron_provider_anthropic` on docs.rs](https://docs.rs/neuron-provider-anthropic)
- [`neuron_provider_openai` on docs.rs](https://docs.rs/neuron-provider-openai)
- [`neuron_provider_ollama` on docs.rs](https://docs.rs/neuron-provider-ollama)
- [`Provider` trait in `neuron_types`](https://docs.rs/neuron-types/latest/neuron_types/trait.Provider.html)
- [`EmbeddingProvider` trait in `neuron_types`](https://docs.rs/neuron-types/latest/neuron_types/trait.EmbeddingProvider.html)
