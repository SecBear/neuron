# CLAUDE.md

## What This Project Is

Neuron is a Rust workspace implementing a 6-layer composable agentic AI architecture. Layer 0 (`layer0` crate) defines the stability contract — four protocol traits (Turn, Orchestrator, StateStore, Environment), two cross-cutting interfaces (Hook, Lifecycle events), and the message types that cross every boundary. Layers 1-5 build implementations on top.

## Required Reading

Before doing ANY work, read these documents IN ORDER:

1. **NEURON-REDESIGN-PLAN.md** — The authoritative plan for the workspace redesign. 6-layer architecture, workspace structure, design decisions, phased implementation.
2. **docs/architecture/HANDOFF.md** — Layer 0 implementation spec. Trait signatures, type definitions, module structure, layer definitions.
3. **docs/architecture/composable-agentic-architecture.md** — Design rationale. 4 protocols + 2 interfaces, gap analysis, coverage map.
4. **docs/architecture/platform-scope-mapping.md** — Where features live (Neuron vs platform vs external infra).
5. **docs/architecture/agentic-decision-map-v3.md** — Full design space. All 23 architectural decisions.
6. **DEVELOPMENT-LOG.md** — Complete history of all decisions, research, and rationale across all sessions.

## Build & Test

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo doc --no-deps
```

All four must pass before any commit. For layer0 test-utils features:

```bash
cargo test --features test-utils -p layer0
```

## Rules

### Do
- Follow NEURON-REDESIGN-PLAN.md for all structural decisions
- Match layer0 trait signatures exactly — they are the stability contract
- Use `#[deny(missing_docs)]` on every public item
- Test that every message type round-trips through serde_json
- Test that every trait is object-safe (Box<dyn Trait> compiles and is Send + Sync)
- Keep layer0 dependencies minimal (serde, async-trait, thiserror, rust_decimal — that's it)
- Update DEVELOPMENT-LOG.md after each phase

### Do Not
- Add dependencies to layer0 beyond what's already there
- Add methods to layer0 protocol traits beyond what HANDOFF.md defines
- Change layer0's trait signatures — they are the stability contract
- Make layer0 traits non-object-safe
- Skip phases — the phased approach is sequential
- Make undocumented decisions — update the plan first if deviating
