# neuron-provider-anthropic

Implements the `Provider` trait from `neuron-types` for the Anthropic Messages API.

## Structure

- `src/lib.rs` — Public API, module declarations, re-exports
- `src/client.rs` — `Anthropic` struct, builder (`new`, `from_env`), and `Provider` impl
- `src/mapping.rs` — Request/response JSON mapping
- `src/streaming.rs` — SSE stream parsing and `StreamHandle` construction
- `src/error.rs` — HTTP status → `ProviderError` mapping

## Key design decisions

- Uses `reqwest` with `.bytes_stream()` for streaming; SSE is parsed manually
- `async_stream::stream!` macro drives the SSE parser state machine
- All content block types are mapped bidirectionally (including `Compaction`)
- Server-side context management: maps `CompletionRequest.context_management`
  to Anthropic's `context` object with `budget_tokens` and compaction `type`
- Parses `StopReason::Compaction` and `ContentBlock::Compaction` from server
- Per-iteration token usage parsed into `TokenUsage.iterations: Vec<UsageIteration>`
- Cache control, tool definitions, system prompts all support Anthropic's current API
- `ToolChoice::Required` maps to Anthropic's `{"type": "any"}` (their term for it)

## Testing

Tests use `wiremock` for integration tests and plain unit tests for mapping logic.
Run with: `cargo test`
