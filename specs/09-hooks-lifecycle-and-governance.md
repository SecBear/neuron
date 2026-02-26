# Hooks, Lifecycle, and Governance

## Purpose

Hooks and lifecycle vocabulary provide controlled intervention and cross-layer coordination.

This is how you enforce budget, tool policy, redaction, audit, and observability without baking those concerns into every operator.

## Hooks

`layer0::Hook` defines:

- hook points (pre/post inference, tool use, exit checks)
- actions (continue, halt, skip tool, modify input/output)

Hook errors should not implicitly halt execution; a hook must explicitly choose `Halt`.

## Lifecycle Vocabulary

Lifecycle types define coordination events:

- budget events
- compaction events
- observable events

These are shared vocabulary types, not a separate lifecycle service.

## Current Implementation Status

- Hook traits exist in layer0.
- Hook registries exist in `neuron-hooks`.
- Policy/security hooks exist in `neuron-hook-security`.

Still required for “core complete”:

- explicit examples showing how orchestration consumes lifecycle vocab to coordinate compaction/budget
- tests for edge hook actions (skip tool, modify tool input/output) across the operator runtime

