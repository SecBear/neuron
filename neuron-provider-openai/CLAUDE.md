# neuron-provider-openai

Implements the `Provider` and `EmbeddingProvider` traits from `neuron-types`
for the OpenAI Chat Completions and Embeddings APIs.

## Structure

- `src/lib.rs` — Public API, module declarations, re-exports
- `src/client.rs` — `OpenAi` struct, builder, and `Provider` impl
- `src/embeddings.rs` — `EmbeddingProvider` impl for OpenAI Embeddings API
- `src/mapping.rs` — Request/response JSON mapping
- `src/streaming.rs` — SSE stream parsing and `StreamHandle` construction
- `src/error.rs` — HTTP status -> `ProviderError` mapping

## Key design decisions

- Uses `reqwest` with `.bytes_stream()` for streaming; SSE is parsed manually
- `async_stream::stream!` macro drives the SSE parser state machine
- OpenAI uses `role: "developer"` for system prompts (not "system")
- Tool calls are in `choices[0].message.tool_calls` array format
- Tool results use `role: "tool"` with `tool_call_id`
- `ToolChoice::Required` maps to OpenAI's `"required"` (not "any" like Anthropic)
- Streaming uses `data: [DONE]` sentinel

## Embedding support

- POSTs to `{base_url}/v1/embeddings` with `encoding_format: "float"`
- Default model: `text-embedding-3-small` when `EmbeddingRequest.model` is empty
- Error mapping: 401/403 -> Authentication, 429 -> RateLimit, 400/404 -> InvalidRequest
- Reuses the same `OpenAi` client (api_key, base_url, reqwest::Client)
- Optional `dimensions` field for controlling output embedding size
- `extra` fields merged into the JSON body for provider-specific options

## Testing

Tests use `wiremock` for integration tests and plain unit tests for mapping logic.
Run with: `cargo test`
