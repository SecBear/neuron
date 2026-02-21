# agent-provider-ollama

Implements the `Provider` trait from `agent-types` for the Ollama Chat API.

## Structure

- `src/lib.rs` -- Public API, module declarations, re-exports
- `src/client.rs` -- `Ollama` struct, builder, and `Provider` impl
- `src/mapping.rs` -- Request/response JSON mapping
- `src/streaming.rs` -- NDJSON stream parsing and `StreamHandle` construction
- `src/error.rs` -- HTTP status -> `ProviderError` mapping

## Key design decisions

- Uses `reqwest` with `.bytes_stream()` for streaming; NDJSON is parsed line-by-line
- `async_stream::stream!` macro drives the NDJSON parser
- Ollama does not provide tool call IDs; we synthesize them with `uuid::Uuid::new_v4()`
- No authentication by default (Ollama is local)
- `keep_alive` field controls model memory residency ("5m", "0" to unload)
- `options` object carries `num_predict`, `temperature`, `top_p`, `stop`
- Tool definitions use the same format as OpenAI (Ollama adopted it)

## Testing

Tests use `wiremock` for integration tests and plain unit tests for mapping logic.
Run with: `cargo test`
