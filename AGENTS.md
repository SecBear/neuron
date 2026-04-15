# AGENTS.md

Entrypoint for any coding agent working in this repo.

`CLAUDE.md` is a symlink to this file. Both point to the same content.

## What This Project Is

Skelegent is a Rust workspace implementing a 6-layer composable agentic AI
runtime. Layer 0 defines the stability contract (protocol traits, wire types).
Layers 1–5 build implementations on top. Every concern — from provider
serialization to secret management — lives in exactly one crate.

Core values (in priority order): composability over convenience, declaration
separated from execution, slim defaults with opt-in complexity. See
`ARCHITECTURE.md` for full rationale.

## Key Abstractions

You must understand these types to work in this codebase:

| Type                             | Crate              | Role                                                                                                                                                                                    |
| -------------------------------- | ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Operator`                       | layer0             | Streaming-first universal primitive. `descriptor() -> CapabilityDescriptor` + `handle(input, ctx) -> Result<DispatchHandle, ProtocolError>`. Everything is an Operator.                |
| `DispatchContext`                | layer0             | Execution metadata threaded through every boundary: dispatch ID, trace context, auth, typed extensions. Every operator receives this.                                                   |
| `Context`                        | skg-context-engine | Mutable conversation substrate: messages, extensions, metrics, intents. Direct synchronous mutations. Intents declared via `push_intent()`, drained into `OperatorOutput::intents`.    |
| `ContextOp` / `ReactivePipeline` | skg-context-engine | Event-driven context transformations. `ContextOp::trigger()` selects which `AgentEvent`s fire it; `ReactivePipeline::emit()` evaluates matching ops at every loop boundary.            |
| `AgentLoop` / `AgentBehaviour`   | skg-context-engine | The agentic loop. `AgentBehaviour` callbacks: `init_context`, `capabilities`, `handle_response`, `handle_action_result`. Returns `LoopDecision` (Continue/Complete/Suspend/Delegate).  |
| `SyncOperator`                   | skg-context-engine | Convenience trait for simple tools/operators. `execute(input, ctx) -> Result<OperatorOutput, ProtocolError>`. Wrapped as `Operator` via `SyncOperatorAdapter`.                         |
| `Router`                         | skg-context-engine | Name-based dispatch to registered `Arc<dyn Operator>` children. Implements `Operator`.                                                                                                 |
| `Intent`                         | layer0             | Executable declarations (Delegate, Handoff, Signal, WriteMemory, etc.). Operators declare; outer layers execute.                                                                        |
| `ExecutionEvent`                 | layer0             | Semantic observation envelope: status changes, tool calls, intent declarations, artifacts, completion. Stream-first.                                                                    |
| `CapabilityDescriptor`           | layer0             | Read-only discovery. Describes what an operator accepts and produces. Returned by `Operator::descriptor()`.                                                                             |
| `Outcome`                        | layer0             | Typed invocation result: Terminal, Suspended, Transferred, Limited, Intercepted.                                                                                                        |
| `ProtocolError`                  | layer0             | Canonical serializable failure at invocation boundaries.                                                                                                                                |
| `Provider`                       | skg-turn           | NOT object-safe. Generic `<P: Provider>` in `AgentLoop`. Erased at the `Operator` boundary. Wraps LLM inference (Anthropic, OpenAI, Ollama, etc.).                                     |

### How they connect

```
User message
  → Router.handle(input, ctx)
    → looks up ctx.operator_id → delegates to child Operator
    → AgentLoop.handle(input, ctx)
      → spawns task, returns DispatchHandle
      → behaviour.init_context()
      → pipeline.emit(LoopStarted)
      → loop:
          pipeline.emit(BeforeInference)
          behaviour.capabilities() → compile context
          provider.infer(request) → response
          pipeline.emit(AfterInference)
          behaviour.handle_response() → LoopDecision
          if actions: dispatch via router
            → pipeline.emit(ActionRequested)
            → child Operator.handle()
            → pipeline.emit(ActionCompleted)
      → pipeline.emit(LoopEnding)
    → OperatorOutput { content, outcome, intents, metadata }
  → outer layer executes declared intents
```

## Where to Make Changes

| Task                                         | Where                                                              |
| -------------------------------------------- | ------------------------------------------------------------------ |
| New protocol trait or wire type              | `layer0/`                                                          |
| New context operation                        | `op/skg-context-engine/` implementing `ContextOp`                  |
| New operator behaviour                       | `op/skg-context-engine/` (like `AgentLoop`, `Router`)              |
| New simple operator/tool                     | implement `SyncOperator`, use `#[skg_tool]` macro                  |
| New LLM provider                             | new `provider/skg-provider-*` crate implementing `Provider`        |
| New intent variant                           | `layer0/src/intent.rs`                                             |
| New state backend                            | new `state/` crate implementing `StateStore`                       |
| Compute runtime                              | `op/skg-op-compute-runtime/`                                       |
| Auth/secrets                                 | `auth/`, `secret/`                                                 |

## Where Truth Lives

| What                       | Where                                                  |
| -------------------------- | ------------------------------------------------------ |
| Architectural positions    | `ARCHITECTURE.md`                                      |
| Behavioral requirements    | `specs/v2/` (current)                                  |
| Operational constraints    | `rules/`                                               |
| Deep rationale             | `specs/v2/` (see spec docs for rationale)              |

Authority: ARCHITECTURE.md > specs/v2 > rules > agent judgment. If specs are
ambiguous, update the specs (do not invent behavior).

## Load Order

Before implementation work, load in order:

1. This file
2. `ARCHITECTURE.md`
3. `SPECS.md` then the specific spec(s) for your task under `specs/v2/`
4. The relevant `rules/`

## Verification

This repo uses Nix-provided Rust tooling. All must pass before any commit:

```bash
nix develop -c nix fmt
nix develop -c cargo test --workspace --all-targets
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
```

Use the Nix commands directly; there is no wrapper verification script.

For layer0 test-utils:
`nix develop -c cargo test --features test-utils -p layer0`

Do not claim "done" without fresh evidence from the relevant commands.

## Communication Hygiene

- Optimize outputs for terminal readability. Avoid excess vertical sections and
  vertical whitespace where unnecessary.

## Patterns to Know

**Everything is an Operator.** Tools, agents, routers — all implement `Operator`. `SyncOperator` is the
convenience path for simple tools; `SyncOperatorAdapter` wraps it automatically.

**Context operations are first-class.** `ContextOp` with the `Trigger` system fires on `AgentEvent`s.
`ReactivePipeline::emit()` evaluates matching ops at every loop boundary. Budget guards, compaction,
and telemetry are all `ContextOp` implementations.

**AgentLoop is the agentic behaviour.** `AgentBehaviour` trait provides callbacks (`init_context`,
`capabilities`, `handle_response`, `handle_action_result`). The loop emits 14 `AgentEvent` variants
through the `ReactivePipeline` at every boundary.

**Router replaces Dispatcher.** Name-based dispatch to registered `Arc<dyn Operator>` children.
There is no `Dispatcher` trait; routing is done via `Router`, which itself implements `Operator`.

**Provider is generic, Operator is object-safe.** `AgentLoop<P: Provider, B: AgentBehaviour>` is
generic over the provider. The object-safe boundary is `Operator::handle()`, which erases the
provider type. This is by design — see ARCHITECTURE.md §"The Object-Safety Decision."

**Intents are on Context.** Operators declare via `ctx.push_intent()` during execution. These are
drained into `OperatorOutput::intents` by the runtime. There is no separate intent parameter.

## Codifying Learnings

When a failure mode repeats:

1. Fix the immediate issue.
2. Encode: behavior requirement → spec in `specs/v2/`. Process constraint → rule in
   `rules/`.

## Rules Index

Rules in `rules/` are numbered by concern area. Gaps in numbering are
intentional — numbers reserve space for future rules in their domain. Currently
defined: `01` (scope), `02` (verification), `04` (TDD), `06` (worktrees), `07`
(commits), `08` (review), `11` (protocol philosophy).
