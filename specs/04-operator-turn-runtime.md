# Operator (Turn) Runtime

## Purpose

The operator runtime is where the agent “thinks and acts.” It is the inner loop.

In Neuron, this is implemented by crates like `neuron-op-react` and `neuron-op-single-shot` using provider implementations and tool/context infrastructure.

## Required Capabilities

Core capabilities expected from turn/operator implementations:

- accept `OperatorInput` with triggers/config/metadata
- assemble context using a `StateReader` (read-only state)
- call a provider model
- execute tools
- emit `OperatorOutput` with:
  - message content
  - exit reason
  - metadata (tokens, cost, turns, timing)
  - declared effects

## Exit Reasons

Exit reasons must be explicit and stable because orchestration uses them to decide what happens next (retry, downgrade, halt, etc.).

## Current Implementation Status

- `neuron-op-single-shot` exists and is functional.
- `neuron-op-react` exists and emits effects.

Still required for “core complete”:

- stronger documentation/examples for building custom operators
- explicit contracts on which effects are emitted in which situations

