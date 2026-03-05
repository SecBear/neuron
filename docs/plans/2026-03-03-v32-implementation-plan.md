# V3.2 Implementation Plan

> Orchestration prompt for implementing all v3.2 audit findings in Neuron.
> Load this after AGENTS.md. This is the session's mission.

## Mission

Implement all V32 items from `ralph_queue.md` in two batches, then verify
Neuron's implementation matches the v3.2 architectural decisions.

## Required Load Order

1. `AGENTS.md` (this repo's operating manual)
2. This file (`docs/plans/2026-03-03-v32-implementation-plan.md`)
3. `ralph_queue.md` (full item specs with files, changes, tests, verify commands)
4. `specs/09-hooks-lifecycle-and-governance.md` (hook architecture spec)
5. `specs/04-operator-turn-runtime.md` (operator spec with exit priority + three-primitive)
6. `ARCHITECTURE.md` (architectural positions — three-primitive, HookKind, exit priority)

## Current State

- Branch: merged into `main`
- Last commit: `e70bf55` — governing docs updated with v3.2 decisions
- V3.2 architectural decisions finalized
- All V32 items are in `ralph_queue.md` with full context
- Governing docs (ARCHITECTURE, AGENTS, specs 04/09) already reflect the target architecture
- RFC-ExecPrimitives: COMPLETE (verified, in Completed section of queue)
- DX-04 through DX-07: NOT started, in queue after V32 items

## What to Implement

10 items across 6 ralph agents in 2 batches.

### Batch 1 — 4 parallel isolated ralphs (no file overlaps)

| Ralph | Queue Items | Crates/Files | What |
|---|---|---|---|
| R1-Layer0 | V32-04 layer0 part + V32-08 layer0 part | `layer0/src/hook.rs` | Add 3 HookPoint variants (PreSteeringInject, PostSteeringSkip, PreMemoryWrite) + 4 HookContext fields (steering_messages, skipped_tools, memory_key, memory_value) + update HookContext::new() |
| R2-Hooks | V32-02 + V32-06 | `hooks/neuron-hooks/src/lib.rs`, `hooks/neuron-hooks/Cargo.toml` | Add HookKind enum, change dispatch to type-based composition, add tracing for hook errors |
| R3-Retry | V32-03 | `turn/neuron-turn/src/provider.rs`, `provider/neuron-provider-anthropic/src/lib.rs`, `provider/neuron-provider-openai/src/lib.rs`, `provider/neuron-provider-ollama/src/lib.rs` | Split RequestFailed into TransientError/ContentBlocked/AuthFailed, fix is_retryable(), update all providers |
| R4-ExfilGuard | V32-07 | `hooks/neuron-hook-security/src/lib.rs` | Generalize ExfilGuardHook beyond curl/wget to detect URLs + sensitive data in any tool input |

**Verify batch 1:** `nix develop -c cargo build --workspace && nix develop -c cargo test --workspace --all-targets`

### Batch 2 — 2 parallel isolated ralphs (after batch 1 merges)

| Ralph | Queue Items | Crates/Files | What |
|---|---|---|---|
| R5-OpReact | V32-01 → V32-04 op-react → V32-05 → V32-09 → V32-10 | `op/neuron-op-react/src/lib.rs` | Fix exit priority, add steering hook dispatch, add compaction reserve, add step/loop limits, add model selector. MUST be sequential (same file). |
| R6-Effects | V32-08 effects part | `effects/neuron-effects-local/src/lib.rs`, `orch/neuron-orch-kit/src/runner.rs` | Wire PreMemoryWrite hook into both effect executors before state.write() calls |

**Verify batch 2:** `nix develop -c cargo build --workspace && nix develop -c cargo test --workspace --all-targets`

### Dependencies

```
R1-Layer0 ──┬──→ R5-OpReact (needs new HookPoint variants)
             └──→ R6-Effects (needs PreMemoryWrite variant)
R2-Hooks ────→ R5-OpReact (needs HookKind-aware registry for dispatch)
R3-Retry ────→ standalone
R4-ExfilGuard → standalone (but R2 changes add() signature; update registration)
```

R4-ExfilGuard should register with `HookKind::Guardrail` after R2 merges.
If R4 runs before R2, use current `add()` API — R2 migration will update it.

## Post-Implementation Verification

After both batches merge and all tests pass:

### 1. Architectural decision comparison

Compare Neuron's implementation against v3.2 decision positions as documented
in the project's governing specs (ARCHITECTURE.md, specs/04, specs/09).

Check that Neuron implements:
- [ ] Hook viability: 9 hookable concerns have hook points (tools, retry, exit, child context, result routing, lifecycle, observation, memory writes, observability)
- [ ] Hook composition: guardrail=short-circuit, transformer=chain, observer=parallel
- [ ] Hook scope principle: pre-hooks see more and can change more; post-hooks observe
- [ ] Exit priority: safety halt > budget > max turns > model done
- [ ] Steering observable via hooks without being a hook

### 2. Source-to-spec diff

For each governing doc, verify the code matches the spec:

| Spec | Code | Check |
|---|---|---|
| `ARCHITECTURE.md` §Three-Primitive | `op/neuron-op-react` builder API | with_hooks/with_steering/with_planner all work |
| `specs/09` HookPoint table | `layer0/src/hook.rs` HookPoint enum | All 9 points exist |
| `specs/09` HookKind table | `hooks/neuron-hooks/src/lib.rs` | Guardrail/Transformer/Observer dispatch correct |
| `specs/09` Hooks vs Steering table | `op/neuron-op-react` steering sections | PreSteeringInject/PostSteeringSkip fire |
| `specs/04` Exit priority | `op/neuron-op-react` exit section | Hook before limits |
| `specs/04` Compaction reserve | `op/neuron-op-react` compaction section | Reserve enforced |
| `specs/04` Model selection | `op/neuron-op-react` inference section | Selector callback invoked |

### 3. Full test suite

```bash
nix develop -c cargo fmt --check
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo test --workspace --all-targets
nix develop -c cargo doc --no-deps
```

### 4. Update queue

Move all V32 items to Completed section of `ralph_queue.md` with dates and verify commands.

## After V32 — Next Steps

1. DX-04 through DX-07 (already in queue, coordinates with V32-08)
2. ReleasePrep (RELEASE_NOTES.md, MIGRATION.md, version bump, publish dry-run)
3. LOW items (V32-11 through V32-15) — track, don't implement yet

## File Index (all files that will be modified)

```
layer0/src/hook.rs                              ← R1 (batch 1)
hooks/neuron-hooks/src/lib.rs                   ← R2 (batch 1)
hooks/neuron-hooks/Cargo.toml                   ← R2 (batch 1)
turn/neuron-turn/src/provider.rs                ← R3 (batch 1)
provider/neuron-provider-anthropic/src/lib.rs   ← R3 (batch 1)
provider/neuron-provider-openai/src/lib.rs      ← R3 (batch 1)
provider/neuron-provider-ollama/src/lib.rs      ← R3 (batch 1)
hooks/neuron-hook-security/src/lib.rs           ← R4 (batch 1)
op/neuron-op-react/src/lib.rs                   ← R5 (batch 2)
effects/neuron-effects-local/src/lib.rs         ← R6 (batch 2)
orch/neuron-orch-kit/src/runner.rs              ← R6 (batch 2)
```
