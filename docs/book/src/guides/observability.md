# Observability

neuron provides observability through the `ObservabilityHook` trait and the
`neuron-otel` crate, which implements OpenTelemetry instrumentation following
the GenAI semantic conventions.

## The `ObservabilityHook` trait

The `ObservabilityHook` trait (defined in `neuron-types`) is the extension point
for logging, metrics, and telemetry. Hooks receive events at each step of the
agent loop and can observe or control execution.

```rust,ignore
pub trait ObservabilityHook: Send + Sync {
    fn on_event(&self, event: HookEvent<'_>) -> impl Future<Output = Result<HookAction, HookError>> + Send;
}
```

See the [agent loop guide](loop.md#observability-hooks) for details on hook
events and the `HookAction` enum.

## `neuron-otel` -- OpenTelemetry instrumentation

`neuron-otel` provides `OtelHook`, an `ObservabilityHook` implementation that
emits structured tracing spans using the OpenTelemetry GenAI semantic conventions
(`gen_ai.*` attributes).

### Quick example

```rust,ignore
use neuron_otel::OtelHook;
use neuron_loop::AgentLoop;

let agent = AgentLoop::builder(provider, context)
    .tools(registry)
    .hook(OtelHook::default())
    .build();
```

That's it. `OtelHook` emits spans for every LLM call, tool execution, and loop
iteration, with attributes following the `gen_ai.*` namespace.

### What gets traced

`OtelHook` emits spans at each hook event:

| Event | Span name | Key attributes |
|---|---|---|
| `LoopIteration` | `gen_ai.loop.iteration` | `gen_ai.loop.turn` |
| `PreLlmCall` / `PostLlmCall` | `gen_ai.chat` | `gen_ai.request.model`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`, `gen_ai.response.stop_reason` |
| `PreToolExecution` / `PostToolExecution` | `gen_ai.execute_tool` | `gen_ai.tool.name`, `gen_ai.tool.is_error` |
| `ContextCompaction` | `gen_ai.context.compaction` | `gen_ai.context.old_tokens`, `gen_ai.context.new_tokens` |

### Configuration

`OtelHook` uses the standard `tracing` crate subscriber model. Configure your
tracing pipeline as usual with `tracing-opentelemetry` and the OpenTelemetry
SDK:

```rust,ignore
use neuron_otel::OtelHook;
use opentelemetry::trace::TracerProvider;
use tracing_subscriber::prelude::*;

// Set up your OpenTelemetry pipeline (exporter, batch processor, etc.)
let tracer_provider = /* your OTel setup */;

// Install tracing-opentelemetry layer
tracing_subscriber::registry()
    .with(tracing_opentelemetry::layer().with_tracer(
        tracer_provider.tracer("neuron")
    ))
    .init();

// Add the hook to your agent
let agent = AgentLoop::builder(provider, context)
    .hook(OtelHook::default())
    .build();
```

`OtelHook` does not configure the OpenTelemetry pipeline itself -- it only
emits `tracing` spans. You bring your own exporter (Jaeger, OTLP, Zipkin, etc.)
and configure it through the standard OpenTelemetry SDK.

### GenAI semantic conventions

The span attributes follow the emerging
[OpenTelemetry GenAI semantic conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/)
specification. Key attributes include:

- `gen_ai.system` -- the provider system (e.g., `"anthropic"`, `"openai"`)
- `gen_ai.request.model` -- the model identifier
- `gen_ai.usage.input_tokens` -- input token count
- `gen_ai.usage.output_tokens` -- output token count
- `gen_ai.response.stop_reason` -- why the model stopped generating
- `gen_ai.tool.name` -- the name of the tool being called

### Using with `neuron-runtime`'s `TracingHook`

`neuron-runtime` also ships a `TracingHook` for basic `tracing` span emission.
`OtelHook` and `TracingHook` serve different purposes:

- **`TracingHook`** -- lightweight, emits simple `tracing` spans for local
  debugging. No GenAI semantic conventions. Ships with `neuron-runtime`.
- **`OtelHook`** -- full OpenTelemetry instrumentation with GenAI semantic
  conventions. Designed for production observability pipelines. Ships with
  `neuron-otel`.

You can use both simultaneously -- they are independent hooks:

```rust,ignore
use neuron_otel::OtelHook;
use neuron_runtime::TracingHook;

let agent = AgentLoop::builder(provider, context)
    .hook(TracingHook::default())  // Local debug logging
    .hook(OtelHook::default())     // Production OTel export
    .build();
```

## Installation

Add `neuron-otel` directly:

```toml
[dependencies]
neuron-otel = "*"
```

Or use the umbrella crate with the `otel` feature:

```toml
[dependencies]
neuron = { features = ["anthropic", "otel"] }
```

## API reference

- [`neuron_otel` on docs.rs](https://docs.rs/neuron-otel)
