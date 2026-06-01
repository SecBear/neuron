# Skelegent V2 Specifications

This directory defines the `v2` core architecture for Skelegent — the current,
shipped architecture. The original v1 numbered specs are archived under
`../skg-archived/specs/`.

`specs/v2/20-v2-architecture-redesign.md` is the current runtime redesign and
supersedes the runtime, invocation, tool-dispatch, and environment direction of
specs `01`–`17`. Specs `00`–`17` remain authoritative only for the wire types
and semantics that survive (Content, Intent, ExecutionEvent, Outcome,
CapabilityDescriptor, DispatchContext).

## Status and Authority

- The `v2` track has been implemented as a breaking cutover; it is the shipped
  architecture. The original v1 numbered specs are archived under
  `../skg-archived/specs/`.
- `specs/v2/20` is implementation-authoritative for the runtime, invocation,
  tool-dispatch, and environment model. Where `01`–`17` disagree with `20` or
  with the code, `20` and the code win.
- Authority order (per `AGENTS.md`): `ARCHITECTURE.md` > `specs/v2` > `rules` >
  agent judgment.

## Reading Order

Read in order:

1. `specs/v2/00-overview-and-migration.md`
2. `specs/v2/01-kernel-principles-and-layers.md`
3. `specs/v2/02-invocation-outcomes-and-waits.md`
4. `specs/v2/03-intents-and-semantic-events.md`
5. `specs/v2/04-capability-sources-and-descriptors.md`
6. `specs/v2/05-streaming-runtime-and-provider-projection.md`
7. `specs/v2/06-scheduling-and-turn-execution.md`
8. `specs/v2/07-state-base-and-extension-families.md`
9. `specs/v2/08-content-environment-and-artifacts.md`
10. `specs/v2/09-durable-alignment-and-control.md`
11. `specs/v2/10-errors-versioning-and-conformance.md`
12. `specs/v2/11-session-state-memory-and-context.md`
13. `specs/v2/12-observation-intervention-and-queries.md`
14. `specs/v2/13-composition-patterns-and-control-surfaces.md`
15. `specs/v2/14-lifecycle-policies-compaction-and-governance.md`
16. `specs/v2/15-layer0-invocation-outcome-migration-annex.md`
17. `specs/v2/16-intent-event-migration-annex.md`
18. `specs/v2/17-capability-discovery-migration-annex.md`
19. `specs/v2/20-v2-architecture-redesign.md`

## Core Commitments

V2 commits Skelegent to:

- one invocation model
- one semantic event plane
- one native capability model
- executable intents separated from observations
- stream-first execution
- extension-based backend families instead of ever-growing monolithic traits

## Index

| Spec | Domain | Summary |
|------|--------|---------|
| `specs/v2/00-overview-and-migration.md` | Product/Strategy | Goals, invariants, migration map, and adoption policy |
| `specs/v2/01-kernel-principles-and-layers.md` | Architecture | Kernel purity rules and revised layer model |
| `specs/v2/02-invocation-outcomes-and-waits.md` | Invocation | Stream-first invocation, typed outcomes, shared waits |
| `specs/v2/03-intents-and-semantic-events.md` | Runtime Core | Split intents from observations and define the semantic event envelope |
| `specs/v2/04-capability-sources-and-descriptors.md` | Discovery | CapabilitySource and CapabilityDescriptor as native discovery surfaces |
| `specs/v2/05-streaming-runtime-and-provider-projection.md` | Turn Runtime | Provider stream-first contract and semantic projection rules |
| `specs/v2/06-scheduling-and-turn-execution.md` | Scheduling | Promote turn-kit planning into the runtime and remove ad hoc concurrency |
| `specs/v2/07-state-base-and-extension-families.md` | State | Base state trait plus extension families and accessors |
| `specs/v2/08-content-environment-and-artifacts.md` | Content/Environment | Binary content, artifacts, and declarative environment contract |
| `specs/v2/09-durable-alignment-and-control.md` | Durable Control | Shared immediate/durable nouns with durable-only lifecycle kept above Layer 0 |
| `specs/v2/10-errors-versioning-and-conformance.md` | Quality/Release | Structured errors, semver rules, and conformance requirements |
| `specs/v2/11-session-state-memory-and-context.md` | State/Runtime | Distinguish session state, active context, persistent memory, and structural state |
| `specs/v2/12-observation-intervention-and-queries.md` | Control/Visibility | Separate observation, intervention, guardrails, oracle consultation, and queries |
| `specs/v2/13-composition-patterns-and-control-surfaces.md` | Composition | Define multi-agent control surfaces without a kernel workflow abstraction |
| `specs/v2/14-lifecycle-policies-compaction-and-governance.md` | Lifecycle | Define compaction, persistence, crash posture, and budget governance above Layer 0 |
| `specs/v2/15-layer0-invocation-outcome-migration-annex.md` | Implementation Annex | Lock the first layer0 migration slice for outcomes, invocation handles, waits, and structured protocol errors |
| `specs/v2/16-intent-event-migration-annex.md` | Implementation Annex | Lock the intent/event cutover slice: replace Effect + DispatchEvent with Intent + ExecutionEvent and stream projection rules |
| `specs/v2/17-capability-discovery-migration-annex.md` | Implementation Annex | Lock the capability discovery cutover slice: CapabilitySource/Descriptor replace tool-metadata-centric discovery |
| `specs/v2/20-v2-architecture-redesign.md` | Architecture | Current runtime redesign — Operator/Behaviour/ContextOp model; supersedes 01–17 runtime direction |
