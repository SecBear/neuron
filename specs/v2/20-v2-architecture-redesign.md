# v2 Architecture Redesign

## Purpose

Replace the current trait hierarchy (Operator + ToolDyn + Dispatcher + Environment)
with a unified, streaming-first, Erlang/OTP-inspired architecture where everything
is an Operator and context operations are first-class reactive primitives.

This spec supersedes the architectural direction of specs 01-17 for the runtime,
invocation, tool dispatch, and environment models. Wire types (Content, Intent,
ExecutionEvent, Outcome, OperatorInput, OperatorOutput, CapabilityDescriptor,
DispatchContext) survive with minimal changes.

## Design Principles

### Everything is an Operator

One universal primitive. Tools, agents, sub-agents, routers, supervisors — all
implement the same trait. The caller never knows whether the thing behind the
interface is a calculator function or a multi-turn agent with its own tool set.
This is the actor model applied to agentic AI: Erlang's "everything is a process."

### Behaviours over raw primitives

Nobody writes raw `spawn/receive` in production Erlang. Nobody should write raw
`Operator::handle()` for common patterns. OTP behaviours (gen_server, supervisor,
gen_statem) extract generic machinery into reusable modules. Skelegent provides
behaviours: `AgentLoop`, `SyncOperator`, `Supervisor`, `Router`, `StateMachine`.
Developers fill in callbacks. The framework handles streaming, error recovery,
state threading, shutdown.

### Context operations are the instruction set

The context is the agent's entire reality. Whoever controls context controls the
agent. Context operations are first-class, composable, reactive primitives that
trigger on any system event — not just inference boundaries. They compose like a
programming language: chain, branch, filter, gate. The Pipeline is a reactive
event engine, not a before/after hook list.

### Streaming-first

The base Operator trait returns a streaming handle. Simple operators emit one
event. Complex operators emit many. Collected (non-streaming) results are a
convenience wrapper, not the primary interface.

### Security by architecture

Three tiers of secret protection:
- Tier 1 (solvable): Secrets the LLM never touches (provider keys, service tokens).
  Sidecar proxy / on-the-wire injection. The LLM context never contains these.
- Tier 2 (solvable): Secrets tools need but the LLM doesn't (DB credentials).
  Constructor injection into operators, never in input schemas or error messages.
- Tier 3 (unsolvable in general): Data the LLM must reason about. Mitigated by
  output sanitization middleware, monitoring, sandboxing, least-privilege tools.

### Environment as provisioning, not execution

Environments provision isolation and inject credentials. They don't execute
operators — they wrap them. A Docker environment wraps any operator with container
isolation. The operator inside doesn't know where it's running. Environment
backends are separate crates, not core framework code.

---

## What Gets Deleted

| Component | Location | Reason |
|---|---|---|
| `ToolDyn` trait | `turn/skg-tool/src/lib.rs` | Replaced by `SyncOperator` |
| `ToolDynStreaming` trait | `turn/skg-tool/src/lib.rs` | Operator is streaming-first |
| `ToolRegistry` struct | `turn/skg-tool/src/lib.rs` | Replaced by `Router` |
| `ToolOperator` adapter | `turn/skg-tool/src/adapter.rs` | No adapter needed — tools are Operators |
| `ToolRegistryOrchestrator` | `turn/skg-tool/src/adapter.rs` | Replaced by `Router` |
| `ApprovalPolicy` on tools | `turn/skg-tool/src/lib.rs` | Moves to ContextOp (approval is a context operation) |
| `ToolConcurrencyHint` | `turn/skg-tool/src/lib.rs` | Moves to CapabilityDescriptor scheduling facts |
| `react_loop()` | `op/skg-context-engine/src/runtime.rs` | Replaced by `AgentLoop` behaviour |
| `stream_react_loop()` | `op/skg-context-engine/src/stream_runtime.rs` | Operator is streaming-first, no separate path |
| `react_loop_structured()` | `op/skg-context-engine/src/runtime.rs` | Structured output is a ContextOp |
| `AgentOperator` | `op/skg-context-engine/src/agent_operator.rs` | Replaced by `AgentLoop` |
| `AgentBuilder` | `op/skg-context-engine/src/builder.rs` | Rebuilt for AgentLoop |
| `Operator` trait (current) | `layer0/src/operator.rs` | Replaced by streaming-first Operator |
| `Dispatcher` trait | `layer0/src/dispatch.rs` | Merged into Operator |
| `Environment` trait | `layer0/src/environment.rs` | Replaced by `EnvironmentProvider` |
| `ComputeOperator` | `op/skg-op-compute-runtime/src/operator.rs` | Rebuilt as SyncOperator tool |
| `ReactLoopConfig` | `op/skg-context-engine/src/runtime.rs` | Replaced by AgentLoop config + Pipeline |
| `ToolFilter` | `op/skg-context-engine/src/runtime.rs` | Replaced by ContextOp on ActionRequested |
| Compile module's tool filtering | `op/skg-context-engine/src/compile.rs` | Pipeline handles capability filtering |

## What Survives

| Component | Location | Changes |
|---|---|---|
| `OperatorInput` | `layer0/src/operator.rs` | Keep as-is |
| `OperatorOutput` | `layer0/src/operator.rs` | Keep as-is |
| `Outcome` / `TerminalOutcome` | `layer0/src/operator.rs` | Keep as-is |
| `TriggerType` | `layer0/src/operator.rs` | Keep as-is |
| `OperatorConfig` | `layer0/src/operator.rs` | Keep as-is |
| `DispatchHandle` | `layer0/src/dispatch.rs` | Rename to `OperatorHandle` |
| `DispatchEvent` | `layer0/src/dispatch.rs` | Rename to `OperatorEvent` |
| `Artifact` | `layer0/src/dispatch.rs` | Keep as-is |
| `CollectedDispatch` | `layer0/src/dispatch.rs` | Rename to `CollectedOutput` |
| `Content` / `ContentBlock` | `layer0/src/content.rs` | Keep as-is |
| `Intent` / `IntentKind` | `layer0/src/intent.rs` | Keep as-is |
| `ExecutionEvent` | `layer0/src/event.rs` | Keep as-is |
| `ProtocolError` / `ErrorCode` | `layer0/src/error.rs` | Keep as-is |
| `DispatchContext` | `layer0/src/dispatch_context.rs` | Keep as-is |
| `CapabilityDescriptor` | `layer0/src/capability.rs` | Keep as-is |
| `CapabilitySource` | `layer0/src/capability.rs` | Keep as-is |
| `EnvironmentSpec` | `layer0/src/environment.rs` | Keep as-is |
| All ID types | `layer0/src/id.rs` | Keep as-is |
| `Provider` trait | `turn/skg-turn` | Keep as-is |
| `Context` | `op/skg-context-engine/src/context.rs` | Keep, extend with event emission |
| `Middleware` trait | `op/skg-context-engine/src/middleware.rs` | Evolve into ContextOp |
| `ComputeRuntime` | `op/skg-op-compute-runtime` | Keep as infrastructure |
| `ComputeBackend` | `op/skg-op-compute-runtime` | Keep as infrastructure |
| `CompiledContext` | `op/skg-context-engine/src/compile.rs` | Keep, simplify |
| `#[skg_tool]` macro | `turn/skg-tool-macro` | Retarget to generate SyncOperator |
| All provider crates | `provider/` | Keep as-is |
| State crates | `state/` | Keep as-is |
| Secret/auth crates | `secret/`, `auth/` | Keep as-is |

---

## Layer 0: The Operator Primitive

### Operator trait

```rust
/// The universal primitive. Everything is an Operator.
///
/// Tools, agents, supervisors, routers — all implement this trait.
/// Streaming-first: returns an OperatorHandle that emits events.
/// Simple implementations emit one Completed event.
/// Complex implementations emit progress, artifacts, and completion.
#[async_trait]
pub trait Operator: Send + Sync {
    /// Discovery metadata. What this operator can do.
    fn descriptor(&self) -> CapabilityDescriptor;

    /// Handle an invocation. Returns a streaming event handle.
    async fn handle(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<OperatorHandle, ProtocolError>;
}
```

### OperatorHandle (renamed from DispatchHandle)

No structural change. `DispatchHandle` is renamed to `OperatorHandle`.
`DispatchEvent` is renamed to `OperatorEvent`. Internal channel mechanics
(tokio mpsc, watch for cancellation) remain identical.

```rust
pub enum OperatorEvent {
    Progress { content: Content },
    ArtifactProduced { artifact: Artifact },
    Completed { output: OperatorOutput },
    Failed { error: ProtocolError },
    AwaitingApproval(ApprovalRequest),
}
```

### OperatorHandle convenience methods

```rust
impl OperatorHandle {
    /// Consume all events and return the terminal OperatorOutput.
    pub async fn collect(self) -> Result<OperatorOutput, ProtocolError>;

    /// Consume all events, preserving intermediate events.
    pub async fn collect_all(self) -> Result<CollectedOutput, ProtocolError>;

    /// Receive the next event.
    pub async fn recv(&mut self) -> Option<OperatorEvent>;

    /// Cancel the operation cooperatively.
    pub fn cancel(&self);
}
```

---

## Layer 1: Context Operations (Reactive Event Engine)

### AgentEvent

Every meaningful thing that happens in the system. Emitted by the AgentLoop
at every boundary. Pipeline evaluates matching ContextOps for each event.

```rust
#[non_exhaustive]
pub enum AgentEvent {
    // Inference lifecycle
    LoopStarted,
    BeforeInference,
    AfterInference { response: InferResponse },

    // Action lifecycle
    ActionRequested { id: OperatorId, input: OperatorInput },
    ActionCompleted { id: OperatorId, output: OperatorOutput },
    ActionFailed { id: OperatorId, error: ProtocolError },

    // Threshold crossings
    TokenThreshold { count: usize, limit: usize },
    TurnThreshold { count: u32, limit: u32 },
    BudgetThreshold { spent: Decimal, limit: Decimal },

    // Time
    Timer { id: String },
    Timeout { kind: TimeoutKind },

    // External
    Signal { kind: String, payload: Value },

    // Lifecycle
    LoopEnding { outcome: Outcome },

    // Escape hatch
    Custom { kind: String, payload: Value },
}
```

### ContextOp trait

```rust
/// A composable context transformation triggered by system events.
#[async_trait]
pub trait ContextOp: Send + Sync {
    /// Which events trigger this operation.
    fn trigger(&self) -> Trigger;

    /// Transform context in response to the event.
    async fn apply(
        &self,
        event: &AgentEvent,
        ctx: &mut Context,
        dispatch_ctx: &DispatchContext,
    ) -> OpResult;
}
```

### OpResult

```rust
pub enum OpResult {
    /// Context was (possibly) mutated. Continue processing.
    Continue,
    /// Stop the loop with this reason.
    Halt(String),
    /// Pause the loop, waiting for external input.
    Suspend(WaitReason),
    /// This op doesn't apply to this event. Skip to next.
    Skip,
}
```

### Trigger (composable predicates)

```rust
pub enum Trigger {
    /// Match a specific event kind.
    Event(EventKind),
    /// All sub-triggers must match.
    All(Vec<Trigger>),
    /// Any sub-trigger must match.
    Any(Vec<Trigger>),
    /// Arbitrary predicate over event and context.
    When(Arc<dyn Fn(&AgentEvent, &Context) -> bool + Send + Sync>),
    /// Fires on every event.
    Always,
}

/// Discriminant-only enum for matching AgentEvent variants.
pub enum EventKind {
    LoopStarted,
    BeforeInference,
    AfterInference,
    ActionRequested,
    ActionCompleted,
    ActionFailed,
    TokenThreshold,
    TurnThreshold,
    BudgetThreshold,
    Timer,
    Timeout,
    Signal,
    LoopEnding,
    Custom,
}
```

### Pipeline (reactive engine)

```rust
pub struct Pipeline {
    ops: Vec<Arc<dyn ContextOp>>,
}

impl Pipeline {
    pub fn new() -> Self;

    /// Add a context operation.
    pub fn add(mut self, op: impl ContextOp + 'static) -> Self;

    /// Evaluate all ops whose trigger matches the event.
    /// Runs in registration order. First Halt/Suspend wins.
    pub async fn emit(
        &self,
        event: &AgentEvent,
        ctx: &mut Context,
        dispatch_ctx: &DispatchContext,
    ) -> OpResult;
}
```

### Built-in ContextOps

These ship with the context engine crate:

| Op | Trigger | Effect |
|---|---|---|
| `BudgetGuard` | `BeforeInference` | Check turn/cost/token limits, Halt if exceeded |
| `CompactOnThreshold` | `TokenThreshold` | Run compaction strategy on context |
| `InjectSystemPrompt` | `LoopStarted` | Set system prompt from config or dynamic fn |
| `ApprovalGate` | `ActionRequested` | Check approval policy, Suspend if needed |
| `ToolFilter` | `BeforeInference` | Filter capabilities based on context state |
| `MetricsRecorder` | `Always` | Record telemetry for every event |
| `OutputSanitizer` | `ActionCompleted` | Scan tool results for secret patterns |

### ContextOp builder API

```rust
pub fn on(kind: EventKind) -> ContextOpBuilder;

impl ContextOpBuilder {
    pub fn when(self, f: impl Fn(&AgentEvent, &Context) -> bool) -> Self;
    pub fn apply(self, f: impl AsyncFn(&AgentEvent, &mut Context, &DispatchContext) -> OpResult) -> impl ContextOp;
    pub fn chain(self, ops: Vec<Arc<dyn ContextOp>>) -> impl ContextOp;
}
```

---

## Layer 1: Behaviours

### SyncOperator (convenience for simple tools)

```rust
/// For tools and simple operators that compute and return.
/// Blanket-implemented as Operator via spawn + single Completed event.
#[async_trait]
pub trait SyncOperator: Send + Sync {
    fn descriptor(&self) -> CapabilityDescriptor;

    async fn execute(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<OperatorOutput, ProtocolError>;
}

// Blanket impl: every SyncOperator is an Operator.
#[async_trait]
impl<T: SyncOperator + 'static> Operator for T {
    fn descriptor(&self) -> CapabilityDescriptor {
        SyncOperator::descriptor(self)
    }

    async fn handle(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<OperatorHandle, ProtocolError> {
        // Create channel, spawn execution, send Completed event.
        // Implementation detail: single-event handle optimization.
    }
}
```

`#[skg_tool]` macro generates `SyncOperator` impls.

### Router (name-based dispatch)

```rust
pub struct Router {
    routes: HashMap<OperatorId, Arc<dyn Operator>>,
}

impl Router {
    pub fn new() -> Self;
    pub fn route(mut self, id: impl Into<OperatorId>, op: Arc<dyn Operator>) -> Self;
    pub fn get(&self, id: &OperatorId) -> Option<&Arc<dyn Operator>>;
    pub fn capabilities(&self) -> Vec<CapabilityDescriptor>;
}

// Router implements Operator:
// - descriptor() returns a meta-descriptor listing all routes
// - handle() looks up ctx.operator_id in routes, delegates to child
// - Returns NotFound if no route matches
```

### AgentLoop (the agentic behaviour)

```rust
/// Callbacks for an agentic loop. The developer's extension point.
#[async_trait]
pub trait AgentBehaviour: Send + Sync {
    /// Assemble initial context. Called once per handle() invocation.
    async fn init_context(
        &self,
        input: &OperatorInput,
        ctx: &DispatchContext,
    ) -> Context;

    /// Which capabilities are available this turn.
    /// Called before each inference to build the tool list.
    fn capabilities(&self, ctx: &Context) -> Vec<CapabilityDescriptor>;

    /// After inference: decide what to do next.
    async fn handle_response(
        &self,
        response: &InferResponse,
        ctx: &mut Context,
    ) -> LoopDecision;

    /// After an action (tool/sub-agent) completes: process the result.
    async fn handle_action_result(
        &self,
        action: &OperatorId,
        result: &OperatorOutput,
        ctx: &mut Context,
    ) -> LoopDecision;
}

pub enum LoopDecision {
    /// Keep looping.
    Continue,
    /// Done. Return this output.
    Complete(OperatorOutput),
    /// Pause. Waiting for external input.
    Suspend(WaitReason),
    /// Hand off to another operator.
    Delegate(OperatorId, OperatorInput),
}
```

```rust
/// The generic agentic loop. Implements Operator.
pub struct AgentLoop<P: Provider, B: AgentBehaviour> {
    provider: P,
    behaviour: B,
    router: Arc<dyn Operator>,
    pipeline: PipelineFactory,
    descriptor: CapabilityDescriptor,
}
```

The AgentLoop implements Operator by running a dumb cycle that emits
AgentEvents through the Pipeline at every boundary:

```
init_context()
pipeline.emit(LoopStarted)
loop:
    pipeline.emit(BeforeInference)
    capabilities = behaviour.capabilities()
    compiled = ctx.compile(capabilities)
    response = provider.infer(compiled)
    pipeline.emit(AfterInference { response })
    decision = behaviour.handle_response(response, ctx)
    match decision:
        Complete/Suspend/Delegate → break
        Continue → proceed to actions
    for action in response.actions():
        pipeline.emit(ActionRequested { id, input })
        result = router.handle(action_input, dispatch_ctx)
        pipeline.emit(ActionCompleted/ActionFailed { id, result })
        decision = behaviour.handle_action_result(id, result, ctx)
        match decision:
            Complete/Suspend/Delegate → break
            Continue → next action
    check thresholds → emit TokenThreshold/TurnThreshold/BudgetThreshold
pipeline.emit(LoopEnding { outcome })
```

### Supervisor (fault-tolerant operator tree)

```rust
pub struct Supervisor {
    children: Vec<ChildSpec>,
    strategy: RestartStrategy,
}

pub struct ChildSpec {
    pub id: OperatorId,
    pub operator: Arc<dyn Operator>,
    pub restart: RestartPolicy,
}

pub enum RestartStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
}

pub enum RestartPolicy {
    Permanent,  // always restart
    Transient,  // restart only on failure
    Temporary,  // never restart
}
```

Supervisor implements Operator. It routes invocations to children by name
and monitors their health. On child failure, it applies the restart strategy.

### StateMachine (state-driven agents)

```rust
#[async_trait]
pub trait StateMachineBehaviour: Send + Sync {
    type State: Send + Sync + Clone;

    fn initial_state(&self) -> Self::State;

    async fn handle_event(
        &self,
        state: &Self::State,
        event: &AgentEvent,
        input: &OperatorInput,
        ctx: &mut Context,
        dispatch_ctx: &DispatchContext,
    ) -> (Self::State, LoopDecision);
}
```

---

## Environment Model

### EnvironmentProvider trait

```rust
#[async_trait]
pub trait EnvironmentProvider: Send + Sync {
    /// Can this provider satisfy the given spec?
    fn supports(&self, spec: &EnvironmentSpec) -> bool;

    /// Provision an environment and return an operator that routes into it.
    /// The returned operator wraps `inner` with the environment's isolation.
    async fn provision(
        &self,
        spec: &EnvironmentSpec,
        inner: Arc<dyn Operator>,
    ) -> Result<Arc<dyn Operator>, EnvError>;

    /// Tear down a provisioned environment.
    async fn teardown(&self, env_id: &str) -> Result<(), EnvError>;
}
```

Implementations are separate crates:
- `skg-env-local`: no isolation, pass-through (dev mode)
- `skg-env-docker`: container isolation (future)
- `skg-env-nix`: Nix sandbox (future)

The core framework provides the trait and `EnvironmentSpec`. It does not
import Docker, Nix, or any backend-specific dependencies.

### Credential flow

1. `EnvironmentSpec` declares `CredentialRef` requirements
2. `EnvironmentProvider::provision()` resolves refs against secret store
3. Provider injects credentials into the environment (env var, file mount, sidecar)
4. Operator code inside the environment accesses services normally
5. The LLM context never contains raw credentials

---

## Security Architecture

### Tier 1: Secrets the LLM never touches

Provider API keys, service tokens, database passwords.

Architecture: sidecar proxy intercepts outbound calls, injects credentials
on the wire. The credential exists only in the proxy's memory. The proxy is
not LLM-controllable.

Layer 0 models this via `CredentialInjection::Sidecar`.

### Tier 2: Secrets tools need but the LLM doesn't

Database connection strings, internal API endpoints with auth.

Architecture: operators receive credentials via constructor injection.
Tool input schemas never include credential fields. Error messages are
sanitized — no connection strings, no usernames, no host details.

Built-in `OutputSanitizer` ContextOp scans action results for patterns
matching known secret formats before they enter the context.

### Tier 3: Data the LLM must reason about

User emails, financial records, medical notes.

Architecture: cannot prevent exfiltration in general. Mitigations:
- `OutputSanitizer` ContextOp (pattern-based, bypassable)
- Network restrictions on agent environment
- Behavioral monitoring via `MetricsRecorder` ContextOp
- Least-privilege tool sets via `ToolFilter` ContextOp
- Human-in-the-loop via `ApprovalGate` ContextOp

Documentation must be explicit about what is and isn't guaranteed.

---

## Crate Map (post-redesign)

### Core (layer0)

- `Operator` trait (streaming-first)
- `OperatorHandle`, `OperatorEvent`, `Artifact`
- `OperatorInput`, `OperatorOutput`, `Outcome`
- `DispatchContext`
- `CapabilityDescriptor`, `CapabilitySource`
- `EnvironmentSpec`, `EnvironmentProvider` trait
- `Intent`, `ExecutionEvent`
- `Content`, `ProtocolError`
- All ID types

### Context Engine (skg-context-engine)

- `Context` (mutable substrate)
- `AgentEvent` (system event enum)
- `ContextOp` trait, `Trigger`, `OpResult`
- `Pipeline` (reactive event engine)
- `AgentLoop` + `AgentBehaviour` (agentic loop behaviour)
- `SyncOperator` trait + blanket Operator impl
- `Router` (name-based dispatch)
- `Supervisor` + `ChildSpec` + `RestartStrategy`
- `StateMachine` + `StateMachineBehaviour`
- `CompileConfig`, `CompiledContext`
- Built-in ContextOps (BudgetGuard, CompactOnThreshold, etc.)

### Tool infrastructure (skg-tool, skg-tool-macro)

- `#[skg_tool]` macro (generates SyncOperator impls)
- Schema derivation utilities
- `skg-tool/src/lib.rs` re-exports `SyncOperator` and schema helpers

### Everything else

Provider, state, secret, auth, MCP, orch crates: unchanged in this pass.
They will need minor updates to import paths (DispatchHandle → OperatorHandle).

---

## Implementation Order

### Phase 1: Demolition

Delete all components listed in "What Gets Deleted." Fix compile errors by
stubbing or removing dependent code. Examples and tests will be broken;
that's expected.

### Phase 2: Foundation

1. New `Operator` trait in layer0
2. Rename `DispatchHandle` → `OperatorHandle`, `DispatchEvent` → `OperatorEvent`
3. `AgentEvent` enum in skg-context-engine
4. `ContextOp` trait + `Trigger` + `OpResult` in skg-context-engine
5. Redesign `Pipeline` as reactive event engine

### Phase 3: Behaviours

1. `SyncOperator` trait + blanket Operator impl
2. `Router` struct implementing Operator
3. `AgentLoop` + `AgentBehaviour`
4. `Supervisor` (can be deferred if not immediately needed)

### Phase 4: Reconnect

1. Retarget `#[skg_tool]` macro for SyncOperator
2. Rebuild examples against new APIs
3. Rebuild compute runtime integration as SyncOperator tool
4. Fix worker.py `__SKG_RESULT` bug, embed via `include_str!`
5. Full test suite, clippy, architecture fitness checks

---

## Proving Tests

- Every Operator impl (tool, agent, router, supervisor) composes with every other
- A SyncOperator tool is callable from inside an AgentLoop via Router
- ContextOps fire on correct events and mutate context as expected
- Pipeline halts the loop when a ContextOp returns Halt
- Pipeline suspends the loop when a ContextOp returns Suspend
- AgentLoop emits events at every boundary point
- Router returns NotFound for unregistered operators
- OperatorHandle streams events and collects terminal output
- CapabilityDescriptor is derivable from any Operator
- EnvironmentProvider wraps an Operator transparently (local provider)

---

## Migration from Current Architecture

This is a breaking change to Layer 0 and Layer 1. All consumers must update.

No backwards-compatibility shims. No gradual migration. Full cutover.

The v2 branch is the working branch. All 17 prior specs describe wire types
and semantics that survive. This spec describes the runtime architecture
that replaces the prior Operator/Dispatcher/ToolDyn/Environment model.
