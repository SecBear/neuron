# neuron-runtime

Production runtime layer for neuron. Sessions, sub-agents,
guardrails, durability, sandboxing.

## Key types
- `Session`, `SessionState`, `SessionSummary` -- conversation session management
- `SessionStorage` trait -- persist/load sessions (RPITIT)
- `InMemorySessionStorage` -- Arc<RwLock<HashMap>> for testing
- `FileSessionStorage` -- one JSON file per session
- `SubAgentConfig` -- system prompt, tool filter, max depth, max turns
- `SubAgentManager` -- register and spawn sub-agents
- `InputGuardrail`, `OutputGuardrail` -- safety checks with Pass/Tripwire/Warn
- `GuardrailResult` -- Pass, Tripwire(reason), Warn(reason)
- `LocalDurableContext<P>` -- passthrough DurableContext for local dev
- `Sandbox` trait, `NoOpSandbox` -- tool execution isolation

## Key patterns
- Provider and ContextStrategy passed as generics (RPITIT, not dyn-compatible)
- Guardrail traits type-erased via ErasedInputGuardrail/ErasedOutputGuardrail
- SubAgentManager::spawn takes provider/context as generic params
- LocalDurableContext wraps Arc<P> and Arc<ToolRegistry>

## Architecture
- Depends on neuron-types, neuron-tool, neuron-loop
- neuron-context is a dev-dependency only (for tests)
- No unwrap() in library code
- thiserror errors come from neuron-types

## Conventions
- Flat file structure: session.rs, sub_agent.rs, guardrail.rs, durable.rs, sandbox.rs
- All public types re-exported from lib.rs
- Doc comments on every public item
- #[must_use] on Result-returning functions
