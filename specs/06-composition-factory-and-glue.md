# Composition Factory and Glue

## The Question

Where does “glue” live?

- Inside orchestrator implementations?
- As a wrapper around Neuron?

## Answer (Specification)

Composition glue that wires agents, policies, and topology belongs with orchestration implementations (Layer 2), not in `layer0`.

Reason:

- It is inherently an orchestration concern (it chooses routing/topology/policy).
- It must be shared by examples and tests to prevent drift.
- It should remain optional and opinionated; `layer0` must not become a product DSL.

A separate wrapper product (outside Neuron) can exist to provide:

- YAML workflow DSL
- Slack/email delivery
- long-running job scheduling UX

That wrapper depends on Neuron and uses the composition factories.

## Required APIs

Neuron core should provide a small set of composition factory entrypoints that:

- accept a declarative spec (flow/topology + runtime profile)
- return a runnable orchestrator graph
- support mock and real profiles

## Current Implementation Status

On `redesign/v2`, there is no `neuron-orch-compose` factory crate yet.

This is required for “core complete” because it is the mechanism that proves composability and prevents example/test drift.

