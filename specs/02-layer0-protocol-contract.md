# Layer 0 Protocol Contract

## Purpose

`layer0` is the stability contract. It defines:

- protocol traits (object-safe)
- message/effect types that cross boundaries
- error vocabulary
- IDs and scopes
- secret source vocabulary

It must be easy for any implementation to adopt and must avoid coupling to any specific runtime.

## Protocol Traits

Layer 0 defines four primary protocol traits:

- `Operator`: one unit of agent work
- `Orchestrator`: dispatch/compose operators and manage workflow control surfaces
- `StateStore` + `StateReader`: persistence and retrieval
- `Environment`: isolated execution boundary

Layer 0 also defines cross-cutting governance interfaces:

- `Hook` (observation/intervention at hook points)
- lifecycle vocabulary types (`BudgetEvent`, `CompactionEvent`, `ObservableEvent`)

## Compatibility Rules

- Traits must remain object-safe and usable behind `dyn` for composition.
- Wire types must remain serde-serializable.
- Additive changes are preferred; breaking signature changes should be avoided until a planned breaking release.

## Current Implementation Status

Implemented in this repo:

- `layer0` exists and defines the above interfaces.

Still required to be considered “core complete”:

- A clear, user-facing contract doc describing semantics (not just trait signatures).
- A compatibility story for versioning and deprecation.

