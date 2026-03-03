# Agent Constitution

## Preamble

This document governs architectural decisions in this codebase. Its audience is any agent — human or artificial — working in this repository. It defines the values, principles, and positions that all contributions must conform to.

This is a living document. The problem space will evolve. When it does, update the positions here first, then propagate to code. The constitution leads; the code follows.

This constitution derives from the [Agentic Decision Map](docs/architecture/agentic-decision-map-v3.md), which enumerates all 23 architectural decisions any agentic system must answer. Where the decision map is analytical (here is the design space), this document is normative (here is what we believe, and why).

### How to Use This Document

**When implementing**: Before writing code that touches any of the 23 decisions, find the relevant section below. Your implementation must conform to the stated position. If the position says "protocol boundary — implementor chooses," your code must preserve that choice for users rather than hardcoding an answer.

**When reviewing**: Check each change against the decision it affects. A change that violates a position here is a bug, even if it compiles and passes tests.

**When the position is wrong**: Update this document first. Make the case in the rationale. Get agreement. Then change the code. Do not let the code drift from the constitution — that path leads to implicit decisions nobody can find.

### Authority Hierarchy

When sources conflict, higher authority wins:

1. **This constitution** — foundational values and architectural positions
2. **Specs** (`specs/`) — detailed behavioral requirements
3. **Rules** (`rules/`) — operational constraints and process
4. **Agent judgment** — for situations not covered above

A spec may refine a constitutional position but must not contradict it. A rule may constrain process but must not override a spec. Agent judgment fills gaps but must not invent behavior that contradicts any of the above.

---

## Core Values

These are ordered by priority. When values conflict, the higher-ranked value prevails.

### 1. Composability Over Convenience

Every engineering decision in an agentic system should be answerable independently. Durability should not force a model selection. Isolation should not dictate communication patterns. Memory strategy should not constrain orchestration topology.

The alternative — bundling answers to unrelated decisions into a monolith — is the framework trap. It trades short-term convenience for long-term rigidity. When you need Temporal's durability but not its programming model, or NanoClaw's container isolation but not its context assembly, a monolith forces you to accept answers to questions you didn't ask.

The composable alternative: define protocol boundaries between layers so that implementations can be swapped. The web succeeded not because someone built a universal web framework, but because HTTP is a protocol between client and server — any client, any server, any middleware. We follow the same principle.

**What this means in practice**: When you find yourself coupling two concerns that the 23 decisions list separately, stop. Introduce an interface. The cost of one more trait is far less than the cost of an implementation that can't be replaced.

### 2. Declaration Separated From Execution

Operators reason and declare intent. Orchestrators and environments execute. No component should both decide what to do and carry it out. This is the effects boundary, and it is sacred.

An operator that directly writes to a state store has coupled reasoning to infrastructure. An operator that emits a `WriteMemory` effect has declared intent while preserving the orchestrator's freedom to execute that write against git, PostgreSQL, or an in-memory store. The operator doesn't know and doesn't care.

This separation is what makes composition possible. If operators execute their own effects, swapping the state backend requires rewriting every operator. If operators declare effects, swapping the backend is a configuration change.

**What this means in practice**: When you see an operator importing a concrete state store, database client, or filesystem API, that's a violation. Effects are the only channel from reasoning to the outside world.

### 3. Slim Defaults, Opt-In Complexity

The simplest useful configuration must work without understanding the full system. Sequential tool execution. No steering. No streaming. Local best-effort effects. One model. No observer.

Advanced behavior — barrier scheduling, parallel tool execution, mid-loop steering, durable execution, multi-model routing, continuous observation — is opt-in via small, composable traits. Each capability is independently adoptable. No boolean soup. No configuration matrices where enabling feature A requires understanding features B through F.

**What this means in practice**: Every new capability must work as an additive layer. If adopting streaming requires changing how non-streaming tools work, the design is wrong. If enabling durability requires modifying operators that don't care about durability, the abstraction leaks.

### 4. Protocol Stability as the Foundation

Layer 0 — the protocol traits and wire types — is the stability contract. It changes slowly, additively, and with version discipline. Everything above Layer 0 can change freely, as long as it satisfies the protocols.

This is the foundation that makes independent implementation possible. If the `Operator` trait changes, every operator breaks. If `neuron-op-react` changes its internal loop, nothing else notices. The stability boundary must be defended with the same discipline as a public API contract.

**What this means in practice**: Adding a method to a Layer 0 trait is a breaking change that affects every implementor. Adding a field to a Layer 0 message type requires serde compatibility. These changes need version planning. Changes above Layer 0 are routine.

### 5. Explicit Over Implicit

Exit conditions are enumerated, not emergent. Execution strategies are declared, not inferred. Steering boundaries are defined, not discovered. Lifecycle coordination flows through observable events, not hidden state.

A system where behavior depends on ambient configuration, implicit defaults, or undocumented interactions between components is a system that no agent — human or artificial — can reason about reliably. When something goes wrong, the debugging path must be traceable through explicit decisions, not through reverse-engineering emergent behavior.

**What this means in practice**: Every exit reason has a name. Every hook point is documented. Every effect variant is in the enum. If behavior exists, it's in a type that can be inspected, logged, and tested.

---

## The Turn

The turn is the atomic unit of agency: one cycle of receive, reason, act. Every agent, regardless of pattern, processes turns. These decisions are universal.

### Triggers and Context Assembly (D1, D2A–D2E)

**The principle**: An agent's behavior is shaped entirely by its assembled context. What you include creates capability; what you exclude creates safety. The trigger type informs assembly strategy but does not constrain it.

**Positions**:

- **D1 — Triggers**: The orchestration protocol owns trigger routing. The turn protocol receives triggers through a uniform `OperatorInput` regardless of source (user message, task assignment, signal, schedule, system event). Operators must not special-case trigger types.

- **D2A — Identity**: Identity specification is the turn protocol's responsibility. Supported range: from rich prompt injection (multi-section system prompts) to structural constraint (environment-as-identity). The protocol does not mandate a single approach — but the chosen identity must be explicitly configured, not implicitly assumed.

- **D2B — History**: Conversation state is maintained by the turn runtime with a read dependency on the state protocol. The turn reads history; it does not write history directly. Writes flow through effects. The state backend (in-memory, event-sourced, database-backed) is swappable without turn runtime changes.

- **D2C — Memory**: Three tiers (hot, warm, cold) with distinct retrieval characteristics. Hot memory is a tax on every turn. Cold memory is free until needed, then costs a tool call. The architectural decision — what goes in each tier — is a per-agent configuration choice, not a framework-level mandate.

- **D2D — Tools**: Tool definitions are the source of truth for execution metadata, including concurrency hints (Shared/Exclusive). Tool surface injection (static schemas vs. lazy catalog vs. compressed structural map) is a turn-level configuration. Tool execution is mediated by the environment protocol, not the turn runtime.

- **D2E — Context budget**: The turn runtime owns allocation. Reserve space for compaction (the context window must never be 100% full before compaction runs). Budget allocation is a turn-level concern; lifecycle coordination (L4) observes but does not directly control it.

### Inference (D3A–D3C)

**The principle**: Model selection, durability, and retry strategy affect cost, reliability, and capability. These are separable decisions, but durability and retry are architecturally entangled with orchestration — this coupling is real and acknowledged, not fought.

**Positions**:

- **D3A — Model selection**: The turn protocol owns model selection. Multi-tier routing (strong model for planning, fast model for execution) is a turn-level configuration. The protocol supports single-model, two-tier, and three-tier patterns without mandating one.

- **D3B — Durability**: The orchestration protocol owns durability. The turn protocol cooperates via heartbeat hooks (interface 5). Local orchestration provides no durability (crash = restart). Durable orchestration (Temporal, Restate) provides checkpoint or replay recovery. This is a deployment choice, not a code change — the same operator works in both environments.

- **D3C — Retry**: The orchestration protocol owns retry policy. The turn protocol classifies errors as retryable vs. non-retryable (budget exhaustion and safety refusals must not retry). A single retry authority must exist — SDK retry and orchestrator retry must not conflict.

### Tool Execution and Response Handling (D4A–D4C)

**The principle**: Tool execution is where the agent's reasoning meets the real world. The trust boundary (how much you trust generated code), the credential boundary (what secrets the agent can see), and the integration boundary (how results re-enter context) are three independent decisions mediated by the environment protocol.

**Positions**:

- **D4A — Isolation**: The environment protocol owns isolation. The full spectrum (no isolation through multi-layer sandbox) is supported via protocol implementations. The turn runtime does not know its isolation level — it calls tools the same way regardless. Moving from no isolation to container isolation is an environment swap, not a turn runtime change.

- **D4B — Credentials**: The environment protocol owns credential injection. Agents should not see raw secrets when avoidable. The preferred pattern is boundary injection (credentials added at the environment edge, stripped from context). Tests must prove no secret leakage through tool outputs or logs.

- **D4C — Backfill**: The turn protocol owns result integration. Tool outputs constitute the majority of what the model reasons over. Formatting tool results for model consumption is a first-class engineering concern — treat it with the same care as prompt design. Security-sensitive content should be stripped before backfill.

### Exit Conditions (D5)

**The principle**: Production systems must layer multiple independent stop conditions. A single condition creates failure modes — "model signals done" alone lets the model loop forever; "max turns" alone cuts off genuinely hard tasks.

**Position**: The turn protocol evaluates exit conditions, but exit can be triggered from multiple sources: the model (no more tool calls), configuration (max turns, budget), hooks (observer halt), and lifecycle (budget exhaustion). The `ExitReason` enum is explicit — every exit path has a named variant. Conflicts between sources are resolved by priority: safety halt > budget > max turns > model done.

---

## Composition

Composition is how turns from different agents relate to each other. Not every system needs composition, but every system that does will face these decisions.

### The Six Primitives

All composition patterns are built from six atomic operations: **Chain**, **Fan-out**, **Fan-in**, **Delegate**, **Handoff**, and **Observe**. The first five follow a rule: one agent's output becomes another agent's input. Observe breaks this rule — the observer watches the process concurrently and may intervene at any point.

When designing a new composition pattern, decompose it into these primitives first. If it cannot be expressed as a combination of the six, that's a signal the framework may need a new primitive — update this constitution.

### Context Transfer (C1)

**The principle**: The context boundary between parent and child is the most consequential composition decision. Too much context wastes tokens and risks confusion. Too little leaves the child unable to do its job.

**Position**: The orchestration protocol owns context transfer. Task-only injection is the default — the child gets a task description and selected data, not the parent's full conversation. Context boundaries should be enforced by infrastructure (separate process, separate context window), not by prompt instruction (telling the model to ignore parent context is fragile). Summary injection is preferred over full context inheritance for multi-level delegation.

### Result Routing (C2)

**The principle**: How results flow back determines the parent's token cost and audit trail quality.

**Position**: The orchestration protocol owns result routing. The preferred pattern is two-path: full output to persistent storage (for audit and retrieval), summary to parent context (for token efficiency). Direct injection of full child output is acceptable for simple cases but does not scale.

### Agent Lifecycle (C3)

**The principle**: Child agent lifetime determines resource cost and communication capability.

**Position**: The orchestration protocol owns lifecycle. Ephemeral (fire-and-forget) is the default. Long-lived agents are opt-in for cases requiring follow-up communication. Conversation-scoped handoff (child inherits and continues the conversation) is a distinct pattern from delegation — the parent terminates.

### Communication (C4)

**The principle**: Communication mechanism determines latency, durability, and ordering guarantees.

**Position**: The orchestration protocol owns communication. Function call/return (synchronous) is the default for local orchestration. Signals (async, durable) are for distributed orchestration. Shared state mutation and event streams are supported but require explicit ordering and conflict resolution strategies.

### Observation (C5)

**The principle**: The observer primitive is orthogonal to topology — you can observe any pattern. It is the only primitive that provides concurrent visibility into the process, enabling course-correction before the agent's output is final.

**Position**: Observation is mediated by hooks (interface 5), attached by the orchestration protocol. Three manifestations are supported: oracle (pull-based, advisory), guardrail (checkpoint-based, can halt), and observer agent (continuous, can halt/inject/modify). Hook handlers must not block indefinitely — this is a contract, not enforced by the protocol. Latency-sensitive systems should prefer parallel guardrails over sequential ones.

---

## Lifecycle

These concerns span multiple turns and multiple agents. They cut across all protocols and require coordination.

### Memory Persistence (L1)

**The principle**: Memory is both a read concern (context assembly) and a write concern (lifecycle). The critical question is: when does the agent commit state, and what does it write?

**Position**: The state protocol owns persistence. Write timing is coordinated by the lifecycle interface, triggered by hooks. Pre-compaction flush is mandatory — before destroying old conversation turns, the agent must write important state to persistent storage. This is the single most important lifecycle mechanism for long-running agents. On agent termination, capture work product before the context window is destroyed.

### Compaction (L2)

**The principle**: Every system with a finite context window eventually faces context growth. The strategy chosen determines what survives and what is lost.

**Position**: Compaction is a three-way coordination between the turn protocol (detects pressure, executes summarization), the orchestration protocol (may execute continue-as-new), and the state protocol (persists results). LLM summarization is the default strategy. The compaction reserve (context space held back for the summarization call itself) must never be zero — a system that fills context to 100% before compacting has no room to run compaction.

### Crash Recovery (L3)

**The principle**: Durability and orchestration are architecturally inseparable. You cannot have orchestration-agnostic durability.

**Position**: Crash recovery is owned by the orchestration protocol because it is entangled with it. Local orchestration provides no recovery (crash = lost work, acceptable for short-lived tasks). Durable orchestration provides replay recovery (acceptable for long-running tasks). The same operator code works in both — the durability boundary is at the orchestration level, not the turn level. This entanglement is architectural, not incidental — we accept it rather than fighting it with leaky abstractions.

### Budget Governance (L4)

**The principle**: Runaway agents burn money. Budget control requires a single authority with visibility across all layers.

**Position**: Budget governance is coordinated by the lifecycle interface. The turn emits cost events. The orchestration tracks aggregate cost across agents. The lifecycle coordinator (which may be the orchestration engine) makes halt/continue/downgrade decisions. A single authority for limits: budget, time, and turn counts live in the exit controller. Planners observe remaining budget (read-only) — they do not set it.

### Observability (L5)

**The principle**: Every layer emits events. Without correlation IDs and event ordering, distributed tracing across agents is impossible.

**Position**: Observability is cross-cutting — all layers emit through a common `ObservableEvent` interface with source, type, timestamp, data, and trace ID. Consumers (dashboards, tracing, audit logs, alerting) subscribe independently. Hook-based observation (interface 5) provides the attachment points. The overhead must be proportional — structured tracing for production, full event logging for debugging.

---

## Decision Index

Quick reference. For reasoning, read the narrative sections above.

| # | Decision | Position |
|---|----------|----------|
| D1 | Trigger | Orchestration routes; turn receives uniformly via `OperatorInput` |
| D2A | Identity | Turn-owned; explicit configuration required, structural or prompt-based |
| D2B | History | Turn reads from state; writes via effects only |
| D2C | Memory | Three tiers (hot/warm/cold); tier assignment is per-agent config |
| D2D | Tools | Definitions are source of truth for metadata; execution via environment |
| D2E | Budget | Turn allocates; compaction reserve must be non-zero |
| D3A | Model | Turn-owned; single through three-tier routing supported |
| D3B | Durability | Orchestration-owned; entangled with crash recovery by design |
| D3C | Retry | Orchestration-owned; single retry authority, no double-retry |
| D4A | Isolation | Environment-owned; full spectrum supported; turn is isolation-unaware |
| D4B | Credentials | Environment-owned; boundary injection preferred; no secret leakage |
| D4C | Backfill | Turn-owned; tool output formatting is first-class engineering |
| D5 | Exit | Multi-source with priority: safety > budget > max turns > model done |
| C1 | Context | Task-only injection default; infra-enforced boundaries preferred |
| C2 | Results | Two-path preferred: full to storage, summary to parent |
| C3 | Lifecycle | Ephemeral default; long-lived opt-in |
| C4 | Communication | Sync default; signals for distributed; explicit ordering required |
| C5 | Observation | Via hooks; three forms (oracle/guardrail/observer); no indefinite blocking |
| L1 | Memory writes | Pre-compaction flush mandatory; capture before context destruction |
| L2 | Compaction | Three-way coordination; summarization default; reserve space non-zero |
| L3 | Recovery | Orchestration-entangled; local = none; durable = replay |
| L4 | Budget | Single authority; lifecycle coordinates; planners read-only |
| L5 | Observability | Cross-cutting; common event interface; correlation IDs required |

---

## Adapting This Document

This constitution is designed to be forked. The structure is universal; the positions are project-specific.

**What is universal** (keep when forking):
- The five core values and their priority ordering
- The three-layer organization (Turn, Composition, Lifecycle)
- The 23 decision points and their principles
- The authority hierarchy pattern
- The decision index format

**What is project-specific** (replace when forking):
- Each decision's **position** (the specific choice your project makes)
- The authority hierarchy entries (your project's spec/rule locations)
- References to specific protocols, traits, and crate names

**How to update**: When the problem space evolves, update the relevant position. If a new decision emerges that doesn't fit any of the 23, add it — and consider contributing it back to the [Agentic Decision Map](docs/architecture/agentic-decision-map-v3.md) that this constitution derives from.

**Version discipline**: Date your changes. When a position changes, note what it was and why it changed. Agents loading this document in future sessions need to know whether their cached understanding is still current.

---

*Derived from the [Agentic Decision Map v3](docs/architecture/agentic-decision-map-v3.md). Last updated: 2026-03-03.*
