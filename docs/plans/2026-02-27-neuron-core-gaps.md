# Neuron “Core Complete” Gaps (2026-02-27)

This note records the biggest remaining gaps to consider Neuron redesign/v2 “core complete”.

It is intentionally not queued yet (Brain-first priority).

## Biggest gaps

1. Reference effect execution pipeline (end-to-end semantics)
   - Spec: `specs/03-effects-and-execution-semantics.md`
   - Missing: a single reference engine proving delegate/handoff/signal/state effects work end-to-end

2. Orchestration semantics for workflow control
   - Spec: `specs/05-orchestration-core.md`
   - Missing: local `signal`/`query` semantics that are defined and test-proven (not noop/null)

3. Composition factory / glue layer
   - Spec: `specs/06-composition-factory-and-glue.md`
   - Missing: a reference composition story that wires graphs consistently (prevents example drift)

4. User-facing contract + compatibility story
   - Spec: `specs/02-layer0-protocol-contract.md`
   - Missing: semantics-focused contract docs + versioning/deprecation guidance

5. Docs/tutorial parity and matching examples
   - Spec: `specs/13-documentation-and-dx-parity.md`
   - Missing: progressive tutorial series + examples aligned to the current codebase

