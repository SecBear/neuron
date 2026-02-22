# neuron-loop

Agentic while loop for neuron. Composes Provider + Tool + Context.

## Key types
- `AgentLoop<P, C>` — the core loop, generic over Provider and ContextStrategy
- `LoopConfig` — system prompt, max turns, parallel tool execution
- `AgentResult` — final response, messages, usage, turn count
- `TurnResult` — per-turn result enum for step-by-step iteration
- `StepIterator` — drives the loop one turn at a time
- `BoxedHook` — type-erased ObservabilityHook
- `BoxedDurable` — type-erased DurableContext

## Key methods
- `AgentLoop::run()` — drive to completion
- `AgentLoop::run_step()` — step-by-step iteration
- `AgentLoop::run_stream()` — streaming via provider's complete_stream
- `AgentLoop::add_hook()` — register observability hooks
- `AgentLoop::set_durability()` — enable durable execution

## Architecture
- Depends on `neuron-types` (traits) and `neuron-tool` (ToolRegistry)
- `neuron-context` is a dev-dependency only (for tests)
- RPITIT traits (Provider, ObservabilityHook, DurableContext) are type-erased
  via internal boxed wrappers for dyn-compatibility
- Hook firing uses per-event-type helper functions (not generic) because
  HookEvent is consumed on each call

## Conventions
- Flat file structure: config.rs, loop_impl.rs, step.rs
- All public types re-exported from lib.rs
- No `unwrap()` in library code
- `#[must_use]` on Result-returning run methods
- `thiserror` errors come from neuron-types (LoopError)
