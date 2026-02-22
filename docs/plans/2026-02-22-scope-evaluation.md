# Scope Evaluation: neuron Building Blocks vs SDK Layer

**Date:** 2026-02-22
**Purpose:** Deep architectural evaluation of what belongs in neuron (building
blocks) vs what belongs in an SDK layer built on top (the SDK layer or user projects).

## The Principle

neuron is **serde, not serde_json**. It defines the traits, provides the
foundational implementations, and gets out of the way. An SDK layer (user
frameworks) composes these blocks into opinionated workflows.

The test: **Can a user reasonably need this block without wanting the rest of
the stack?** If yes, it belongs in neuron. If it only makes sense as part of a
composed workflow, it belongs above.

## Current Stack Evaluation

### neuron-types — CORRECT SCOPE

Pure types, traits, errors. Zero logic. This is the lingua franca.

**Keeps:** Message, CompletionRequest, Provider trait, Tool trait,
ContextStrategy trait, ObservabilityHook trait, DurableContext trait,
PermissionPolicy, all error types.

**Concern: DurableContext trait is too Temporal-shaped.** The methods
(`execute_llm_call`, `execute_tool`, `wait_for_signal`, `continue_as_new`,
`should_continue_as_new`, `sleep`, `now`) map 1:1 to Temporal's workflow API.
This is fine as a starting point — Temporal's model is the most general — but
`ActivityOptions` with `RetryPolicy` bakes in retry-at-the-durable-layer.
That's correct: retry belongs in the durable engine, not in neuron's loop.

**Concern: RetryPolicy in neuron-types.** `RetryPolicy` (initial_interval,
backoff_coefficient, maximum_attempts, maximum_interval,
non_retryable_errors) lives in neuron-types because `ActivityOptions` uses it,
and `ActivityOptions` is a parameter to `DurableContext` methods. This is the
right place — it describes what the durable engine should do, not what neuron
does. neuron never executes retries itself.

**Verdict: No changes needed.**

### neuron-tool — CORRECT SCOPE

ToolRegistry, middleware pipeline, ToolError::ModelRetry. This is axum's
`from_fn` pattern for tools. Anyone building tool-using agents needs this
regardless of what framework they use.

**Verdict: No changes needed.**

### neuron-context — CORRECT SCOPE

Context compaction strategies, token counting. These are pure algorithms with
no opinions about how they're composed into a loop.

**Verdict: No changes needed.**

### neuron-loop — CORRECT SCOPE, with one caveat

The ~300-line while loop. Commodity. Composes Provider + Tool + Context. Every
framework converges on this same loop.

**Caveat:** The loop currently handles `StopReason::Compaction` (continues
automatically) and `ToolError::ModelRetry` (converts to error tool result).
Both of these are **loop-level behavior**, not framework-level. Any agent loop
needs these. Correct scope.

**Verdict: No changes needed.**

### neuron-mcp — CORRECT SCOPE

Wraps rmcp, bridges MCP tools to neuron's Tool trait. Anyone using MCP needs
this regardless of framework.

**Verdict: No changes needed.**

### neuron-provider-* — CORRECT SCOPE

One crate per provider, each implements the Provider trait. The serde_json of
LLM providers.

**Verdict: No changes needed.** More providers (Groq, DeepSeek) are just
more serde_json equivalents — they belong here.

### neuron-runtime — NEEDS EVALUATION

This is the crate where scope gets blurry. Let's evaluate each module:

**session.rs — BORDERLINE.** Session is a concrete data structure with two
storage implementations. It's useful to anyone building a multi-turn agent.
But:
- `Session` has fields like `token_usage`, `event_count`, `custom` metadata
  that assume a specific session model.
- `InMemorySessionStorage` and `FileSessionStorage` are reference impls.
- The `SessionStorage` trait is generic enough.

**Verdict: Keep.** Sessions are infrastructure, not opinions. Every multi-turn
agent needs persistence. The trait is minimal, the impls are reference-quality.
This is like providing `InMemoryTokenBucket` alongside a `RateLimiter` trait.

**sub_agent.rs — BELONGS IN SDK LAYER.** SubAgentManager is opinionated:
- It assumes a registry-of-configs model for sub-agents
- It filters tools by name list (one specific strategy)
- It assumes sub-agents use the same Provider/ContextStrategy as parent
- `spawn()` and `spawn_parallel()` are composition patterns, not building blocks

A building block would be: "here's how to create a nested AgentLoop with
different tools." SubAgentManager adds opinions about configuration, naming,
depth limits, and tool filtering.

**Counter-argument:** The sub-agent pattern is so common that not having it
means every user reimplements the same 50 lines. But those 50 lines are
straightforward composition of existing blocks (AgentLoop + ToolRegistry).
That's exactly what an SDK should provide.

**Verdict: Move to SDK layer.** neuron should make it easy to compose a
sub-agent from blocks, but the SubAgentManager registry pattern is opinionated.

**guardrail.rs — CORRECT SCOPE.** InputGuardrail and OutputGuardrail are pure
traits with a simple result type (Pass/Tripwire/Warn). The type erasure layer
is necessary for RPITIT. The runner functions are 10-line utilities. No
opinions about what to guard against or how to respond.

**Verdict: Keep.** But the traits could move to neuron-types (they're just
trait definitions), and the runner functions stay in neuron-runtime.

**durable.rs — CORRECT SCOPE.** LocalDurableContext is a passthrough for local
development. The DurableContext trait is in neuron-types (correct). Temporal
and Restate implementations would be separate crates (correct — like
serde_json is separate from serde).

**Verdict: Keep LocalDurableContext as reference impl.**

**sandbox.rs — CORRECT SCOPE.** Sandbox trait + NoOpSandbox. Pure
extensibility point. No opinions.

**Verdict: Keep.**

### neuron (umbrella) — CORRECT SCOPE

Re-exports with feature flags. Build last. This is correct.

---

## ROADMAP Evaluation

### v0.2 Items

| Item | Verdict | Reasoning |
|------|---------|-----------|
| More providers (Gemini, Groq, DeepSeek) | **neuron** | Provider-per-crate pattern, same as existing |
| EmbeddingProvider trait | **neuron-types** | Pure trait, same pattern as Provider |
| OpenTelemetry hook (TracingHook) | **neuron, feature-gated** | Reference impl, like serde_json. Behind `tracing` feature flag. |
| More examples | **neuron** | Documentation |
| CHANGELOG.md | **neuron** | Infrastructure |

**Server-side compaction and ModelRetry are already shipped (v0.1).** The
ROADMAP still lists them under v0.2 — needs fixing.

### Later Items

| Item | Verdict | Reasoning |
|------|---------|-----------|
| SDK layer | **Separate project** | Correct — opinionated framework on blocks |
| Additional providers | **neuron** | More provider crates |
| VectorStore trait | **neuron-types** | Pure trait. Implementations as separate crates. |
| Resilience layer (retry, circuit breaker, rate limit) | **NOT neuron** | This is tower-level infrastructure. Use tower::retry, tower::limit. Or the durable engine handles it. neuron should not reimplement tower. |
| Config-driven provider routing | **SDK layer** | Opinionated composition pattern |
| Cost/usage tracking | **neuron-types field + SDK layer** | Token pricing metadata belongs in types (it's data). Cost estimation logic belongs in SDK (it's policy). |
| Dedicated docs site | **neuron** | Documentation |

### Items from Competitive Audit

| Item | Verdict | Reasoning |
|------|---------|-----------|
| Message::user(), etc. | **Done** ✅ | |
| ToolContext::default() | **Done** ✅ | |
| from_env() on providers | **Done** ✅ | |
| TracingHook | **neuron, feature-gated** | Reference impl proves the trait works |
| crates.io categories | **neuron** | Metadata |
| #[neuron_tool] in Quick Start | **neuron** | Documentation |
| More examples (10+) | **neuron** | Documentation |
| OpenAI-compat thin providers | **neuron** | Just base_url overrides, still provider crates |
| Standalone docs site | **neuron** | Documentation |
| Agent[Deps, Output] generics | **SDK layer** | Opinionated composition |
| Handoff protocol | **SDK layer** | Multi-agent orchestration policy |
| StopAtTools | **SDK layer** | Agent-level policy, not loop-level |
| Agent.override() for testing | **SDK layer** | Framework testing DX |
| Parallel guardrails | **SDK layer** | Orchestration pattern |
| ToolPrepareFunc (conditional tools) | **Borderline** | Could be middleware in neuron-tool, or SDK composition |
| ResolvedContext | **neuron-context or SDK** | Context strategy, could reduce hallucinations |
| enhanced_description() | **neuron-types** | Just a field on ToolDefinition |
| Search-friendly branding | **Marketing** | Not code |

---

## Scope Refinements

### What moves OUT of neuron

1. **SubAgentManager** — the registry/spawn pattern is opinionated. neuron
   already provides everything needed to compose sub-agents (AgentLoop +
   ToolRegistry + LoopConfig). An SDK adds the management layer.

2. **Resilience layer (from ROADMAP)** — retry, circuit breaker, rate limiting
   are tower concerns or durable engine concerns. neuron should not reimplement
   them. `ProviderError::is_retryable()` already tells callers what's
   retryable. The decision of whether/how to retry belongs above.

3. **Config-driven provider routing (from ROADMAP)** — opinionated composition.
   SDK territory.

### What stays IN neuron

1. **Session management** — infrastructure, not opinions. Every multi-turn agent
   needs it.

2. **Guardrails (traits + runner)** — pure extensibility. No opinions about what
   to guard against.

3. **DurableContext** — the trait is in types, the passthrough impl is in
   runtime. Correct layering.

4. **Sandbox** — pure extensibility point.

5. **TracingHook** — reference implementation behind feature flag. Proves the
   ObservabilityHook trait works. Like serde_json to serde.

6. **EmbeddingProvider trait** — pure trait, same pattern as Provider.

7. **VectorStore trait** — pure trait. Implementations in separate crates.

8. **More providers** — same pattern as existing provider crates.

### What's borderline (keep for now, revisit)

1. **ToolPrepareFunc** — conditional tool registration could be a middleware
   variant. If it's 10 lines on ToolRegistry, keep it. If it needs its own
   config model, move to SDK.

2. **ResolvedContext** — reducing tool hallucinations by filtering the tool
   list per-turn based on context. Could be a ContextStrategy concern or a
   ToolRegistry method. Keep evaluating.

---

## The Scope Line, Stated Simply

**neuron provides:**
- Trait definitions (in neuron-types)
- Reference implementations (one or two per trait)
- The commodity loop (neuron-loop)
- Infrastructure that any agent needs regardless of framework (sessions,
  guardrails, durability, tools, context, MCP)

**neuron does NOT provide:**
- Agent lifecycle management (registry, naming, handoff)
- Opinionated composition patterns (config-driven routing, parallel guardrails)
- Retry/resilience (tower or durable engine)
- Framework-level DX (Agent[Deps, Output], override for testing)

**The test:** If removing it would force every user of the blocks to
reimplement the same 200+ lines of non-trivial code, it belongs in neuron.
If removing it would force users to write 20-50 lines of straightforward
composition, it belongs in the SDK.
