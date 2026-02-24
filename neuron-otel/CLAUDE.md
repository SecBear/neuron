# neuron-otel

OpenTelemetry instrumentation for neuron using GenAI semantic conventions.
Implements `ObservabilityHook` with `tracing` spans in the `gen_ai.*` namespace.

## Key types

- `OtelHook` -- an `ObservabilityHook` that emits `tracing` spans following the OTel GenAI semantic conventions. Always returns `HookAction::Continue` (observe-only, never controls the loop).
- `OtelConfig` -- configuration for opt-in content capture (`capture_input`, `capture_output`). Both default to `false` for privacy.

## Key design decisions

- **Uses `tracing` spans, not OTel SDK directly.** Users bring their own `tracing-opentelemetry` subscriber to export spans. This keeps the crate lightweight and subscriber-agnostic.
- **GenAI semantic conventions** (`gen_ai.*` namespace) -- follows the [OTel GenAI spec](https://opentelemetry.io/docs/specs/semconv/gen-ai/) for attribute naming (`gen_ai.system`, `gen_ai.request.model`, `gen_ai.usage.input_tokens`, etc.).
- **Opt-in content capture** -- request/response message bodies are NOT included in spans by default. Set `capture_input` / `capture_output` to `true` explicitly. This prevents accidental PII leakage.
- **Observe-only hook** -- `OtelHook` always returns `HookAction::Continue`. It never skips or terminates the loop.
- **Span hierarchy**: `gen_ai.loop.iteration` (per turn), `gen_ai.chat` (LLM call), `gen_ai.execute_tool` (tool execution), `gen_ai.context.compaction` (compaction events).

## Dependencies

- `neuron-types` (workspace) -- `ObservabilityHook`, `HookEvent`, `HookAction`, `HookError`
- `tracing` (workspace) -- span creation and instrumentation

## Structure

```
neuron-otel/
    CLAUDE.md
    Cargo.toml
    src/
        lib.rs             # OtelHook, OtelConfig, ObservabilityHook impl
```

## Conventions

- Single-file crate (all types and impl in `lib.rs`)
- All public types re-exported from `lib.rs`
- `#[must_use]` on constructors
- No `unwrap()` in library code
- Doc comments on every public item
- `Default` impls for both `OtelHook` and `OtelConfig`
