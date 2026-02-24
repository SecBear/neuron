# Comparison with Other Frameworks

neuron takes a different approach from most agent frameworks. This page compares
its architecture with other popular options in the Rust and Python ecosystems.

> **Honest note:** neuron is at an early stage. This comparison focuses on
> architectural differences, not feature completeness. Where other frameworks
> have more mature implementations, we say so.

## Summary matrix

| | neuron (Rust) | Rig (Rust) | ADK-Rust (Google) | OpenAI Agents SDK (Python) | Pydantic AI (Python) |
|---|---|---|---|---|---|
| **Architecture** | Independent crates (building blocks) | Monolithic library | Multi-crate with DAG engine | Single package | Single package |
| **Provider abstraction** | Trait in types crate, impl per crate | Trait + built-in impls | Google-focused, extensible | OpenAI-only | Multi-provider |
| **Tool system** | Typed trait + type erasure + middleware | Typed trait, no middleware | Typed with annotations | Function decorators | Function decorators with typed args |
| **Middleware** | axum-style `from_fn` pipeline | None | None | Hooks | None |
| **Usage limits** | `UsageLimits` (tokens, requests, tool calls) | None | None | None | `UsageLimits` (tokens, requests) |
| **Tool timeouts** | `TimeoutMiddleware` (per-tool configurable) | None | None | None | None |
| **Context management** | Client-side + server-side compaction | Manual | Built-in | Built-in | Manual |
| **Durable execution** | `DurableContext` trait (Temporal/Restate) | None | None | None | None |
| **Async model** | RPITIT (native, no alloc) | `#[async_trait]` (boxed) | `#[async_trait]` (boxed) | Python async | Python async |
| **OpenTelemetry** | `neuron-otel` with GenAI semantic conventions | None | None | Built-in tracing | None |
| **MCP support** | Via `neuron-mcp` (wraps rmcp) | Community | Limited | Built-in | Limited |
| **Graph/DAG** | Not included (SDK layer) | Not included | LangGraph port | Not included | Not included |
| **Maturity** | Early | Established | Early | Established | Established |

## Detailed comparisons

### Rig (Rust)

Rig is the most established Rust agent framework. It provides a solid
multi-provider abstraction and a typed tool system.

**Where Rig excels:**
- Mature ecosystem with multiple provider implementations
- Good documentation and examples
- Proven in production use cases

**Where neuron differs:**
- **Crate independence.** Rig is a monolithic library -- you depend on `rig-core`
  and get everything. neuron lets you pull in just the tool system, or just a
  provider, without the rest.
- **Message model.** Rig uses a variant-per-role enum (`UserMessage`,
  `AssistantMessage`), which requires roughly 300 lines of conversion code per
  provider. neuron uses a flat `Message { role, content }` struct that maps
  directly to every API.
- **Tool middleware.** Rig has no middleware pipeline. Adding logging, rate
  limiting, or permission checks requires wrapping each tool individually.
  neuron's middleware pipeline applies cross-cutting concerns to all tools.
- **Async model.** Rig uses `#[async_trait]`, which heap-allocates on every
  call. neuron uses RPITIT (native Rust 2024 async traits) with zero overhead
  for non-erased dispatch.

### ADK-Rust (Google's Agent Development Kit)

Google's ADK-Rust is a multi-crate Rust framework that includes a port of
LangGraph's DAG execution engine.

**Where ADK-Rust excels:**
- Comprehensive multi-crate architecture
- Built-in DAG/graph orchestration for complex workflows
- Strong Google Cloud integration

**Where neuron differs:**
- **No graph layer.** ADK-Rust's LangGraph port is its most complex component
  and, based on community feedback, its least-used. neuron deliberately omits
  graph orchestration -- most agent use cases are sequential loops, not DAGs.
- **Block independence.** ADK-Rust's crates have tighter coupling than neuron's.
  neuron's leaf crates (providers, tools, MCP) have zero knowledge of each other.
- **Durable execution.** neuron's `DurableContext` trait is designed specifically
  for Temporal/Restate integration. ADK-Rust does not have a durability
  abstraction.

### OpenAI Agents SDK (Python)

The OpenAI Agents SDK provides a clean Python API for building agents with
strong support for handoff protocols between agents.

**Where Agents SDK excels:**
- Elegant handoff protocol for multi-agent systems
- Built-in MCP support
- Well-documented, easy to get started
- Built-in tracing

**Where neuron differs:**
- **Language.** neuron is Rust, giving you compile-time safety, zero-cost
  abstractions, and predictable performance. The Agents SDK is Python-only.
- **Provider lock-in.** The Agents SDK is designed for OpenAI's API. neuron's
  `Provider` trait is provider-agnostic from the foundation.
- **Building blocks vs. framework.** The Agents SDK is an opinionated framework
  with a specific agent lifecycle model. neuron gives you the pieces to build
  your own lifecycle.

### Pydantic AI (Python)

Pydantic AI brings typed tool arguments and structured output validation to
Python agents. neuron adopted its `ModelRetry` self-correction pattern.

**Where Pydantic AI excels:**
- Typed tool arguments with runtime validation (Pydantic models)
- Multi-provider support
- Clean API for structured output
- The `ModelRetry` pattern for tool self-correction

**Where neuron differs:**
- **Compile-time types.** Pydantic validates at runtime. neuron's `Tool` trait
  uses `schemars::JsonSchema` for schema generation and `serde::Deserialize`
  for deserialization, both checked at compile time.
- **ModelRetry adoption.** neuron's `ToolError::ModelRetry(String)` is directly
  inspired by Pydantic AI. When a tool returns `ModelRetry`, the hint is
  converted to an error tool result so the model can self-correct.
- **UsageLimits adoption.** neuron's `UsageLimits` is inspired by Pydantic AI's
  budget enforcement, extended with tool call limits.
- **Middleware.** Pydantic AI has no tool middleware pipeline. neuron provides
  `TimeoutMiddleware`, `StructuredOutputValidator`, and `RetryLimitedValidator`
  as composable middleware.

## What neuron does not do

Being honest about scope:

- **neuron is not a framework.** It does not give you a `run_agent()` function
  that handles everything. You compose the blocks.
- **neuron does not include a CLI, TUI, or GUI.** Those are built on top of the
  blocks.
- **neuron does not include RAG pipelines.** Retrieval is a tool or context
  strategy implementation, not a core block.
- **neuron does not include sub-agent orchestration.** Multi-agent handoff is
  straightforward composition of `AgentLoop` + `ToolRegistry` and belongs in an
  SDK layer.

## Choosing the right tool

- **If you want a batteries-included Rust agent framework today:** Rig is more
  mature and has a larger ecosystem.
- **If you want composable building blocks you can adopt incrementally:** neuron
  lets you use exactly the pieces you need.
- **If you need durable execution (Temporal/Restate):** neuron is the only Rust
  option with a dedicated `DurableContext` trait.
- **If you work primarily in Python:** Pydantic AI and the OpenAI Agents SDK are
  excellent choices with larger communities.
- **If you need DAG/graph orchestration:** ADK-Rust includes a LangGraph port.
  neuron does not include a graph layer by design.
