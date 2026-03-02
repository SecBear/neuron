# Composition Proof Readiness Design (0.4.0)

## Context

This design treats `/Users/bear/dev/neuron-explore` (`redesign/v2`) as the canonical repo for the Neuron redesign and release preparation for `0.4.0` (not `1.0`).

The primary goal is to prove real composability of the architecture before swap-over, then harden test coverage, then bring docs/DX to parity.

## Sequence

Execution order is fixed:

1. Composition proof (`2`)
2. Coverage hardening (`3`)
3. Docs/DX parity (`1`)

## Scope

Deliver three first-class example binaries under `examples/`:

1. `daily_digest`
2. `triage_escalation`
3. `provider_parity`

Examples should assume real provider/backends in architecture and wiring, while tests prove swappability by replacing components with mock implementations through shared composition factories.

## Architecture

### Orchestrator-Centric Factory Layer

Factory responsibilities live in orchestration-layer implementation (via a dedicated composition crate so layer boundaries remain clean).

Proposed structure:

- Add `neuron-orch-compose` crate
- Define composition inputs (`CompositionSpec`, backend selectors, policy selectors)
- Build prewired local orchestrator graphs from specs:
  - mock path wiring
  - real path wiring

Examples and tests must consume the same factory entry points.

### Test Model

Two-path verification:

1. Mock path (required in CI)
2. Real path (opt-in, env-gated, ignored by default)

Real-path tests use the same composition specs and only swap backend selectors.

## Coverage Criteria

Coverage target is behavioral completeness per primitive (not raw line percentage):

- Orchestration: single dispatch, fanout, partial failure, missing agents
- Operator: normal completion, controlled exits, tool failure mapping
- State: backend parity, scope isolation, persistence semantics
- Hooks/Policy: continue/deny/modify paths and ordering
- Provider integration: response mapping and guarded live tests

Each primitive must have success and failure-path assertions in at least one example-backed test.

## Docs/DX Parity (After Composition + Coverage)

Add and gate for `0.4.0`:

- Root `README.md` with quickstart and example catalog
- CI workflows (`build`, `test`, `clippy`, `doc`)
- `llms.txt`
- Expanded workspace `CLAUDE.md` guidance
- Clear crate status matrix (`production-ready` vs `stub`)

## Non-Goals

- No `1.0` migration in this phase
- No forced completion of all backend stubs before `0.4.0`
- No context split across two canonical repos

## Decision Log

- Canonical repo: `neuron-explore` on `redesign/v2`
- Delivery mode: examples-first, then coverage, then docs/DX
- Example location: `examples/` runnable binaries (with integration tests)
- Factory ownership: orchestrator composition layer
