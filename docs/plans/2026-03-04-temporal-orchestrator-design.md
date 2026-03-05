# Design: `neuron-orch-temporal` — Temporal-Backed Orchestrator

**Date:** 2026-03-04
**Status:** Draft
**Crate:** `neuron/orch/neuron-orch-temporal/`
**Depends on:** `layer0`, `temporal-sdk-core` (or `temporal-client`)

---

## 1. Decision: Why Temporal

We evaluated three options for durable orchestration:

| Criterion               | Temporal                         | Restate                          | Checkpoint-to-StateStore          |
|--------------------------|----------------------------------|----------------------------------|-----------------------------------|
| Maturity                 | 5+ years, battle-tested          | Newer, fast iteration            | Custom, must build from scratch   |
| Rust support             | Official `temporal-sdk-core`     | Community Rust SDK               | N/A (internal)                    |
| Production scale proof   | Uber, Stripe, Netflix, Datadog   | Early adopters                   | Unproven                          |
| Signal/Query primitives  | Native, first-class              | Virtual objects (similar)        | Must implement manually           |
| Activity retry/heartbeat | Built-in, configurable policies  | Built-in                         | Must implement manually           |
| Operator trait mapping   | Natural (activities + workflows) | Possible (handlers + services)   | Forced (state machine + loops)    |

**Choice: Temporal.** It is the most mature durable execution framework with an official Rust SDK.
The natural mapping between Temporal's workflow/activity/signal/query primitives and our existing
`Orchestrator` trait methods makes this the lowest-friction path to durable orchestration.

Restate was a close second — its simpler deployment model is attractive — but the smaller Rust
ecosystem and shorter production track record tipped the balance. The checkpoint-to-StateStore
approach was rejected: it reimplements what durable execution frameworks already solve, and
the maintenance burden is not justified when Temporal exists.

---

## 2. Architecture

```
neuron/orch/neuron-orch-temporal/
├── Cargo.toml
├── src/
│   ├── lib.rs              # TemporalOrch, re-exports
│   ├── config.rs           # TemporalConfig
│   ├── client.rs           # Internal TemporalClient wrapper (NOT exported)
│   ├── worker.rs           # Activity/workflow worker registration
│   ├── activities.rs       # Operator-as-activity bridge
│   ├── workflows.rs        # Workflow definitions (sweep pipeline etc.)
│   └── effect_interpreter.rs  # DurableEffectInterpreter
└── tests/
    ├── dispatch_test.rs
    ├── signal_query_test.rs
    └── effect_test.rs
```

### Dependency graph

```
layer0 (traits: Orchestrator, Operator, Effect, OperatorInput/Output)
  │
  ├── neuron-orch-local      (in-process, no durability)
  │
  └── neuron-orch-temporal    (this crate)
        ├── temporal-sdk-core  (Temporal Rust bridge)
        ├── tokio              (async runtime)
        └── serde_json         (already transitive from layer0)
```

Per the architecture, `TemporalOrch` does **not** export the Temporal client. All Temporal
internals are private. Consumers depend only on `layer0::Orchestrator`.

---

## 3. Mapping: Orchestrator Trait → Temporal Primitives

This is the critical design section. Every `Orchestrator` method maps to a Temporal concept:

| Orchestrator method                  | Temporal concept         | Semantics                                                                 |
|--------------------------------------|--------------------------|---------------------------------------------------------------------------|
| `dispatch(agent, input)`             | **Activity execution**   | Each operator invocation is a single Temporal activity within a workflow   |
| `dispatch_many(tasks)`               | **Parallel activities**  | Fan-out via `tokio::join!` over activity futures, or child workflows       |
| `signal(target, payload)`            | **Workflow signal**      | Native 1:1 mapping — `SignalPayload` delivered to workflow signal channel  |
| `query(target, payload)`             | **Workflow query**       | Native 1:1 mapping — `QueryPayload` handled by workflow query handler     |

### dispatch → Activity

```rust
// Conceptual (not final API)
async fn dispatch(&self, agent: &AgentId, input: OperatorInput) -> Result<OperatorOutput, OrchError> {
    // OperatorInput implements Serialize — sent as activity input
    // OperatorOutput implements Deserialize — received as activity result
    // Temporal handles retry, timeout, heartbeat automatically
    let result = self.workflow_handle
        .execute_activity(OperatorActivity {
            agent_id: agent.clone(),
            input,
        })
        .await?;
    Ok(result)
}
```

Key property: `OperatorInput` and `OperatorOutput` already derive `Serialize`/`Deserialize`.
They are wire-compatible with Temporal's activity input/output payloads with zero transformation.

### dispatch_many → Parallel Activities

```rust
async fn dispatch_many(&self, tasks: Vec<(AgentId, OperatorInput)>) -> Vec<Result<OperatorOutput, OrchError>> {
    // Each task becomes an independent activity
    // Temporal tracks each independently — partial failures are isolated
    let futures: Vec<_> = tasks.into_iter()
        .map(|(agent, input)| self.dispatch(&agent, input))
        .collect();
    futures::future::join_all(futures).await
}
```

Alternative: for large fan-outs (50+ tasks), use child workflows to isolate failure domains.
Decision threshold TBD during implementation.

### signal → Temporal Signal

```rust
async fn signal(&self, target: &WorkflowId, signal: SignalPayload) -> Result<(), OrchError> {
    // SignalPayload is already serde-compatible
    // WorkflowId maps directly to Temporal workflow ID
    self.client.signal_workflow(
        target.as_str(),
        "operator_signal",
        signal,
    ).await.map_err(|e| OrchError::SignalFailed { /* no secrets */ })
}
```

### query → Temporal Query

```rust
async fn query(&self, target: &WorkflowId, query: QueryPayload) -> Result<serde_json::Value, OrchError> {
    // query_type becomes the Temporal query type
    // params becomes the query payload
    self.client.query_workflow(
        target.as_str(),
        &query.query_type,
        query.params,
    ).await.map_err(|e| OrchError::QueryFailed { /* no secrets */ })
}
```

### Pipeline Functions → Workflow Definitions

Higher-level functions like `run_sweep()` become Temporal **workflow definitions**:

```rust
// A sweep is a workflow that orchestrates multiple operator dispatches
async fn sweep_workflow(ctx: WfContext, input: SweepInput) -> Result<SweepOutput, Error> {
    for entity in input.entities {
        // Each operator call is an activity — retried, heartbeated, durable
        let result = ctx.activity(OperatorActivity { ... }).await?;
        // Effects from the result are executed via DurableEffectInterpreter
        ctx.activity(ExecuteEffects { effects: result.effects }).await?;
    }
}
```

---

## 4. Effect Execution

Effects are the critical boundary between LocalOrch and TemporalOrch.

### LocalOrch (current)

Effects are executed inline, immediately after `dispatch()` returns:

```
dispatch() → OperatorOutput { effects: [...] }
   ↓
for effect in output.effects {
    effect_executor.execute(effect).await;  // inline, in-process
}
```

### TemporalOrch (proposed)

Effects are serialized as activity output, then executed as separate activities by the
workflow orchestration code:

```
dispatch() → OperatorOutput { effects: [...] }
   ↓
// Effects are part of the serialized activity result
// Workflow code then executes each effect as its own activity
for effect in output.effects {
    ctx.activity(EffectActivity { effect }).await;
}
```

### DurableEffectInterpreter

Wraps each effect execution in a Temporal activity, providing:

- **Durability**: Effect execution is recorded in workflow history. If the process crashes
  after executing effect 3 of 5, replay skips 1–3 and resumes at 4.
- **Retry**: Transient failures (network errors during `WriteMemory` to a remote store)
  are retried per the activity retry policy.
- **Visibility**: Each effect is a named activity in the Temporal UI, making debugging
  straightforward.

```rust
pub struct DurableEffectInterpreter {
    inner: Arc<dyn EffectExecutor>,  // The actual executor (same one LocalOrch uses)
}

impl DurableEffectInterpreter {
    /// Execute an effect as a Temporal activity.
    /// The inner executor does the real work; this wrapper provides durability.
    pub async fn execute(&self, ctx: &ActivityContext, effect: Effect) -> Result<(), EffectError> {
        // Temporal records the result — replay skips this on recovery
        self.inner.execute(effect).await
    }
}
```

The same `EffectExecutor` implementation is shared between LocalOrch and TemporalOrch.
The difference is purely in the execution boundary — inline vs. activity-wrapped.

---

## 5. Temporal SDK Assessment

### `temporal-sdk-core`

- **Role**: Low-level Rust bridge to Temporal. This is the official crate maintained by
  Temporalio. It implements the Core SDK spec — the same layer that backs the TypeScript,
  Python, and .NET SDKs.
- **API level**: Worker/client primitives, poll loops, activity/workflow task handling.
- **Maturity**: Production-used (it backs the official language SDKs), but the Rust-native
  API surface is less polished than Go/Java/TypeScript.
- **Async**: Fully async, tokio-compatible.
- **Repository**: `temporalio/sdk-core` on GitHub.

### `temporal-client`

- **Role**: Higher-level Rust client wrapper. Evaluate whether this provides meaningful
  abstraction over `temporal-sdk-core` or is just a thin wrapper.
- **Status**: Needs maturity assessment before committing. May be abandoned or sparsely
  maintained.

### Maturity reality check

The Temporal Rust SDK is **not** as mature as the Go, Java, or TypeScript SDKs:

- Go/Java: Feature-complete, stable API, extensive documentation, production-proven.
- TypeScript: Feature-complete, stable, well-documented.
- Rust (via `sdk-core`): Functional but lower-level. API may require more boilerplate.
  Documentation is thinner. Some higher-level patterns (interceptors, advanced codec)
  may need manual implementation.

### Fallback: gRPC Direct

If the Rust SDK has critical gaps, we can use Temporal's **gRPC API** directly:

- Temporal's server exposes a well-documented gRPC API (protobuf definitions in
  `temporalio/api`).
- Use `tonic` for gRPC client generation.
- This gives full control but requires implementing workflow replay logic ourselves
  (significant effort — avoid if possible).
- **Recommendation**: Start with `temporal-sdk-core`. Fall back to gRPC only for
  specific gaps (e.g., missing interceptor support), not wholesale replacement.

---

## 6. Configuration

```rust
/// Configuration for connecting to a Temporal server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalConfig {
    /// Temporal server URL (e.g., "http://localhost:7233").
    /// Resolved from environment at connection time — never hardcoded.
    pub server_url: String,

    /// Temporal namespace (e.g., "default", "neuron-prod").
    pub namespace: String,

    /// Task queue name. Workers poll this queue for tasks.
    pub task_queue: String,

    /// TLS configuration. None = plaintext (local dev only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,

    /// Activity retry policy defaults. Individual activities can override.
    #[serde(default)]
    pub retry_policy: RetryPolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to client certificate (PEM).
    pub client_cert_path: String,
    /// Path to client key (PEM).
    pub client_key_path: String,
    /// Optional CA certificate for server verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_cert_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicyConfig {
    /// Maximum number of retry attempts. 0 = no retry.
    pub max_attempts: u32,
    /// Initial retry interval.
    pub initial_interval_ms: u64,
    /// Maximum retry interval.
    pub max_interval_ms: u64,
    /// Backoff coefficient (multiplied each retry).
    pub backoff_coefficient: f64,
}
```

### Connection management

`TemporalClient` is an internal wrapper (not exported, per architectural rules). It handles:

- Lazy connection establishment
- Reconnection on transient failures
- Health checking (ping before dispatch)

Environment variables (e.g., `TEMPORAL_SERVER_URL`) are resolved at connection time,
never stored in config structs. Error messages must not leak env var values.

### Worker registration

Operators are registered as activity implementations:

```rust
impl TemporalOrch {
    pub fn register_agent(&mut self, id: impl Into<String>, operator: Arc<dyn Operator>) {
        // Operator is wrapped in an activity handler
        // When Temporal dispatches an activity, we look up the operator by agent ID,
        // deserialize OperatorInput, call operator.run(), serialize OperatorOutput
        self.agents.insert(id.into(), operator);
    }
}
```

---

## 7. Testing Strategy

### Unit tests (no Temporal server required)

- **Mock Temporal client**: Replace `TemporalClient` internals with a mock that records
  activity/signal/query calls.
- **Verify dispatch → activity mapping**: Assert that `dispatch(agent, input)` produces
  the correct activity invocation with serialized `OperatorInput`.
- **Verify signal/query mapping**: Assert correct workflow ID and payload routing.
- **Effect interpreter**: Test that `DurableEffectInterpreter` delegates to the inner
  executor correctly.
- **No real HTTP calls**: All Temporal communication is mocked at the client boundary.

### Integration tests (Temporal test server)

- Use **temporalite** (lightweight Temporal server) in Docker for integration tests.
- Test full round-trip: register operator → start workflow → dispatch activity → receive output.
- Test signal delivery and query handling end-to-end.
- Test crash recovery: kill worker mid-activity, restart, verify replay completes correctly.

### Operator compatibility

Existing operators must work unchanged with both `LocalOrch` and `TemporalOrch`:

```rust
#[tokio::test]
async fn operator_works_with_both_orchestrators() {
    let operator = Arc::new(EchoOperator::new());

    // Same operator, LocalOrch
    let local = LocalOrch::new();
    local.register_agent("echo", operator.clone());
    let result_local = local.dispatch(&AgentId::new("echo"), input.clone()).await;

    // Same operator, TemporalOrch (with mock client)
    let temporal = TemporalOrch::with_mock_client();
    temporal.register_agent("echo", operator.clone());
    let result_temporal = temporal.dispatch(&AgentId::new("echo"), input.clone()).await;

    // Both produce the same output
    assert_eq!(result_local.unwrap(), result_temporal.unwrap());
}
```

---

## 8. Migration Path

### Phase 1: dispatch-as-activity (minimal viable)

- Implement `TemporalOrch` with `dispatch()` and `dispatch_many()`.
- Each operator invocation is a single Temporal activity.
- No signal/query support yet — return `OrchError::Unsupported`.
- **Deliverable**: Can run a single operator through Temporal with retry and durability.

### Phase 2: Signal and query support

- Implement `signal()` → Temporal signal.
- Implement `query()` → Temporal query.
- Requires workflow definitions that handle signal/query channels.
- **Deliverable**: Full `Orchestrator` trait compliance.

### Phase 3: DurableEffectInterpreter

- Wrap effect execution in activities.
- Effect-by-effect durability — crash recovery resumes at the exact effect.
- **Deliverable**: Effects survive crashes.

### Phase 4: Workflow definitions for sweep pipeline

- `run_sweep()` as a Temporal workflow.
- Entity-level parallelism via child workflows or activity fan-out.
- Progress tracking via query handlers.
- **Deliverable**: Full sweep pipeline running durably on Temporal.

Each phase is independently deployable. Phase 1 is useful on its own for basic durability.

---

## 9. Risks

| Risk                                    | Likelihood | Impact | Mitigation                                                                 |
|-----------------------------------------|------------|--------|----------------------------------------------------------------------------|
| Temporal Rust SDK immaturity            | Medium     | High   | Use `temporal-sdk-core` (official, used by other SDKs). Fall back to gRPC for specific gaps. |
| Temporal server as dev dependency       | High       | Medium | Use temporalite for local dev (single binary, no external deps). CI uses Docker. |
| Serialization overhead                  | Low        | Low    | `OperatorInput`/`OperatorOutput` already use serde. Temporal uses protobuf, but payload conversion is O(n) with small constant. |
| Operator non-determinism breaks replay  | Medium     | High   | Document replay safety requirements. Operators that use wall-clock time or random values must use Temporal's deterministic APIs. Lint for common violations. |
| Activity timeout misconfiguration       | Medium     | Medium | Provide sensible defaults in `RetryPolicyConfig`. LLM inference calls need long timeouts (minutes, not seconds). Document this prominently. |
| Workflow versioning complexity          | Medium     | Medium | Use Temporal's workflow versioning (`get_version()`) from day one. Never deploy incompatible workflow changes without versioning gates. |

---

## 10. Architectural Decision Alignment

### Durable Execution and Crash Recovery

This crate implements the durable execution strategy. The decision to use durable
execution (framework replays workflow from event history, activities skip cached results)
is the most robust approach. `neuron-orch-temporal` implements exactly this:

- Activity results are cached in Temporal's event history.
- On crash, the workflow replays from history — completed activities return their cached
  results without re-execution.
- Zero token waste on crash recovery.

### Crash Recovery

Temporal replay **is** crash recovery. When a worker crashes:

1. Temporal detects the worker is gone (heartbeat timeout).
2. Another worker picks up the workflow task.
3. The workflow replays from event history — all completed activities skip.
4. Execution resumes from the exact point of failure.

This maps to the durable execution approach: automatic, exact state restoration.

### Heartbeat and Corrective Retry

Temporal activities natively support both:

- **Heartbeat**: Long-running activities (LLM inference) call `heartbeat()` periodically.
  If the worker crashes, Temporal detects the missing heartbeat and reassigns the activity.
  Heartbeat payloads can include progress information for UI display.
- **Retry policies**: Configurable per activity — max attempts, initial/max intervals,
  backoff coefficient, non-retryable error types. Transient failures (network, rate limit)
  retry automatically. Permanent failures (invalid input, auth failure) fail fast.



## Appendix A: Edge Cases

### Temporal Server Unreachable

When the Temporal server is unreachable:

- **`dispatch()`**: Returns `OrchError::Unavailable` with a generic message (no server URL
  in the error — per zero-secret constraint).
- **No graceful degradation to LocalOrch**: Falling back silently would violate the caller's
  durability expectations. If you asked for durable execution, you get durable execution or
  an error. The caller can implement fallback logic if desired.
- **Connection retry**: The `TemporalClient` wrapper retries connection with exponential
  backoff before surfacing the error.

### Workflow Versioning

Temporal workflows are long-lived. Code changes must be backward-compatible with running
workflows:

- **Use `get_version()` from day one**: Every workflow decision point that might change in
  the future must be wrapped in a version check.
- **Never change activity signatures**: Add new activity types instead. Old activities
  continue to replay correctly.
- **Namespace isolation**: Use separate Temporal namespaces for staging and production.
  Never replay production workflows against staging code.

### Replay Safety: Operator Determinism

Temporal replays workflows by re-executing workflow code and matching results against history.
This means workflow code must be **deterministic**:

- **Operators are safe**: Operators run as activities, not workflow code. Activities are NOT
  replayed — their results are read from history. Operators can be non-deterministic (they
  call LLMs, which are inherently non-deterministic).
- **Workflow definitions must be deterministic**: Code in `workflows.rs` (branching logic,
  loop conditions) must not use `SystemTime::now()`, `rand`, or `HashMap` iteration order.
  Use Temporal's `workflow::time()` and `workflow::random()` instead.
- **Effect execution is safe**: Effects run as activities (via `DurableEffectInterpreter`),
  so they are also not replayed.

**Summary**: Operators and effects are safe because they run as activities. Only workflow
orchestration code (the glue between operator calls) must be deterministic.

---

## Appendix B: Crate Structure Decision

The crate lives at `neuron/orch/neuron-orch-temporal/`, following the established pattern:

```
neuron/orch/
├── neuron-orch-kit/       # Shared utilities for orchestrator implementations
├── neuron-orch-local/     # In-process, no durability
├── neuron-orch-sweep/     # Sweep pipeline logic
└── neuron-orch-temporal/  # This crate — Temporal-backed durability
```

`neuron-orch-temporal` depends on `layer0` for traits and on `neuron-orch-kit` for shared
utilities. It does NOT depend on `neuron-orch-local` — they are sibling implementations
of the same trait.
