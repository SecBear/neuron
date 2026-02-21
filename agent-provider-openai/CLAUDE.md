# agent-provider-openai

Implements the `Provider` trait from `agent-types` for the OpenAI Chat Completions API.

## Structure

- `src/lib.rs` — Public API, module declarations, re-exports
- `src/client.rs` — `OpenAi` struct, builder, and `Provider` impl
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

## Testing

Tests use `wiremock` for integration tests and plain unit tests for mapping logic.
Run with: `cargo test`
