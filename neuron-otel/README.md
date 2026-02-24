# neuron-otel

[![crates.io](https://img.shields.io/crates/v/neuron-otel.svg)](https://crates.io/crates/neuron-otel)
[![docs.rs](https://docs.rs/neuron-otel/badge.svg)](https://docs.rs/neuron-otel)
[![license](https://img.shields.io/crates/l/neuron-otel.svg)](LICENSE-MIT)

OpenTelemetry instrumentation for the neuron ecosystem using GenAI semantic
conventions. Implements the `ObservabilityHook` trait with `tracing` spans in the
`gen_ai.*` namespace, following the
[OTel GenAI specification](https://opentelemetry.io/docs/specs/semconv/gen-ai/).

## Installation

```sh
cargo add neuron-otel
```

## Key Types

- `OtelHook` -- an `ObservabilityHook` that emits `tracing` spans following OTel GenAI semantic conventions. Always returns `HookAction::Continue` (observe-only, never controls the loop).
- `OtelConfig` -- configuration for opt-in content capture (`capture_input`, `capture_output`). Both default to `false` for privacy.

## Span Hierarchy

`OtelHook` creates the following spans, mirroring the agentic loop lifecycle:

| Span Name | Created When | Key Attributes |
|---|---|---|
| `gen_ai.loop.iteration` | Each turn of the agent loop | `gen_ai.loop.turn` |
| `gen_ai.chat` | Each LLM completion call | `gen_ai.system`, `gen_ai.request.model`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens` |
| `gen_ai.execute_tool` | Each tool execution | `gen_ai.tool.name`, `gen_ai.tool.call_id` |
| `gen_ai.context.compaction` | Context compaction events | `gen_ai.compaction.old_tokens`, `gen_ai.compaction.new_tokens` |

## Usage

Create an `OtelHook` and register it with an `AgentLoop` via the builder's
`.hook()` method. Pair with `tracing-opentelemetry` and your preferred OTel
exporter to ship spans to Jaeger, Honeycomb, Datadog, or any OTLP-compatible
backend.

```rust,ignore
use neuron_otel::{OtelHook, OtelConfig};
use neuron_loop::AgentLoop;

// Default config: no content capture (privacy-safe)
let hook = OtelHook::default();

// Or opt in to capturing request/response content
let hook = OtelHook::new(OtelConfig {
    capture_input: true,
    capture_output: true,
});

let mut agent = AgentLoop::builder(provider, context)
    .hook(hook)
    .build();
```

## Privacy

By default, `OtelHook` does **not** capture message content in spans. Request
and response bodies may contain PII or sensitive data. To enable content capture,
explicitly set `capture_input` and/or `capture_output` to `true` in `OtelConfig`.
This is an opt-in design to prevent accidental data leakage into telemetry
backends.

## Part of neuron

This crate is part of [neuron](https://github.com/secbear/neuron), a
composable building-blocks library for AI agents in Rust.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
