# Composable Operator Architecture — Design Document

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:writing-plans to create the implementation plan from this design.

**Date:** 2026-02-25
**Status:** Approved design, pending implementation plan
**Supersedes:** NEURON-REDESIGN-PLAN.md Phase 6-8 (Phases 1-5 remain valid and are complete)

---

## 1. What Changed and Why

Deep research across Temporal, Airflow, Dagster, AutoGen, CrewAI, LangGraph, the Actor Model, MCP, and our own 23-decision architecture document revealed three design flaws in the original plan:

1. **The word "Turn" is overloaded.** In common usage, a "turn" is one model call. In layer0, `Turn` meant a complete agent execution (many model calls). `TurnMetadata.turns_used` counted model calls inside one `Turn::execute()`. This naming collision made the system hard to reason about.

2. **neuron-turn was monolithic.** It contained both the shared toolkit (Provider trait, types, ContextStrategy) and a specific execution pattern (the ReAct loop). Other execution patterns (single-shot, plan, classify) would need the toolkit but not the ReAct loop.

3. **The composition model needed clarification.** The original plan didn't precisely define who composes what, leading to confusion about whether operators should compose other operators (they shouldn't — that's the Orchestrator's job).

---

## 2. The Operator Rename

### From Turn to Operator

**layer0 rename:**

| Old | New |
|-----|-----|
| `trait Turn` | `trait Operator` |
| `TurnInput` | `OperatorInput` |
| `TurnOutput` | `OperatorOutput` |
| `TurnError` | `OperatorError` |
| `TurnMetadata` | `OperatorMetadata` |
| `TurnConfig` | `OperatorConfig` |
| `ExitReason` | `ExitReason` (unchanged) |

### Why "Operator"

**Airflow precedent:** The most established orchestration framework uses "Operator" for exactly this concept — `BashOperator`, `PythonOperator`, `HttpOperator`. Atomic, pluggable units of work composed by a DAG (orchestrator).

**Dagster precedent:** Renamed their atomic unit from "solid" to "op" (operation) for the same reason — clearer semantics.

**Mathematical precedent:** An operator is a function that transforms input to output. Our Operator literally does this: `OperatorInput → Result<OperatorOutput, OperatorError>`.

**Kubernetes collision:** Tolerable. Different domain, different audience. Airflow has coexisted with K8s for a decade with both using "Operator."

### The hierarchy, clarified

```
Orchestrator dispatches → Operators (atomic units of work)
                            └── internally may use multiple "turns" (model calls)
                                └── each turn calls Provider::complete()
```

- **Turn** = one model call (atomic, the natural meaning)
- **Operator** = one complete unit of agentic work (may contain many turns)
- **Orchestrator** = composes operators into workflows

---

## 3. Architecture Overview

### The Stack

```
╔══════════════════════════════════════════════════════════════╗
║  APPLICATION LAYER  (what you build)                         ║
║                                                              ║
║  Orchestration engine, CLI, API server, scheduled workflows  ║
╚══════════════════════════════════════════════════════════════╝
         │
         │ assembles from
         ▼
┌──────────────────────────────────────────────────────────────┐
│  ORCHESTRATORS  (how operators compose)                       │
│                                                              │
│  neuron-orch-local      in-process, tokio, no durability     │
│  neuron-orch-temporal   durable workflows, crash recovery    │
│  neuron-orch-team       autonomous agents, mailbox, tasks    │
└──────────────────────────────────────────────────────────────┘
         │
         │ dispatches to
         ▼
┌──────────────────────────────────────────────────────────────┐
│  OPERATORS  (atomic units of work)                            │
│                                                              │
│  neuron-op-react         model + tools in a loop (ReAct)     │
│  neuron-op-single-shot   one model call, return              │
│  neuron-op-plan          generate a plan (orch executes)     │
│  neuron-op-classify      route input to a category           │
│  neuron-op-human         wait for human input                │
│  neuron-op-claude-code   full Claude Code session            │
│  neuron-op-codex         full OpenAI Codex session           │
└──────────────────────────────────────────────────────────────┘
         │
         │ built on
         ▼
┌──────────────────────────────────────────────────────────────┐
│  TOOLKIT  (neuron-turn)                                       │
│                                                              │
│  Provider trait       one model call                         │
│  ContextStrategy      manage context window between calls    │
│  Types                ProviderRequest/Response, ContentPart  │
│                       TokenUsage, StopReason                 │
└──────────────────────────────────────────────────────────────┘
         │
         │ implemented by
         ▼
┌──────────────────────────────────────────────────────────────┐
│  PROVIDERS              STATE              ENVIRONMENTS      │
│                                                              │
│  neuron-provider-       neuron-state-      neuron-env-       │
│    anthropic              memory             local           │
│    openai                 fs                 docker          │
│    ollama                 postgres            k8s            │
│                           redis                              │
│  TOOLS                  HOOKS              MCP               │
│                                                              │
│  neuron-tool            neuron-hooks       neuron-mcp        │
│  (registry)             (logging, safety,  (client+server)   │
│  + MCP-discovered         budget, audit)                     │
└──────────────────────────────────────────────────────────────┘
         │
         │ all built on
         ▼
┌──────────────────────────────────────────────────────────────┐
│  layer0  (protocol traits — the stability contract)           │
│                                                              │
│  trait Operator · trait Orchestrator · trait StateStore       │
│  trait Environment · trait Hook · lifecycle events            │
│  OperatorInput/Output · Effects · Content · IDs              │
└──────────────────────────────────────────────────────────────┘
```

### Crate Naming Convention

| Pattern | Purpose | Examples |
|---------|---------|---------|
| `layer0` | Protocol traits | — |
| `neuron-turn` | Shared toolkit (Provider, types, context) | — |
| `neuron-op-*` | Operator implementations | `neuron-op-react`, `neuron-op-single-shot` |
| `neuron-provider-*` | Provider implementations | `neuron-provider-anthropic`, `-openai`, `-ollama` |
| `neuron-state-*` | StateStore implementations | `neuron-state-memory`, `-fs`, `-postgres` |
| `neuron-env-*` | Environment implementations | `neuron-env-local`, `-docker`, `-k8s` |
| `neuron-orch-*` | Orchestrator implementations | `neuron-orch-local`, `-temporal`, `-team` |
| `neuron-tool` | Tool registry | — |
| `neuron-hooks` | Hook registry | — |
| `neuron-mcp` | MCP client + server | — |

---

## 4. The Operator/Orchestrator Boundary

### The iron rule

**Operators are atomic. They never invoke other operators. They communicate through orchestrator-managed infrastructure.**

Operators CAN:
- Know about peers (via injected team context)
- Address specific peers (via `Effect::Signal`)
- Self-claim work (via shared `StateStore`)
- Decide whom to talk to and when

Operators CANNOT:
- Call another operator's `execute()` directly
- Bypass the orchestrator's delivery infrastructure
- Manage their own lifecycle (start/stop)

### Why this boundary exists

From the architecture source doc: *"The recovery mechanism IS the composition mechanism with cached shortcuts. Any architecture that tries to make durability a pluggable concern independent of orchestration will leak."*

If operators composed other operators:
- Crash recovery would need to instrument nested operators
- The orchestrator would lose visibility into the composition topology
- Budget tracking would be split across two systems
- Durability guarantees would be impossible to reason about

### The dispatch boundary is opaque

`Orchestrator::dispatch(agent: &AgentId, input: OperatorInput) -> Result<OperatorOutput, OrchError>`

Behind the `AgentId` could be:

| Implementation | What it is | Cost per dispatch |
|---|---|---|
| `neuron-op-single-shot` + ollama | One local model call | $0.00 |
| `neuron-op-react` + anthropic | ReAct loop, ~5 turns | $0.05 |
| `neuron-op-claude-code` | Full Claude Code session, ~50 turns | $0.50 |
| `neuron-op-human` | A human reading and typing | $50/hr |
| Another orchestrator | A sub-workflow with its own agents | varies |

The orchestrator doesn't know or care. Same interface. Same composition.

### Orchestrators nest transparently

An orchestrator's dispatch target can BE another orchestrator. This is exactly Temporal's child workflow pattern.

```
Orchestrator A
├── dispatch("agent-1")  → Operator (single-shot)
├── dispatch("agent-2")  → Operator (react)
└── dispatch("team-3")   → Orchestrator B (a whole agent team)
                            ├── dispatch("researcher") → Operator
                            ├── dispatch("reviewer")   → Operator
                            └── mailbox + shared tasks
```

No protocol change needed. The opaque boundary composes naturally.

---

## 5. Composition Patterns

All six composition primitives from the decision map are Orchestrator responsibilities:

```
CHAIN:      A ──▶ B ──▶ C              (sequential dispatch)
FAN-OUT:    A ──┬▶ B C D               (dispatch_many)
FAN-IN:     B C D ──▶ A                (collect results)
DELEGATE:   A ──▶ [B runs] ──▶ A       (dispatch, feed result back)
HANDOFF:    A ──▶ B                     (dispatch, A exits)
OBSERVE:    O watches A                 (Hook trait)
```

### Team patterns decompose into existing primitives

**Debate** = DELEGATE in alternating loop:
```
Orchestrator:
  loop:
    pro  = dispatch(agent_pro, topic + last_contra)
    contra = dispatch(agent_contra, topic + pro)
    judgment = dispatch(judge, pro + contra)
    if judgment.consensus: break
```

**Code review** = DELEGATE in feedback loop:
```
Orchestrator:
  draft = dispatch(writer, task)
  loop:
    review = dispatch(reviewer, draft)
    if review.approved: break
    draft = dispatch(writer, review.critique)
```

**Group chat** (AutoGen-style) = DELEGATE in loop with dynamic selection:
```
Orchestrator:
  history = []
  loop:
    speaker = select_next(history, agents)
    output = dispatch(speaker, history)
    history.push(output)
    if done(history): break
```

**Research team** = DELEGATE + FAN-OUT/FAN-IN:
```
Orchestrator:
  loop:
    plan = dispatch(planner, goal + previous_results)
    if plan.complete: break
    results = dispatch_many(researchers, plan.assignments)
    previous_results = results
```

No new primitives needed. No "Team" trait. No bilateral communication primitive. The Orchestrator implements the interaction protocol; operators participate without knowing the topology.

### Agent teams (autonomous topology)

Inspired by Claude Code agent teams. The orchestrator provides infrastructure (mailbox, shared task list) but agents decide their own interactions:

- Operators discover peers via injected team context
- Operators message peers via `Effect::Signal { target: AgentId }`
- Orchestrator delivers signals as new invocations with `TriggerType::Signal`
- Shared task list is a `StateStore` that operators read/write

This is a different orchestrator implementation (`neuron-orch-team`), not a different protocol. The same operators work in DAG topologies or team topologies without modification.

---

## 6. neuron-turn as Toolkit

`neuron-turn` contains the shared building blocks for all operator implementations:

### What stays in neuron-turn

| Component | Purpose |
|-----------|---------|
| `trait Provider` | One model call: `complete(ProviderRequest) -> ProviderResponse` |
| `ProviderRequest` | Model, messages, tools, system prompt, max_tokens, temperature, extra |
| `ProviderResponse` | Content, stop_reason, usage, model, cost, truncated |
| `ProviderMessage` | Role + Vec<ContentPart> |
| `ContentPart` | Text, ToolUse, ToolResult, Image |
| `TokenUsage` | input_tokens, output_tokens, cache_read, cache_creation |
| `StopReason` | EndTurn, ToolUse, MaxTokens, ContentFilter |
| `ProviderError` | RateLimited, AuthFailed, RequestFailed, InvalidResponse, Other |
| `trait ContextStrategy` | token_estimate, should_compact, compact |
| `ImageSource` | Base64, Url |

### What moves out

The ReAct loop (`NeuronTurn<P>`) moves to `neuron-op-react` as `ReactOperator<P>`.

### Provider trait stays internal (RPITIT, not object-safe)

```rust
pub trait Provider: Send + Sync + 'static {
    fn complete(
        &self,
        request: ProviderRequest,
    ) -> impl Future<Output = Result<ProviderResponse, ProviderError>> + Send;
}
```

Never exposed at protocol boundaries. Only used by operator implementations internally.

---

## 7. Provider-Specific Richness

Each provider crate captures ALL features of that provider's API. Provider-specific features flow through `extra: serde_json::Value` on `ProviderRequest`.

### Anthropic (neuron-provider-anthropic)
- Prompt caching (cache_control blocks)
- Extended thinking (thinking content blocks)
- Streaming (SSE with content_block_delta)
- Tool use with auto/any/specific modes
- Token-efficient tool results
- System prompt as separate field
- Image support (base64 + URL)

### OpenAI (neuron-provider-openai)
- Chat Completions API + Responses API
- Reasoning models (o-series with reasoning_effort)
- Service tiers (auto/default/flex)
- Predicted outputs (for editing)
- Parallel tool calls (parallel_tool_calls flag)
- Strict tool schemas (strict: true)
- Developer role (replaces system in o-series)
- Structured outputs (response_format: json_schema)
- String tool arguments (must parse JSON from string)
- Image support (base64 + URL with detail level)

### Ollama (neuron-provider-ollama)
- Native /api/chat endpoint (not OpenAI-compatible)
- Thinking models (think field, separate thinking content)
- Nanosecond timing metadata (load_duration, prompt_eval_duration, etc.)
- Hardware options (num_gpu, num_thread)
- Keep-alive control (keep_alive parameter)
- Object tool arguments (not string — different from OpenAI)
- Synthesized tool_use IDs (Ollama doesn't generate them)
- System role support (not developer role)
- No auth required
- Model pull/management API

---

## 8. MCP Architecture

neuron-mcp provides BOTH client and server, independently composable.

### Client: Discover and use remote tools

```
neuron-mcp client
├── Connect to MCP servers (stdio, SSE, streamable HTTP)
├── Discover available tools
├── Register discovered tools into ToolRegistry
└── Execute tool calls by proxying to MCP server
```

An operator using MCP tools doesn't know they're remote. The tool call goes through the ToolRegistry, which proxies to the MCP server.

### Server: Expose neuron capabilities as MCP tools

```
neuron-mcp server
├── Expose ToolRegistry as MCP tools
├── Expose StateStore as MCP resources
├── Expose Orchestrator::dispatch as an MCP tool
└── Handle tool calls from external MCP clients
```

This allows external systems (Claude Code, other MCP clients) to use neuron's capabilities.

### Bidirectional: Claude Code integration

```
Direction A: Claude Code connects to neuron's MCP server
  → Claude Code gains: durable state, dispatch, budget control

Direction B: neuron operators connect to Claude Code's MCP
  → Neuron gains: Claude Code's tool suite (Read, Write, Bash, etc.)
```

---

## 9. External Systems as Operators

Full agentic systems (Claude Code, Codex) fit as Operator implementations, not Providers. A Provider is one model call. These systems are complete reasoning engines with their own loops, tools, and context management.

### neuron-op-claude-code

Wraps the `@anthropic-ai/claude-code` SDK or CLI invocation. Takes `OperatorInput`, invokes a full Claude Code session, collects results as `OperatorOutput`.

### neuron-op-codex

Same pattern for OpenAI Codex CLI.

### neuron-op-human

Waits for human input. Returns the human's response as `OperatorOutput`. The operator is a bridge to a UI/notification/email system.

### The composability promise

Any operator can be swapped without changing the orchestrator:

```yaml
# Version 1: cheap, fast, local
coder:
  operator: single-shot
  provider: ollama
  model: codellama

# Version 2: powerful, expensive, hosted
coder:
  operator: claude-code
  model: claude-sonnet-4-6

# Version 3: human in the loop
coder:
  operator: human
  notify: slack
  channel: "#dev-team"
```

Same orchestrator config. Same flow. Different operator behind the AgentId.

---

## 10. 23-Decision Coverage Map

Every decision from `agentic-decision-map-v3.md` mapped to the architecture:

### Turn-Level Decisions (D1-D5)

| ID | Decision | Owner | Swappable | Distributed |
|---|---|---|---|---|
| D1 | Trigger type | Environment | Yes | Yes |
| D2A | Identity/behavior | Hook + ContextStrategy | Yes | Yes |
| D2B | Conversation history | StateStore | Yes | Yes |
| D2C | Memory (cross-session) | StateStore + Hook | Yes | Yes |
| D2D | Tool surface | neuron-tool + neuron-mcp | Yes | Yes |
| D2E | Context budget | ContextStrategy | Yes | Local |
| D3A | Model selection | Provider | Yes | Yes |
| D3B | Inference durability | Orchestrator | Partial | Yes |
| D3C | Retry strategy | Provider + Orchestrator | Partial | Partial |
| D4A | Tool isolation | Environment | Yes | Yes |
| D4B | Credential handling | Environment + Hook | Yes | Yes |
| D4C | Result backfill | Hook + ContextStrategy | Yes | Local |
| D5 | Exit condition | Operator + Hook | Yes | Partial |

### Composition Decisions (C1-C5)

| ID | Decision | Owner | Swappable | Distributed |
|---|---|---|---|---|
| C1 | Child context inheritance | Orchestrator | Yes | Yes |
| C2 | Result flow back | Orchestrator + StateStore | Yes | Yes |
| C3 | Child lifecycle | Orchestrator | Yes | Yes |
| C4 | Agent communication | Orchestrator + StateStore | Yes | Yes |
| C5 | Observation/intervention | Hook | Yes | Yes |

### Lifecycle Decisions (L1-L5)

| ID | Decision | Owner | Swappable | Distributed |
|---|---|---|---|---|
| L1 | Memory writes | Hook + StateStore | Yes | Yes |
| L2 | Compaction | ContextStrategy + Hook | Yes | Partial |
| L3 | Crash recovery | Orchestrator | Partial | Yes |
| L4 | Budget control | Hook + Provider | Yes | Yes |
| L5 | Observability | Hook + StateStore | Yes | Yes |

### Known gaps (3/23)

All three relate to durability — an Orchestrator implementation property:

| Gap | Nature | Resolution |
|-----|--------|------------|
| D3B | neuron-orch-local has no durability | By design — use neuron-orch-temporal for durability |
| D3C | No standard RetryPolicy trait | Add to neuron-turn toolkit as an optional trait |
| L3 | Crash recovery only in Temporal orchestrator | By design — durability is an orchestrator choice |

These are intentional tradeoffs, not architectural flaws. The protocol supports durability; the implementation chooses whether to provide it.

---

## 11. Distribution Model

Every trait boundary is transport-agnostic. The trait doesn't know if the implementation is a function call or a network hop.

### Fully in-process (dev laptop)

```
neuron-orch-local       → tokio::spawn, function calls
neuron-op-react         → in-process loop
neuron-provider-ollama  → localhost:11434
neuron-state-memory     → HashMap
neuron-env-local        → same process
neuron-hooks            → println!

Total infrastructure: none
```

### Globally distributed (production)

```
neuron-orch-temporal         → Temporal Cloud (us-east)
├── dispatch("researcher")
│   └── neuron-op-react      → neuron-env-k8s (eu-west pod)
│       ├── provider          → neuron-provider-anthropic (API)
│       ├── tools             → neuron-mcp client → MCP server (ap-south)
│       └── state             → neuron-state-postgres (us-east RDS)
│
├── dispatch("reviewer")
│   └── neuron-op-claude-code → neuron-env-docker (us-west)
│       └── state             → same postgres (us-east RDS)
│
└── hooks
    ├── audit                 → Datadog (global)
    ├── budget                → postgres
    └── safety                → guardrail API (us-east)
```

**Same code. Same traits. Same config structure.** Only the crate implementations change.

Each component independently local or remote:

| Component | Local option | Remote option |
|---|---|---|
| Orchestrator | neuron-orch-local | neuron-orch-temporal |
| Operator | in-process | neuron-env-k8s (remote pod) |
| Provider | neuron-provider-ollama | neuron-provider-anthropic |
| State | neuron-state-memory | neuron-state-postgres |
| Tools | neuron-tool (in-process) | neuron-mcp (remote server) |
| Hooks | println! | Datadog/Prometheus |
| Environment | neuron-env-local | neuron-env-docker/-k8s |

---

## 12. Example Workflows

### Simple: Morning news digest

```yaml
name: morning-news
schedule: "0 7 * * *"
orchestrator: local

agents:
  tech-searcher:
    operator: single-shot
    provider: openai
    model: gpt-4o-mini
    tools: [web-search]
    prompt: "Search for today's top 5 tech news stories."

  finance-searcher:
    operator: single-shot
    provider: ollama
    model: llama3
    tools: [web-search]
    prompt: "Search for today's top 5 finance stories."

  security-searcher:
    operator: single-shot
    provider: ollama
    model: llama3
    tools: [web-search]
    prompt: "Search for today's top 5 cybersecurity stories."

  summarizer:
    operator: single-shot
    provider: anthropic
    model: claude-sonnet-4-6
    prompt: "Synthesize into a concise morning briefing."

flow:
  - fan-out: [tech-searcher, finance-searcher, security-searcher]
  - fan-in: summarizer
  - effect: notify
    channel: slack
    target: "#morning-news"

state: memory
environment: local
hooks:
  - budget-cap: $0.05
```

### Complex: Beat Minecraft

```yaml
name: beat-minecraft
trigger: api
orchestrator: temporal

agents:
  strategist:
    operator: plan
    provider: anthropic
    model: claude-opus-4-6
    prompt: "Break goal into next 3-5 sub-goals given game state."

  executor:
    operator: react
    provider: anthropic
    model: claude-sonnet-4-6
    tools: [minecraft-move, minecraft-mine, minecraft-craft,
            minecraft-fight, minecraft-place, minecraft-eat]
    max_turns: 50

  observer:
    operator: single-shot
    provider: openai
    model: gpt-4o-mini
    prompt: "Did executor achieve sub-goal? Should we replan?"

flow:
  - loop:
      max_iterations: 1000
      steps:
        - dispatch: strategist
        - for_each: strategist.output.sub_goals
          - dispatch: executor
          - dispatch: observer
          - branch:
              on: observer.output.verdict
              replan: break
        - branch:
            on: state.game.ender_dragon_dead
            true: complete

state: postgres
environment: docker
hooks:
  - budget-cap: $50.00
  - heartbeat: 30s
  - audit-log: true
```

### Multi-agent: Code review team

```yaml
name: feature-implementation
trigger: api
orchestrator: team

agents:
  planner:
    operator: single-shot
    provider: anthropic
    model: claude-opus-4-6
    prompt: "Break this feature into coding sub-tasks."

  coder:
    operator: claude-code
    model: claude-sonnet-4-6
    permissions: read-write
    working_dir: /project

  reviewer:
    operator: claude-code
    model: claude-opus-4-6
    permissions: read-only
    working_dir: /project

flow:
  - dispatch: planner
  - for_each: planner.output.tasks
    - dispatch: coder
    - dispatch: reviewer
    - branch:
        on: reviewer.output.approved
        false: dispatch coder with reviewer.feedback
```

---

## 13. What to Implement Now

### Immediate (this session / next sessions)

1. **Rename Turn → Operator in layer0** — breaking change, free now, expensive after publish
2. **Split neuron-turn** — extract ReAct loop to neuron-op-react, keep toolkit in neuron-turn
3. **Build neuron-provider-openai** — captures OpenAI-specific features
4. **Build neuron-provider-ollama** — captures Ollama-specific features
5. **Build neuron-mcp** — MCP client + server, independently composable
6. **Build neuron-op-single-shot** — simplest operator, one model call
7. **Audit everything** — maximum test coverage for all crates
8. **Real API integration tests** — against Anthropic, OpenAI, Ollama

### After validation

9. **Proof-of-concept** — research assistant + code review agent, swap providers/state/environment atomically
10. **Findings write-up**
11. **Docs, polish, publish**

### Future (not now)

- neuron-orch-temporal, neuron-orch-team
- neuron-env-docker, neuron-env-k8s
- neuron-state-postgres, neuron-state-redis
- neuron-op-plan, neuron-op-classify, neuron-op-human
- neuron-op-claude-code, neuron-op-codex
- YAML workflow engine
- Umbrella crate with feature flags

---

## 14. Dependency Graph (Updated)

```
layer0                              (serde, async-trait, thiserror, rust_decimal)
    ↑
    ├── neuron-turn (toolkit)       (layer0 re-exports types; adds Provider, ContextStrategy)
    │   ↑
    │   ├── neuron-provider-anthropic  (neuron-turn, reqwest, serde_json)
    │   ├── neuron-provider-openai     (neuron-turn, reqwest, serde_json)
    │   ├── neuron-provider-ollama     (neuron-turn, reqwest, serde_json)
    │   ↑
    │   ├── neuron-op-react            (neuron-turn, neuron-tool, neuron-hooks, layer0)
    │   ├── neuron-op-single-shot      (neuron-turn, layer0)
    │   └── (future neuron-op-*)
    │
    ├── neuron-context              (neuron-turn)
    ├── neuron-tool                 (layer0, schemars)
    │   └── neuron-mcp             (layer0, neuron-tool, rmcp)
    ├── neuron-state-memory         (layer0, tokio)
    ├── neuron-state-fs             (layer0, tokio)
    ├── neuron-env-local            (layer0)
    ├── neuron-hooks                (layer0)
    └── neuron-orch-local           (layer0, tokio)
```

---

## 15. Design Principles (Unchanged)

1. **Protocol-first.** layer0 traits are the stability contract. Everything else is pluggable.
2. **Object-safe at boundaries.** `Arc<dyn Operator>`, `Arc<dyn Orchestrator>`, etc. Internal traits (Provider) use RPITIT.
3. **Minimal dependencies.** layer0: serde, async-trait, thiserror, rust_decimal. That's it.
4. **Transport-agnostic.** Every trait boundary works as a function call or a network hop.
5. **Operators are atomic.** They declare effects; orchestrators execute them.
6. **Composition through orchestration.** Operators don't compose operators. Orchestrators do.
7. **Every type serializes.** All layer0 types are `Serialize + Deserialize`.
