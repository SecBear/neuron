# Roadmap

## Scope Philosophy

neuron is **serde, not serde_json**. It defines traits, provides foundational
implementations, and gets out of the way. An SDK layer (your framework)
composes these blocks into opinionated workflows.

**The test:** If removing a feature forces every user to reimplement 200+ lines
of non-trivial code, it belongs in neuron. If it's 20-50 lines of
straightforward composition, it belongs in the SDK layer.

**neuron provides:** trait definitions, reference implementations (one or two
per trait), the commodity agent loop, and infrastructure any agent needs
regardless of framework (sessions, guardrails, durability, tools, context, MCP).

**neuron does NOT provide:** agent lifecycle management, opinionated composition
patterns, retry/resilience (use tower or durable engines), or framework-level DX
(`Agent<Deps, Output>` generics, handoff protocols, testing overrides).

## Now

What ships today:

- **LLM providers** -- Anthropic, OpenAI, Ollama, each in its own crate
- **Tool system** -- `ToolRegistry`, `#[neuron_tool]` derive macro, composable middleware pipeline
- **Context management** -- 4 compaction strategies (sliding window, tool result clearing, LLM-powered summarization, composite), token counting, persistent context, system prompt injection
- **Agent loop** -- configurable `AgentLoop` with streaming, max turns, tool dispatch
- **MCP** -- full Model Context Protocol: client (stdio + Streamable HTTP), server, `McpToolBridge`
- **Runtime** -- sessions (`InMemorySessionStorage`, `FileSessionStorage`), input/output guardrails, `PermissionPolicy`, `Sandbox` trait, `DurableContext`
- **Umbrella crate** -- `neuron` with feature flags for all of the above
- **`Message::user()`, `::assistant()`, `::system()`** -- convenience constructors for the common case
- **`impl Default for ToolContext`** -- zero-boilerplate tool context construction
- **`from_env()` on all providers** -- Anthropic, OpenAI, and Ollama all load credentials from environment variables
- **Compile-checked trait doc examples** -- `no_run` instead of `ignore` so doc examples are syntax-checked
- **Server-side context management** -- `ContextManagement` request field, `ContentBlock::Compaction`, `StopReason::Compaction` for Anthropic's server-side compaction API; loop continues automatically on compaction
- **`ToolError::ModelRetry`** -- self-correction pattern (from Pydantic AI); tools return a hint string converted to an error tool result, letting the model retry with guidance
- **`EmbeddingProvider` trait** -- new trait in `neuron-types` for embedding models, separate from `Provider`; `EmbeddingRequest`, `EmbeddingResponse`, `EmbeddingUsage` types; OpenAI implementation in `neuron-provider-openai`
- **`TracingHook`** -- concrete `ObservabilityHook` in `neuron-runtime` using the `tracing` crate; maps all 8 hook events to structured spans; use with `tracing-opentelemetry` for OTel export
- **`GuardrailHook`** -- `ObservabilityHook` adapter in `neuron-runtime` that wires input/output guardrails into the hook system; builder pattern, Tripwire → Terminate, Warn → log + continue
- **Cancellation support** -- `CancellationToken` checked at loop top and before each tool execution; `LoopError::Cancelled` variant
- **Parallel tool execution** -- `parallel_tool_execution` flag on `LoopConfig`; uses `futures::future::join_all` when enabled with multiple tool calls
- **Per-crate CHANGELOG.md** -- release-please automation with Conventional Commits
- **crates.io categories** -- `categories` in all `Cargo.toml` manifests for better discoverability
- **docs.rs links** -- `documentation` field in all `Cargo.toml` manifests pointing to docs.rs
- **GitHub topics** -- repository topics for discoverability (rust, ai, llm, agent, ai-agent, tools, mcp, building-blocks, context-management)
- **Expanded examples** -- streaming, multi-turn, structured output, multi-provider, context management, model retry, tool middleware, tracing hook, local durable context, derive tool, embeddings, cancellation, parallel tools, full production, testing agents
- **Documentation site** -- mdBook on GitHub Pages with getting-started guides, in-depth guides, architecture pages, and error handling reference
- **Community files** -- CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md, issue templates, PR template
- **CI hardening** -- GitHub Actions with fmt, clippy, test, doc, MSRV matrix
- **Property-based tests** -- proptest for serde roundtrips, error classification, token monotonicity, middleware ordering
- **Criterion benchmarks** -- serialization throughput, token counting, agent loop latency
- **Fuzz targets** -- cargo-fuzz for all 3 provider response parsers
- **`UsageLimits`** -- token/request budget enforcement in the agentic loop; `LoopConfig.usage_limits` field, `LoopError::UsageLimitExceeded` variant; inspired by Pydantic AI's usage limit pattern
- **`TimeoutMiddleware`** -- per-tool execution timeouts via `tokio::time::timeout`; register as global or per-tool middleware to prevent runaway tool calls
- **`StructuredOutputValidator` + `RetryLimitedValidator`** -- JSON Schema validation middleware returning `ToolError::ModelRetry` for self-correction; validates tool input against schemas and gives the model a chance to retry with a hint, with configurable retry limits
- **`neuron-otel`** -- OpenTelemetry instrumentation crate implementing `ObservabilityHook` with `tracing` spans following GenAI semantic conventions (`gen_ai.loop.iteration`, `gen_ai.chat`, `gen_ai.execute_tool`, `gen_ai.context.compaction`); opt-in content capture via `OtelConfig`

## Next

Near-term planned work:

- **Usage limits** -- `UsageLimits` struct with request, tool call, and token budgets; enforced at 3 check points in the agent loop (pre-request, post-response, pre-tool-call). `LoopError::UsageLimitExceeded` on breach.
- **Tool timeout middleware** -- `TimeoutMiddleware` in `neuron-tool` with per-tool duration overrides. Wraps tool execution in `tokio::time::timeout`.
- **Structured output middleware** -- `StructuredOutputMiddleware` that injects a result tool with a JSON Schema, validates output, and feeds validation errors back to the model via `ToolError::ModelRetry`.
- **OpenTelemetry instrumentation** -- `neuron-otel` crate implementing `ObservabilityHook` with OTel GenAI semantic conventions (`gen_ai.*` namespace). Uses `tracing` spans; users bring their own subscriber.

## Later

Community-driven and longer-term:

- **More providers** -- Gemini, Groq, DeepSeek (OpenAI Chat Completions compat), Bedrock, Cohere, Mistral, xAI, Together, HuggingFace as independent crates
- **An SDK layer** -- higher-level composition on neuron building blocks: `Agent<Deps, Output>` typed generics (from Pydantic AI), handoff protocol (from OpenAI Agents SDK), parallel guardrails, sub-agent orchestration. neuron stays as building blocks; an SDK layer is the opinionated framework built on top.
- **`VectorStore` trait** -- trait in `neuron-types` with reference implementations (in-memory, Qdrant) as separate crates
- **Token pricing metadata** -- `ProviderPricing` data in `neuron-types` so cost estimation can be done by callers; cost estimation logic belongs in the SDK layer

## Not Planned

These are explicit non-goals. neuron provides building blocks -- build these on
top if you need them:

- CLI, TUI, or GUI applications
- Opinionated agent framework (compose one from the blocks)
- Built-in RAG pipeline (the `VectorStore` trait will exist, but a full loader-splitter-embedder-retriever chain is out of scope)
- Workflow or DAG engine (use Temporal, Restate, or a dedicated orchestrator)
- Graph orchestrator (not our concern)
- **Retry/resilience middleware** -- use tower middleware or your durable engine's retry policy; neuron exposes `ProviderError::is_retryable()` and `DurableContext` with `RetryPolicy` for durable engines
- **Config-driven provider routing** -- YAML/TOML manifests for provider selection and model mapping are opinionated composition that belongs in application code or the SDK layer
- **Sub-agent orchestration registry** -- `SubAgentManager` is moving to an SDK layer; compose sub-agents directly with `AgentLoop` + `ToolRegistry`
