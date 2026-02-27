# Composable Agentic Architecture

For “where does this live?” (Neuron vs platform vs external infra), see
`docs/architecture/platform-scope-mapping.md`.

## The Core Idea

Every agentic AI system makes the same 23 architectural decisions (enumerated in the Agentic Decision Map). Today, each framework bundles its own answers to all 23 into a monolith — if you want Temporal's durability, you buy into its entire programming model; if you want Claude Code's tool execution, you buy into its entire agent runtime. You can't mix Temporal's crash recovery with NanoClaw's container isolation and Aider's repo-map context assembly.

The composable alternative: define protocol boundaries between the layers so that each decision can be answered independently and implementations can be swapped. This document defines those boundaries, identifies where clean separation is possible and where it isn't, and describes what a system built on these principles would look like.

This is a living design document. It will be updated as new patterns emerge and as the open questions at the bottom are resolved through research and implementation.

---

## Why Not a Framework? Why Not a DSL?

**The framework trap**: A universal framework that handles all patterns becomes either so flexible it's unusable (LangGraph — you're writing a state machine from scratch) or so opinionated it can't handle edge cases (CrewAI — great for the demo, collapses on novel tasks). The 23 decisions are largely independent — durability doesn't care about model selection, isolation doesn't care about compaction strategy. A monolith that bundles them forces you to accept its answer for decisions you didn't ask about.

**The DSL trap**: A domain-specific language encodes assumptions about what's expressible. SQL works because relational algebra is a clean mathematical domain. Agentic orchestration isn't — the right composition for a given task depends on runtime information (how complex is this request? did the first agent fail? what's the budget remaining?). A static DSL can't capture that without becoming Turing-complete, at which point you have a bad programming language with unfamiliar syntax.

**What works instead**: Composable primitives with protocol boundaries. The web didn't succeed because someone built a "universal web framework." It succeeded because HTTP is a protocol between client and server — any client, any server, any middleware. The right architecture for agentic systems follows the same principle: define thin interfaces between layers, let each layer be implemented independently, and compose them through protocols.

---

## The Protocol Boundaries

Gap analysis of all 23 decisions from the Decision Map against proposed protocol boundaries reveals that a clean three-layer model (Turn / Orchestration / State) breaks on 14 of 23 decisions. The honest architecture requires **four protocols** for the clean boundaries and **two cross-cutting interfaces** for the concerns that span layers.

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │                                                     │   │
│   │   ┌────────────────────────────────────────────┐    │   │
│   │   │                                            │    │   │
│   │   │   ┌───────────────────────────────────┐    │    │   │
│   │   │   │                                   │    │    │   │
│   │   │   │     ① TURN PROTOCOL               │    │    │   │
│   │   │   │     what one agent does per cycle  │    │    │   │
│   │   │   │                                   │    │    │   │
│   │   │   └──────────────┬────────────────────┘    │    │   │
│   │   │                  │ hooks (⑤)               │    │   │
│   │   │   ④ ENVIRONMENT PROTOCOL                   │    │   │
│   │   │   isolation, credentials, resources        │    │   │
│   │   │                                            │    │   │
│   │   └──────────────────┬─────────────────────────┘    │   │
│   │                      │ turn interface               │   │
│   │   ② ORCHESTRATION PROTOCOL                          │   │
│   │   composition, durability, topology                 │   │
│   │                                                     │   │
│   └──────────────────────┬──────────────────────────────┘   │
│                          │ state interface                  │
│   ③ STATE PROTOCOL                                          │
│   persistence, retrieval, compaction                        │
│                                                             │
│   ⑥ LIFECYCLE INTERFACE                                     │
│   budget, compaction triggers, observability                │
│   (coordinates across all layers)                           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Protocol ①: Turn

The atomic unit — what one agent does in one cycle. Receives input, assembles context, reasons, acts, produces output.

**Interface**:

```
TurnInput:
  message:      string | structured       # the task/query/signal
  trigger_type: user | task | signal | schedule | system_event
  session_id:   optional<string>          # for conversation continuity
  config:       TurnConfig                # all the knobs

TurnConfig:
  identity:     path | string | null      # D2A: .md file, role string, or structural
  tools:        ToolSurface               # D2D: schemas, catalog ref, or map
  model:        ModelSpec                  # D3A: specific model or tier rule
  max_turns:    int                       # D5: inner loop limit
  budget:       BudgetSpec                # D5: cost limit for this turn

TurnOutput:
  message:      string | structured       # the response or result
  output_type:  text | tool_request | delegate | handoff | error
  metadata:     TurnMetadata              # cost, tokens, duration, tools_used

TurnMetadata:
  tokens_in:    int
  tokens_out:   int
  cost:         float
  tools_called: list<ToolCall>
  turns_used:   int
  exit_reason:  model_done | max_turns | budget | circuit_break | observer_halt
```

**What lives inside this protocol**: Context assembly (D2A-D2E), inference (D3A), tool execution logic (D4C backfill), exit conditions (D5). The turn runtime owns the ReAct while-loop.

**What does NOT live inside**: Isolation (that's Environment), durability/retry (that's Orchestration), memory persistence (that's State). The turn runtime doesn't know it's in a container, doesn't know it's being replayed, doesn't own its memory store.

**The dependency**: The turn runtime READS from the State Protocol during context assembly (D2B history, D2C memory). It does not write directly — it returns metadata about what should be persisted, and the calling layer decides when to write. This keeps the turn runtime stateless from its own perspective.

```
# The turn runtime's view of state — read-only during assembly
StateReader:
  read_hot(scope)    → HotMemory       # CLAUDE.md, identity files
  read_warm(scope, key) → WarmMemory   # specific files, git paths
  search_cold(scope, query) → Results  # search index hits
  read_history(session_id) → Messages  # conversation history
```

### Protocol ②: Orchestration

How turns from different agents relate to each other, and how execution survives failures. Durability and composition are inseparable — Temporal replay IS orchestration IS crash recovery. They're the same system. Any attempt to separate them creates a leaky abstraction.

**Interface**:

```
OrchestrationSpec:
  agents:       map<name, AgentSpec>      # agent definitions
  topology:     Topology                  # C1-C4: how agents connect
  durability:   DurabilitySpec            # D3B/L3: what survives crashes
  policies:     OrchestrationPolicies     # retry, timeout, budget

Topology:
  pattern:      chain | fan_out | fan_in | delegate | handoff | custom
  edges:        list<Edge>                # who talks to whom
  observers:    list<ObserverSpec>         # C5: who watches

Edge:
  from:         agent_name
  to:           agent_name
  context:      full | task_only | summary | structural  # C1
  result:       direct | summary | two_path | signal     # C2

AgentSpec:
  turn_config:  TurnConfig                # per-agent turn configuration
  lifecycle:    ephemeral | long_lived | conversation_scoped  # C3
  environment:  EnvironmentSpec           # per-agent isolation config

DurabilitySpec:
  mode:         none | checkpoint | event_replay | durable_execution
  retry:        RetryPolicy               # D3C
  heartbeat:    duration | null           # for long-running turns
  continue_as_new_threshold: int | null   # event history size limit

OrchestrationResult:
  outputs:      map<agent_name, TurnOutput>
  total_cost:   float
  total_tokens: int
  duration:     duration
  events:       list<Event>               # full execution trace
  recovery_point: optional<RecoveryState> # for crash recovery
```

**What lives inside this protocol**: Agent composition (C1-C4), durability (D3B), retry (D3C), crash recovery (L3), and the observer attachment point (C5). The orchestration engine owns the outer loop — when to start turns, how to connect their outputs, when to retry, how to recover.

**The key coupling**: Durability and orchestration cannot be separated because crash recovery requires knowledge of the composition topology. Temporal replays a workflow by re-executing the orchestration logic and skipping cached activity results. Event sourcing reconstructs state by replaying the orchestration decisions. The recovery mechanism IS the composition mechanism with cached shortcuts. Any architecture that tries to make durability a pluggable concern independent of orchestration will leak.

**Communication mechanisms** (C4) are internal to this protocol. The orchestration engine decides whether agents communicate via function call/return, signals, shared state, or event streams. This is the protocol's implementation detail, not an external interface.

### Protocol ③: State

How data persists and is retrieved across turns and sessions.

**Interface**:

```
StateWriter:
  write(scope, key, value, trigger)  → WriteResult
  delete(scope, key)                 → DeleteResult

StateReader:
  read(scope, key)                   → Value | null
  search(scope, query)               → list<Result>
  list(scope, prefix)                → list<Key>
  history(scope, key, range)         → list<VersionedValue>

StateCompactor:
  compact(scope, policy)             → CompactionResult
  
CompactionPolicy:
  strategy:    summarize | truncate | archive | continue_as_new
  trigger:     context_percent | event_count | time_interval
  flush_before: bool                  # pre-compaction memory flush

Scope:
  session:     string                 # conversation scope
  workflow:    string                 # workflow execution scope
  agent:       string                 # per-agent scope
  global:      string                 # cross-workflow scope
```

**What lives inside this protocol**: Memory storage (L1), retrieval (D2C read side), compaction execution (L2), and versioning/audit trail. The state layer owns the data.

**What this protocol does NOT decide**: *When* to write (that's Turn or Orchestration or Lifecycle), *what format* to persist in (that's the implementation — git, PostgreSQL, filesystem, vector store), or *when* to compact (that's the Lifecycle Interface coordinating between layers).

**Implementation spectrum**: A filesystem. A git repo. PostgreSQL. SQLite. A vector database. S3. All of these implement the same State Protocol interface. The choice of backend is an implementation decision, not an architectural one. Git gives you versioning and audit for free. PostgreSQL gives you transactions and search. The protocol doesn't care.

### Protocol ④: Environment

What wraps the turn runtime — isolation, credentials, and resource constraints. This is the most surprising finding from the gap analysis: isolation is neither turn logic nor orchestration logic. It's a separate concern that wraps the turn runtime, mediating between the agent's actions and the host system.

**Interface**:

```
EnvironmentSpec:
  isolation:    IsolationSpec             # D4A
  credentials:  CredentialSpec            # D4B
  resources:    ResourceSpec              # CPU, memory, disk, network

IsolationSpec:
  level:        none | permission_gate | sandbox | container | multi_layer
  boundaries:   list<Boundary>

Boundary:
  type:         process | container | gvisor | wasm | network_policy
  config:       BoundaryConfig            # layer-specific settings

CredentialSpec:
  mode:         in_process | mounted | boundary_injected | sidecar
  secrets:      list<SecretRef>           # what credentials are available
  
ResourceSpec:
  cpu_limit:    string | null
  memory_limit: string | null
  disk_limit:   string | null
  network:      NetworkPolicy | null      # allowed endpoints
  gpu:          GpuSpec | null
```

**What lives inside this protocol**: Tool execution isolation (D4A), credential handling (D4B), resource limits, network policy. The environment mediates every action the turn runtime takes that touches the outside world.

**The relationship to Turn**: The environment wraps the turn runtime. When the turn runtime executes a tool, the environment layer intercepts and mediates: Is this tool call allowed? Does it need credentials injected? Is it within resource limits? Is the network destination permitted? The turn runtime doesn't know about the environment — it just makes tool calls. The environment makes them safe.

**Why this is separate from Orchestration**: You might think "the orchestrator provisions the environment." And it does — the orchestration engine creates containers, applies network policies, configures sidecars. But the environment's *runtime mediation* (intercepting tool calls, injecting credentials, enforcing limits) operates at a different timescale and granularity than orchestration. Orchestration decides "agent X runs in a container with network policy Y." The environment then enforces that policy on every individual syscall and network request for the duration of the turn. Conflating these makes both harder to reason about.

### Interface ⑤: Hooks

Observation and intervention points inside the turn. This is what makes the turn protocol's opaque input→output boundary penetrable when needed.

**Interface**:

```
HookPoint:
  pre_inference      # before each model call
  post_inference     # after each model call, before tool execution
  pre_tool_use       # before each tool execution
  post_tool_use      # after each tool execution, before backfill
  pre_backfill       # before results enter context
  context_snapshot   # periodic read of current context state
  exit_check         # at each exit decision point

HookAction:
  continue           # proceed normally
  modify(delta)      # modify context, input, or tool call
  halt(reason)       # stop execution (observer tripwire)
  log(data)          # emit observability data without interrupting

Hook:
  point:    HookPoint
  handler:  (HookContext) → HookAction
  
HookContext:
  turn_state:    current context window contents (read-only)
  tool_call:     current tool call (if at tool hook point)
  model_output:  current model response (if at post-inference)
  metadata:      running cost, turn count, elapsed time
```

**Who uses hooks**:

| Consumer | Hook Points Used | Purpose |
|----------|-----------------|---------|
| **Observer/Guardrail (C5)** | All | Security monitoring, quality checks, course correction |
| **Orchestration (D3B)** | post_tool_use | Heartbeat to prove activity is alive |
| **Budget tracking (L4)** | post_inference | Update running cost, check limits |
| **Observability (L5)** | All | Tracing, logging, event emission |
| **Memory sync (L1)** | post_tool_use | Trigger memory writes after state-changing tools |

**Why hooks are a separate interface, not part of the Turn Protocol**: The turn runtime should not need to know who's watching. Hooks are registered externally — by the orchestration engine, the environment, or the observer — and the turn runtime simply calls them at the defined points. This is the observer pattern (GoF), applied at the architecture level. The turn runtime provides the hook points; other protocols provide the handlers.

**The critical constraint**: Hook handlers must not block indefinitely. A guardrail that calls an LLM to validate a tool call adds latency to every tool use. This is a design choice with cost — OpenAI's parallel guardrails trade thoroughness for latency by running guardrails concurrently with agent execution, accepting that the agent may have already acted by the time the guardrail triggers.

### Interface ⑥: Lifecycle

Coordination of concerns that span all four protocols. Budget, compaction, and observability each require information from multiple layers and produce decisions that affect multiple layers.

This is not a protocol in the same sense as the other four — there's no single "lifecycle service" that owns this. It's a coordination interface: a set of events and queries that flow between protocols to make joint decisions.

**Budget coordination**:
```
# Turn emits after each model call:
CostEvent { turn_cost: float, cumulative_turn_cost: float }

# Orchestration tracks across agents:
WorkflowBudget { 
  spent: float, 
  remaining: float, 
  per_agent: map<name, float> 
}

# Lifecycle decision (made by orchestration, informed by turn + state):
BudgetAction: continue | downgrade_model | halt_workflow | request_increase
```

**Compaction coordination**:
```
# Turn detects context pressure:
ContextPressure { fill_percent: float, tokens_used: int, tokens_available: int }

# Decision flow (involves all layers):
1. Turn detects context at 80% → emits ContextPressure
2. Lifecycle decides: compact now? (policy)
3. Turn executes pre-compaction flush via Hooks → State writes critical data (L1)
4. Turn or Orchestration executes compaction:
   - Turn: LLM summarization (replace old turns with summary)
   - Orchestration: continue-as-new (reset execution, carry state)
5. State: persist compaction result
6. Turn: resume with compacted context
```

**Observability coordination**:
```
# Every layer emits events through a common interface:
ObservableEvent:
  source:     turn | orchestration | state | environment
  type:       string
  timestamp:  datetime
  data:       any
  trace_id:   string       # correlates events across layers

# Consumers subscribe:
Dashboard:    queries Orchestration for live workflow state
Tracing:      collects events from all layers via trace_id
Audit log:    State persists all events for compliance
Alerting:     Environment monitors for security events
```

---

## Decision Coverage Map

Every decision from the Agentic Decision Map, mapped to which protocol owns it:

| # | Decision | Primary Owner | Also Involved | Clean? |
|---|----------|--------------|---------------|--------|
| D1 | Trigger | ② Orchestration | ① Turn (receives it) | ✅ |
| D2A | Identity | ① Turn | — | ✅ |
| D2B | History | ① Turn | ③ State (reads from) | ⚠️ Read dependency |
| D2C | Memory | ① Turn | ③ State (reads from) | ⚠️ Read dependency |
| D2D | Tools (schemas) | ① Turn | — | ✅ |
| D2D | Tools (execution) | ④ Environment | ① Turn (initiates) | ⚠️ Mediation |
| D2E | Context budget | ① Turn | — | ✅ |
| D3A | Model selection | ① Turn | — | ✅ |
| D3B | Durability | ② Orchestration | ① Turn (heartbeats via ⑤) | ⚠️ Hook coupling |
| D3C | Retry | ② Orchestration | ① Turn (signals non-retryable) | ⚠️ Error contract |
| D4A | Isolation | ④ Environment | — | ✅ |
| D4B | Credentials | ④ Environment | — | ✅ |
| D4C | Backfill | ① Turn | — | ✅ |
| D5 | Exit | ① Turn | ⑤ Hooks (observer halt), ⑥ Lifecycle (budget) | ⚠️ Multi-source |
| C1 | Child context | ② Orchestration | — | ✅ |
| C2 | Result return | ② Orchestration | — | ✅ |
| C3 | Lifecycle | ② Orchestration | — | ✅ |
| C4 | Communication | ② Orchestration | — | ✅ |
| C5 | Observation | ⑤ Hooks | ② Orchestration (attachment) | ⚠️ Cross-cutting |
| L1 | Memory writes | ③ State | ⑥ Lifecycle (when), ⑤ Hooks (trigger) | ⚠️ Coordination |
| L2 | Compaction | ⑥ Lifecycle | ① Turn + ② Orch + ③ State | ⚠️ Three-way |
| L3 | Crash recovery | ② Orchestration | ③ State (entangled) | ⚠️ Inseparable |
| L4 | Budget control | ⑥ Lifecycle | ① Turn + ② Orchestration | ⚠️ Coordination |
| L5 | Observability | ⑥ Lifecycle | All layers emit | ⚠️ Cross-cutting |

**Scorecard**: 12 clean, 11 with identified couplings. The couplings fall into four categories:

1. **Read dependencies** (D2B, D2C): Turn reads from State. Managed by giving the turn runtime a read-only StateReader interface — it can read, but writes flow through the Lifecycle Interface.
2. **Hook couplings** (D3B, D5, C5, L1): External concerns reach into the turn via hooks. Managed by the Hook Interface — the turn runtime provides hook points, other layers provide handlers.
3. **Coordination** (L1, L2, L4, L5): Decisions that require information from multiple layers. Managed by the Lifecycle Interface — events flow between layers, and a coordinator (which may be the Orchestration engine) makes joint decisions.
4. **Entanglement** (L3): Durability and orchestration are architecturally inseparable. Acknowledged, not fought — they're one protocol (②).

None of these couplings invalidate the architecture. They're managed interfaces, not leaky abstractions. The key property is preserved: **you can swap the implementation of any one protocol without rewriting the others**, as long as you respect the interfaces at the coupling points.

---

## What This Looks Like in Practice

### Example: Swap orchestration without changing anything else

Your turn runtime uses Claude via the Agent SDK. Your state is git-backed. Your environment is Docker containers. You've been running with no orchestration (direct function calls) and want to add Temporal for durability.

**What changes**: Protocol ② implementation. You wrap your existing turn invocations in Temporal activities, add retry policies, add heartbeat hooks.

**What doesn't change**: The turn runtime (still Claude SDK, same agent .md files, same tools). The state protocol (still git — Temporal doesn't replace your memory, it adds execution durability). The environment (still Docker — Temporal schedules the containers, but their isolation config is unchanged). The hooks (same hook handlers, now registered slightly differently).

### Example: Swap isolation without changing anything else

You've been running agents with no isolation (direct host execution) and want to add container isolation.

**What changes**: Protocol ④ implementation. Each agent turn now runs in a container. Tool calls are mediated by the container boundary. Credentials are injected via sidecar.

**What doesn't change**: The turn runtime (same code, same agent definitions — it doesn't know it's in a container). The orchestration (same composition topology, same durability — it just provisions containers instead of processes). The state protocol (same persistence — mounted into the container or accessed via network).

### Example: Add an observer without changing anything else

You want a security monitor that watches all agent tool calls and halts on suspicious patterns.

**What changes**: A new hook handler registered at the `pre_tool_use` point. An observer agent (or rule-based function) that evaluates each tool call.

**What doesn't change**: The turn runtime (doesn't know it's being watched). The orchestration (observer is attached via C5 config, topology is unchanged). The environment (observer runs alongside, doesn't modify isolation). The state (observer may write its own audit log, but doesn't modify agent state).

### Example: Declarative composition

A configuration format that maps to the protocol interfaces:

```yaml
# Each section maps to one protocol

turn:                                 # Protocol ①
  identity: ./agents/researcher.md
  tools: [web-search, file-read, grep]
  model: { default: sonnet, planning: opus }
  exit: { max_turns: 50, budget_usd: 2.00 }

orchestration:                        # Protocol ②
  pattern: orchestrator-workers
  agents:
    coordinator:
      turn: { identity: ./agents/coordinator.md, model: opus }
      lifecycle: long_lived
    worker_a:
      turn: { identity: ./agents/analyst.md, model: sonnet }
      lifecycle: ephemeral
    worker_b:
      turn: { identity: ./agents/writer.md, model: sonnet }
      lifecycle: ephemeral
  topology:
    coordinator:
      delegates_to: [worker_a, worker_b]
      context_transfer: task_only
      result_return: summary_plus_storage
  durability:
    mode: temporal
    retry: { max_attempts: 3, backoff: exponential }
    heartbeat: 60s
    continue_as_new_after: 5000_events

environment:                          # Protocol ④
  isolation: container
  boundaries:
    - type: network_policy
      allow: [api.anthropic.com, localhost]
  credentials:
    mode: sidecar
    secrets: [anthropic-api-key, github-token]
  resources:
    memory: 2Gi
    cpu: "1"

state:                                # Protocol ③
  backend: git
  scopes:
    session: ./memory/sessions/
    workflow: ./memory/workflows/
    global: ./memory/shared/
  compaction:
    strategy: summarize
    trigger: context_fill_80_percent
    flush_before: true

hooks:                                # Interface ⑤
  - point: pre_tool_use
    handler: security_monitor
  - point: post_tool_use
    handler: [heartbeat, memory_sync, telemetry]
  - point: post_inference
    handler: budget_tracker

observers:                            # C5, via Interface ⑤
  - name: budget_guardian
    type: rule_based
    watches: [cost_events]
    action: halt_if_budget_exceeded
  - name: quality_monitor
    type: llm_agent
    model: haiku
    watches: [tool_outputs]
    action: flag_and_log
```

Every section is independently swappable. Change `state.backend` from `git` to `postgresql` — nothing else changes. Change `environment.isolation` from `container` to `none` — the turn runtime doesn't notice. Add a new observer — existing agents are unmodified.

---

## What This Architecture Cannot Do

Honest constraints — these are fundamental, not limitations of a specific implementation:

**1. The orchestration-durability entanglement is real.** You cannot have "orchestration-agnostic durability." If you use Temporal for crash recovery, your composition logic must be Temporal workflows. If you use event sourcing, your composition logic must be replay-compatible. This coupling is architectural, not incidental. The declarative config above abstracts over it (`durability.mode: temporal`), but the implementation of Protocol ② is a single integrated system, not two composable pieces.

**2. Fine-grained durability requires turn runtime cooperation.** Coarse-grained durability (entire turn = one retryable activity) works without the turn runtime knowing. Fine-grained durability (each LLM call and tool call is a separate retryable activity) requires the turn runtime to expose its internal loop to the orchestration engine. Today, most agent SDKs don't expose this — the Agent SDK runs Claude Code as a subprocess with an opaque internal loop. Until SDKs expose model-provider interfaces (let orchestration intercept each LLM call), coarse-grained is the practical ceiling.

**3. Observer latency is a design tax.** Every hook handler adds latency to the inner loop. An LLM-based guardrail that validates each tool call adds hundreds of milliseconds per tool use. Rule-based observers are fast (~1ms) but limited in what they can catch. There's no free observation — you're trading throughput for safety/quality. The architecture makes this explicit (you configure which hooks run where), but it can't eliminate the tradeoff.

**4. Cross-layer coordination has a consistency boundary.** When compaction requires the turn to flush memory, the state layer to persist it, and the orchestration layer to manage continue-as-new, these three operations must succeed atomically or you risk data loss. The Lifecycle Interface coordinates this, but distributed coordination is hard. In practice, "flush to state first, then compact, then continue" is safe because the flush is durable — if the compact fails, the flushed state is still in the store. But the ordering matters, and the architecture must enforce it.

---

## Open Questions

### Protocol Design
- **What is the minimal Turn Protocol interface?** The current `TurnInput → TurnOutput` is clean, but the `StateReader` dependency means the turn isn't truly self-contained. Could the orchestration layer pre-assemble all context and pass it as part of TurnInput, making the turn fully stateless? Codex does this (sends full history every call). What are the tradeoffs at scale?
- **Should the Environment Protocol be split?** Isolation (container boundary), credentials (secret management), and resources (CPU/memory limits) might be three separate concerns that happen to co-locate today. Would separating them improve composability or just add interface overhead?
- **What's the right hook granularity?** The current seven hook points (pre/post inference, pre/post tool, pre-backfill, context snapshot, exit check) emerged from implementation analysis. Is this too many? Too few? Are there hook points we're missing?

### Implementation
- **Can the declarative config fully specify a system?** The YAML example above looks clean, but does it cover edge cases — dynamic topology changes, adaptive model selection, conditional observer attachment? Where do escape hatches to code become necessary?
- **What's the performance overhead of protocol boundaries?** Each boundary is a serialization/deserialization point. For high-frequency inner-loop operations (tool backfill happens every few seconds), is the protocol overhead measurable? When does it matter?
- **How do you test a composed system?** Unit tests work within protocols. Integration tests across protocols require a test harness that can mock each protocol independently. What does that test harness look like?

### Research Areas (from the Decision Map)
- **Optimal capability surface compression** (D2D): Aider's tree-sitter repo map works for code. What's the equivalent for API surfaces, database schemas, document collections?
- **Observer architecture taxonomy** (C5): What are all the intervention points an observer can have? When does observer cost pay for itself? Can observers observe observers?
- **Active context steering**: Can an observer continuously enrich an agent's context mid-execution (not just halt)? What are the mechanisms and safety implications?
- **Context window degradation curves** (D2E): How does performance degrade as context fills? Is it linear, exponential, or threshold-based?
- **Pre-compaction flush completeness** (L2): How do you know the flush captured everything important? Can flush quality be validated automatically?
- **Memory conflict resolution** (L1): When multiple agents write to shared memory concurrently, what's the right resolution strategy?
- **Convergence tracking**: Do new agentic systems introduced in 2026 map onto this 4+2 architecture, or do they introduce genuinely new protocol boundaries?
