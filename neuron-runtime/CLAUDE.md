# neuron-runtime

Production runtime layer for neuron. Sessions, guardrails, durability,
sandboxing.

## Key types
- `Session`, `SessionState`, `SessionSummary` -- conversation session management
- `SessionStorage` trait -- persist/load sessions (RPITIT)
- `InMemorySessionStorage` -- Arc<RwLock<HashMap>> for testing
- `FileSessionStorage` -- one JSON file per session
- `InputGuardrail`, `OutputGuardrail` -- safety checks with Pass/Tripwire/Warn
- `GuardrailResult` -- Pass, Tripwire(reason), Warn(reason)
- `ErasedInputGuardrail`, `ErasedOutputGuardrail` -- type-erased guardrails for dyn dispatch
- `GuardrailHook` -- ObservabilityHook adapter that runs guardrails on PreLlmCall/PostLlmCall
- `LocalDurableContext<P>` -- passthrough DurableContext for local dev
- `Sandbox` trait, `NoOpSandbox` -- tool execution isolation
- `TracingHook` -- concrete ObservabilityHook using `tracing` crate

## Key patterns
- Provider and ContextStrategy passed as generics (RPITIT, not dyn-compatible)
- Guardrail traits type-erased via ErasedInputGuardrail/ErasedOutputGuardrail
- GuardrailHook wraps type-erased guardrails as an ObservabilityHook (builder pattern)
- LocalDurableContext wraps Arc<P> and Arc<ToolRegistry>

## Architecture
- Depends on neuron-types, neuron-tool
- No unwrap() in library code
- thiserror errors come from neuron-types

## Conventions
- Flat file structure: session.rs, guardrail.rs, guardrail_hook.rs, durable.rs, sandbox.rs, tracing_hook.rs
- All public types re-exported from lib.rs
- Doc comments on every public item
- #[must_use] on Result-returning functions
