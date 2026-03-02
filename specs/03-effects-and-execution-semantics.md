# Effects and Execution Semantics

## Purpose

Effects are the boundary between “reasoning” and “side effects.” Operators declare effects; outer layers decide how and when effects execute.

This is the key mechanism that makes Neuron composable.

## Effect Vocabulary

The `layer0::Effect` enum is the shared language for:

- state writes/deletes
- delegation and handoff to other agents
- signaling other workflows

## Required Semantics (Core)

Neuron is “core complete” only when there is a clear, test-proven definition of how effects are handled.

At minimum:

1. **WriteMemory/DeleteMemory**: must be executed against a selected `StateStore` with deterministic key/scope semantics.
2. **Delegate**: represents “run another agent with an input and return results to the current control flow.”
3. **Handoff**: represents “transfer responsibility to another agent,” potentially with different lifecycle semantics.
4. **Signal**: represents “fire-and-forget asynchronous message to a workflow.”

## Who Executes Effects?

Effects should be executed by orchestration/runtime glue, not by the operator itself.

- Local orchestration may execute them immediately.
- Durable orchestration may serialize and execute them inside workflow engines.

## Current Implementation Status

- Operators can emit effects in this repo (e.g. `neuron-op-react`).
- Local orchestrator exists (`neuron-orch-local`), but there is not yet a single “reference effect execution engine” that interprets effects end-to-end.

Required next step:

- Define a reference effect execution pipeline and prove it via examples/tests.

