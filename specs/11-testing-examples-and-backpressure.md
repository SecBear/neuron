# Testing, Examples, and Backpressure

## Purpose

Neuron should be provably composable.

The tests and examples are the backpressure that makes this architecture real.

## Required Example Suite (Core)

Neuron must have a small set of “proof of composition” examples that demonstrate the primitives working together:

- scheduled + state + signal (daily digest)
- multi-agent escalation + policy controls (triage)
- provider swap with parity invariants (provider parity)

These examples must share composition factories so wiring does not drift.

## Mock vs Real Paths

- Mock path must be deterministic and required in CI.
- Real path must be opt-in and env-gated.

## Current Implementation Status

- There are workspace tests under `tests/`.

Still required for “core complete”:

- a composition factory crate shared by examples/tests
- a failure/edge-case matrix test suite proving error paths and policy edge behavior

