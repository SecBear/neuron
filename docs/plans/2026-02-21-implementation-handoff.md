# Implementation Handoff Context

> **Purpose:** This document provides complete context for an agent picking up
> implementation planning. Read this + the design doc + CLAUDE.md before writing
> any code.

## What exists

Two files contain everything:

1. **`CLAUDE.md`** (root) — Philosophy, landscape comparison, decision
   framework, validated decisions, Rust conventions, block dependency graph.
2. **`docs/plans/2026-02-21-neuron-design.md`** — Complete design
   with every type, trait, error enum, block spec, composition example, build
   order, and dependencies finalized.

No code has been written. The meta-repo has only these docs and a git history
of the design process.

## What was validated (Wave 1 research)

8 parallel research agents validated the design against:

- **Anthropic Messages API** — All fields confirmed. Added: model, thinking
  content blocks, tool_choice, structured SystemPrompt, CacheControl,
  ThinkingDelta/SignatureDelta streaming, ContentFilter stop reason.
- **OpenAI Chat Completions + Responses API** — Single Provider trait works as
  lossy abstraction. Added: response_format, reasoning_effort,
  reasoning_tokens, id on ToolUseInputDelta for parallel demux, extra field.
- **Ollama API** — Provider trait maps cleanly. NDJSON streaming works with
  StreamEvent. Added: provider-specific error variants (ModelNotFound,
  ModelLoading, InsufficientResources), extra escape hatch.
- **MCP spec (2025-11-25)** — Our original design targeted deprecated SSE
  transport. Changed to wrap rmcp (official SDK, 3.8M downloads). Added:
  Streamable HTTP, lifecycle/initialization, sampling, resources, prompts,
  pagination, tool annotations.
- **Temporal SDK** — FUNDAMENTAL CHANGE: observation hooks cannot participate in
  replay. Replaced DurabilityHook with DurableContext (wraps side effects) +
  ObservabilityHook (logging/metrics). DurableContext has execute_llm_call,
  execute_tool, wait_for_signal, should_continue_as_new, continue_as_new.
- **Rig source code** — Adopted: native async (Rust 2024), WasmCompatSend/Sync,
  ToolDyn type erasure, IntoFuture builders, schemars, Continue/Skip/Terminate
  hooks. Rejected: Clone on models, OneOrMany, ToolServer+mpsc, variant-per-role
  messages.
- **Tower middleware** — Confirmed our ToolMiddleware with next callback is
  correct (identical to axum's from_fn). Do NOT adopt tower Service/Layer.
  Added: Next::run consumes self, from_fn convenience, per-tool middleware.
- **ADK-Rust** — Confirmed our architecture is better decomposed. Their fat
  adk-core and all-providers-in-one-crate are anti-patterns. Adopted: telemetry
  as cross-cutting concern, umbrella crate pattern.

## Build order (from design doc section 3.3)

1. `neuron-types` — foundation, everything depends on this
2. `neuron-tool` — tool registry and middleware
3. `neuron-context` — compaction strategies
4. `neuron-provider-anthropic` — first real provider
5. `neuron-loop` — composes the above
6. `neuron-mcp` — wraps rmcp
7. `neuron-provider-openai` — second provider
8. `neuron-provider-ollama` — local inference
9. `neuron-runtime` — DurableContext, sessions, sub-agents, guardrails
10. `neuron` — umbrella crate, last

## Key technical decisions already made

- **Rust 2024 edition**, resolver 3, minimum Rust 1.90
- **Native async in traits** — `-> impl Future<Output = T> + WasmCompatSend`
- **No #[async_trait]** — obsolete
- **Tool trait**: strongly-typed `Tool` with associated types (Args, Output,
  Error) + blanket-implemented `ToolDyn` for type-erased dynamic dispatch
- **ToolMiddleware**: next callback pattern (NOT tower). `Next::run` consumes
  self. Support `tool_middleware_fn` for closures. Per-tool middleware via
  `add_tool_middleware`.
- **schemars** for JSON Schema derivation on tool input types
- **thiserror** for all errors, 2 levels max, retryable vs terminal separation
- **neuron-mcp wraps rmcp** — do not reimplement the MCP protocol
- **DurableContext** wraps side effects (not observation hooks). Loop calls
  through it when present, calls provider/tools directly when absent.
- **Each block is its own independent crate** — not a Cargo workspace. Each
  gets its own repo eventually but for development they live in this meta-dir.
- **Flat file layout** per block: lib.rs, types.rs, traits.rs, {feature}.rs,
  error.rs

## What phases come next

### Phase 1: Implementation Plan (NOW)
Write a step-by-step implementation plan following the writing-plans skill
format. TDD, frequent commits, bite-sized tasks. Save to
`docs/plans/2026-02-21-implementation-plan.md`.

### Phase 2: Implementation
Execute the plan block by block in build order. Each block should compile and
have passing tests before moving to the next. Use the executing-plans or
subagent-driven-development skill.

### Phase 3: Integration Testing
Write composition examples from the design doc as actual integration tests.
Minimal agent (3 blocks), coding agent (6 blocks), durable agent (all blocks).
These validate that blocks compose correctly.

### Phase 4: Documentation + Publishing
- README per crate
- docs.rs documentation (already inline from implementation)
- Publish to crates.io in dependency order
- Create the umbrella `neuron` crate

## Constraints for the implementation planner

- **Each block is a separate Cargo project** (not workspace members). Use
  path dependencies during development: `neuron-types = { path = "../neuron-types" }`.
- **TDD**: Write failing test, make it pass, commit. Every public API has tests.
- **No placeholder implementations** — each block must be functional when done.
  The Anthropic provider must actually call the API. The MCP client must
  actually connect to MCP servers.
- **Start with neuron-types** — this is the foundation. Get every type right here
  because everything depends on it.
- **The design doc is the spec** — every type signature, every trait, every error
  variant is already defined. The implementation plan translates the design doc
  into ordered coding tasks, it does not redesign anything.
