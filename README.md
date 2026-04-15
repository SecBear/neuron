# Skelegent — composable agentic runtime

Skelegent is an experiment in building an agentic system that is **composable by construction**:
layered protocol contracts, swappable providers/tools/state, and deterministic backpressure via
tests and specs.

Specs are the source of truth: `SPECS.md` and `specs/`.

## Quickstart (Nix)

This repo assumes Rust tooling is provided by the Nix flake.

- Full verification: `./scripts/verify.sh`
- Canonical commands: see `AGENTS.md §Verification`

## Crate map (workspace members)

Core:

- `layer0/` — protocol kernel

Turn (`turn/`):

- `skg-turn` — `Provider` trait, inference types, streaming
- `skg-tool-macro` — `#[skg_tool]` proc-macro

Operators (`op/`):

- `skg-context-engine` — `Context`, `AgentLoop`, `Router`, `ContextOp`, `ReactivePipeline`, `SyncOperator`
- `skg-op-compute-runtime` — `ComputeRuntime`, `PythonExecTool`

State (`state/`):

- `skg-state-memory` — in-memory `StateStore`
- `skg-state-fs` — filesystem `StateStore`

Providers (`provider/`):

- `skg-provider-anthropic`
- `skg-provider-openai`
- `skg-provider-ollama`

Security (`secret/`, `auth/`):

- `skg-secret` — `SecretResolver`
- `skg-auth` — auth middleware

## Implementations

Heavy-dependency implementations — SQLite, CozoDB, Temporal, Git effects, sweep
operators, and auth providers — live in a separate repository to keep this core
dependency-free:

[**skelegent-extras**](https://github.com/SecBear/skelegent-extras) — provider ecosystem