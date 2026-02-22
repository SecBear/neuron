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

## Cancellation
The loop checks `ToolContext.cancellation_token` (from `tokio_util::sync::CancellationToken`)
at two points per iteration:
1. Top of the loop body (before the max turns check)
2. Before tool execution (after the LLM returns tool calls, before executing them)

If cancelled, `run()` returns `LoopError::Cancelled`, `StepIterator::next()` returns
`TurnResult::Error(LoopError::Cancelled)`, and `run_stream()` sends
`StreamEvent::Error` with "cancelled".

## Parallel tool execution
When `LoopConfig.parallel_tool_execution` is `true` and the LLM returns more than
one tool call, all calls execute concurrently via `futures::future::join_all`.
When `false` (the default), tool calls execute sequentially in order. The internal
`execute_single_tool` method handles pre/post hooks and durability routing for
each individual tool call.

Parallel execution applies to `run()` and `run_step()`. Streaming (`run_stream()`)
always executes tools sequentially (parallel streaming is a future enhancement).

## Compaction handling
- **Client-side**: via `ContextStrategy` — the loop calls `should_compact()` /
  `compact()` between turns when token usage is high
- **Server-side**: when the provider returns `StopReason::Compaction`, the loop
  continues automatically (doesn't treat it as a final response). The compacted
  content arrives in `ContentBlock::Compaction`.

## ToolError::ModelRetry
- When a tool returns `Err(ToolError::ModelRetry(hint))`, the loop converts
  it to a `ToolOutput` with `is_error: true` and the hint as content
- The model sees the hint and can retry with corrected arguments
- Does NOT propagate as `LoopError::Tool`

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
