# Composition Proof Readiness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship three runnable composition-proof examples (`daily_digest`, `triage_escalation`, `provider_parity`) with mock/real swappability, then harden coverage, then close docs/DX gaps for a `0.4.0`-ready swap decision.

**Architecture:** Introduce an orchestrator-centric composition crate (`neuron-orch-compose`) that builds LocalOrch graphs from a `CompositionSpec`. Examples and tests use the same factory entry points. Mock-path tests are required in CI; real-path tests are opt-in and env-gated.

**Tech Stack:** Rust workspace (`cargo`), `layer0` traits, `neuron-orch-local`, `neuron-op-react`, `neuron-op-single-shot`, provider crates (Anthropic/OpenAI/Ollama), `neuron-state-memory`, `neuron-state-fs`, `neuron-hooks`, integration tests.

---

### Task 1: Set Up Isolated Worktree Baseline

**Files:**
- Modify: `.gitignore`
- Create: `.worktrees/` (directory, local only)
- Test: N/A

**Step 1: Ensure worktree directory is ignored**

Add:

```gitignore
/target
/.worktrees/
```

**Step 2: Commit ignore update**

Run: `git add .gitignore && git commit -m "chore: ignore local worktree directory"`
Expected: commit created on `redesign/v2`

**Step 3: Create isolated branch worktree**

Run: `git worktree add .worktrees/composition-proof -b feat/composition-proof-readiness`
Expected: new worktree path created

**Step 4: Verify clean baseline**

Run: `cargo test`
Expected: baseline passes or known failures documented before proceeding

**Step 5: Commit baseline note if needed**

Run: `git add docs/plans/...` (if documenting baseline failures) and commit.

### Task 2: Add Orchestrator Composition Crate Skeleton

**Files:**
- Modify: `Cargo.toml`
- Create: `neuron-orch-compose/Cargo.toml`
- Create: `neuron-orch-compose/src/lib.rs`
- Test: `neuron-orch-compose/src/lib.rs`

**Step 1: Write failing tests for spec and builder API**

Add tests expecting:

```rust
#[test]
fn composition_spec_defaults_to_mock_profile() {
    let spec = CompositionSpec::default();
    assert!(matches!(spec.runtime, RuntimeProfile::Mock));
}

#[test]
fn unknown_example_profile_is_rejected() {
    let err = CompositionSpec::new("unknown").validate().unwrap_err();
    assert!(err.to_string().contains("unknown example"));
}
```

**Step 2: Run test to verify fail**

Run: `cargo test -p neuron-orch-compose`
Expected: FAIL due to missing types/impl

**Step 3: Implement minimal crate and workspace wiring**

Implement:

```rust
pub enum RuntimeProfile { Mock, Real }
pub enum ExampleFlow { DailyDigest, TriageEscalation, ProviderParity }
pub struct CompositionSpec { pub flow: ExampleFlow, pub runtime: RuntimeProfile }
pub fn build_local_orchestrator(spec: &CompositionSpec) -> Result<LocalOrch, ComposeError> { ... }
```

Add crate to workspace members in root `Cargo.toml`.

**Step 4: Run tests to verify pass**

Run: `cargo test -p neuron-orch-compose`
Expected: PASS

**Step 5: Commit**

Run:

```bash
git add Cargo.toml neuron-orch-compose
git commit -m "feat: add orchestrator composition crate skeleton"
```

### Task 3: Implement Shared Factory Wiring for Mock and Real Paths

**Files:**
- Modify: `neuron-orch-compose/src/lib.rs`
- Create: `neuron-orch-compose/tests/factory.rs`
- Test: `neuron-orch-compose/tests/factory.rs`

**Step 1: Write failing integration tests for swappability**

Create tests asserting same flow/spec can build both runtimes:

```rust
#[tokio::test]
async fn triage_flow_builds_with_mock_runtime() { ... }

#[tokio::test]
async fn triage_flow_builds_with_real_runtime_selector() { ... }
```

**Step 2: Run tests to verify fail**

Run: `cargo test -p neuron-orch-compose --test factory`
Expected: FAIL due to unimplemented builders/selectors

**Step 3: Implement flow-specific builder functions**

Add:

```rust
pub fn build_daily_digest(spec: &CompositionSpec) -> Result<ComposedSystem, ComposeError> { ... }
pub fn build_triage_escalation(spec: &CompositionSpec) -> Result<ComposedSystem, ComposeError> { ... }
pub fn build_provider_parity(spec: &CompositionSpec) -> Result<ComposedSystem, ComposeError> { ... }
```

Where `ComposedSystem` contains orchestrator + metadata needed by examples/tests.

**Step 4: Run tests to verify pass**

Run: `cargo test -p neuron-orch-compose`
Expected: PASS

**Step 5: Commit**

```bash
git add neuron-orch-compose
git commit -m "feat: add mock and real composition factories"
```

### Task 4: Add `daily_digest` Example and Mock-Path Test

**Files:**
- Create: `examples/daily_digest.rs`
- Create: `tests/example_daily_digest.rs`
- Modify: `Cargo.toml` (if dependency exposure needed)
- Test: `tests/example_daily_digest.rs`

**Step 1: Write failing integration test for digest flow**

Test should assert:
- digest job dispatch succeeds
- state write effect present or persisted
- delivery signal effect present

**Step 2: Run test to verify fail**

Run: `cargo test --test example_daily_digest`
Expected: FAIL due to missing example/factory hook-up

**Step 3: Implement example binary using `neuron-orch-compose`**

Minimal shape:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let spec = CompositionSpec::daily_digest(RuntimeProfile::Real);
    let system = build_daily_digest(&spec)?;
    system.run_once().await?;
    Ok(())
}
```

**Step 4: Run tests to verify pass (mock path)**

Run: `cargo test --test example_daily_digest`
Expected: PASS

**Step 5: Commit**

```bash
git add examples/daily_digest.rs tests/example_daily_digest.rs Cargo.toml
git commit -m "feat: add daily digest composition example"
```

### Task 5: Add `triage_escalation` Example and Mock-Path Test

**Files:**
- Create: `examples/triage_escalation.rs`
- Create: `tests/example_triage_escalation.rs`
- Modify: `neuron-orch-compose/src/lib.rs`
- Test: `tests/example_triage_escalation.rs`

**Step 1: Write failing test for delegate/handoff and policy behavior**

Assertions:
- initial agent delegates/escalates
- hook policy can deny/modify/continue as configured
- final output and state transitions are deterministic in mock mode

**Step 2: Run test to verify fail**

Run: `cargo test --test example_triage_escalation`
Expected: FAIL

**Step 3: Implement example with composed orchestration flow**

Use composition spec + factory; avoid direct hand-wiring in example.

**Step 4: Run tests to verify pass**

Run: `cargo test --test example_triage_escalation`
Expected: PASS

**Step 5: Commit**

```bash
git add examples/triage_escalation.rs tests/example_triage_escalation.rs neuron-orch-compose/src/lib.rs
git commit -m "feat: add triage escalation composition example"
```

### Task 6: Add `provider_parity` Example and Real-Path Opt-In Tests

**Files:**
- Create: `examples/provider_parity.rs`
- Create: `tests/example_provider_parity_mock.rs`
- Create: `tests/example_provider_parity_real.rs`
- Test: `tests/example_provider_parity_mock.rs`, `tests/example_provider_parity_real.rs`

**Step 1: Write failing mock-path parity test**

Assertions:
- same prompt/spec run through selector A and selector B
- output envelope invariants hold (exit reason, metadata shape, non-empty content)

**Step 2: Run test to verify fail**

Run: `cargo test --test example_provider_parity_mock`
Expected: FAIL

**Step 3: Implement example and mock parity test support**

Example should support runtime profile arg/env:
- `RuntimeProfile::Mock` for default local run
- `RuntimeProfile::Real` for live providers

**Step 4: Add real-path ignored test**

Add `#[ignore]` tests requiring env vars:
- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- optional Ollama service

Run:

```bash
cargo test --test example_provider_parity_mock
cargo test --test example_provider_parity_real -- --ignored
```

Expected: mock PASS; real runs only when env is configured

**Step 5: Commit**

```bash
git add examples/provider_parity.rs tests/example_provider_parity_mock.rs tests/example_provider_parity_real.rs
git commit -m "feat: add provider parity example with mock and real test paths"
```

### Task 7: Coverage Hardening Matrix (Phase 3)

**Files:**
- Create: `tests/composition_failure_matrix.rs`
- Modify: flow tests under `tests/example_*.rs`
- Test: `tests/composition_failure_matrix.rs`

**Step 1: Write failing failure-path tests by primitive**

Add targeted tests:
- missing agent / partial fanout failure
- tool failure mapping
- scope isolation across state backends
- hook deny/modify ordering

**Step 2: Run targeted tests to verify fail**

Run: `cargo test --test composition_failure_matrix`
Expected: FAIL on unimplemented assertions

**Step 3: Implement minimal behavior/factory adjustments**

Only change production code where tests prove missing behavior.

**Step 4: Run full suite for changed areas**

Run:

```bash
cargo test --test composition_failure_matrix
cargo test --test example_daily_digest
cargo test --test example_triage_escalation
cargo test --test example_provider_parity_mock
```

Expected: PASS

**Step 5: Commit**

```bash
git add tests/composition_failure_matrix.rs tests/example_*.rs neuron-orch-compose/src/lib.rs
git commit -m "test: harden composition coverage for failure and edge paths"
```

### Task 8: Docs and DX Parity (Phase 1)

**Files:**
- Create: `README.md`
- Create: `llms.txt`
- Create: `.github/workflows/ci.yml`
- Modify: `CLAUDE.md`
- Create: `docs/examples/README.md` (or `docs/examples.md`)

**Step 1: Write failing docs/command checks (manual checklist)**

Checklist must include:
- mock quickstart command works
- example commands documented
- real test invocation documented with `--ignored`

**Step 2: Add README and examples catalog**

Document:
- architecture snapshot
- 3 examples and intent
- mock vs real execution instructions

**Step 3: Add CI workflow**

Run in CI:

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo doc --no-deps
```

Keep real-provider tests opt-in/manual.

**Step 4: Verify locally**

Run the same commands locally and confirm pass/fail status explicitly.

**Step 5: Commit**

```bash
git add README.md llms.txt .github/workflows/ci.yml CLAUDE.md docs/examples*
git commit -m "docs: add 0.4.0 readiness docs and ci baseline"
```

### Task 9: Release Readiness Validation for 0.4.0

**Files:**
- Create: `docs/plans/2026-02-26-composition-proof-readiness-validation.md`
- Modify: `DEVELOPMENT-LOG.md`
- Test: full workspace verification

**Step 1: Capture release checklist**

Checklist fields:
- examples runnable
- mock coverage passing
- real-path test commands documented
- stub status matrix present

**Step 2: Run final verification**

Run:

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo doc --no-deps
```

**Step 3: Record outputs in validation doc**

Include pass/fail and blocker notes.

**Step 4: Update development log with completion state**

Add summary entry for phases `2 -> 3 -> 1`.

**Step 5: Commit**

```bash
git add docs/plans/2026-02-26-composition-proof-readiness-validation.md DEVELOPMENT-LOG.md
git commit -m "chore: record 0.4.0 composition readiness validation"
```
