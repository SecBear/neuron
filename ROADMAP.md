# Roadmap

## Now (v0.1)

What ships today -- 11 independent crates:

- **3 LLM providers** -- Anthropic, OpenAI, Ollama, each in its own crate
- **Tool system** -- `ToolRegistry`, `#[neuron_tool]` derive macro, composable middleware pipeline
- **Context management** -- 4 compaction strategies (sliding window, tool result clearing, LLM-powered summarization, composite), token counting, persistent context, system prompt injection
- **Agent loop** -- configurable `AgentLoop` with streaming, max turns, tool dispatch
- **MCP** -- full Model Context Protocol: client (stdio + Streamable HTTP), server, `McpToolBridge`
- **Runtime** -- sessions (`InMemorySessionStorage`, `FileSessionStorage`), sub-agents, input/output guardrails, `PermissionPolicy`, `Sandbox` trait, `DurableContext`
- **Umbrella crate** -- `neuron` with feature flags for all of the above

## Next (v0.2)

Near-term planned work:

- **More providers** -- Gemini, Groq, DeepSeek (these follow the OpenAI Chat Completions format, so the existing OpenAI mapping code can be reused with minimal changes)
- **`EmbeddingProvider` trait** -- new trait in `neuron-types` for embedding models, separate from `Provider`
- **OpenTelemetry hook** -- concrete `ObservabilityHook` implementation wrapping the `tracing` + `opentelemetry` crates, following GenAI Semantic Conventions
- **More examples** -- streaming, multi-turn conversation, structured output, multi-agent orchestration
- **CHANGELOG.md** -- per-crate changelogs with release-please automation

## Later

Community-driven and longer-term:

- **Additional providers** -- Bedrock, Cohere, Mistral, xAI, Together, HuggingFace, and others as independent crates
- **`VectorStore` trait** -- trait in `neuron-types` with reference implementations (in-memory, Qdrant) as separate crates
- **Resilience layer** -- retry with backoff, circuit breaker, rate limiting, provider failover (likely a `neuron-resilience` crate)
- **Config-driven provider routing** -- YAML/TOML manifests for provider configuration, model mapping, and routing strategies
- **Cost and usage tracking** -- token pricing metadata, per-request cost estimation
- **Dedicated docs site** -- beyond docs.rs, a standalone site with guides and tutorials

## Not Planned

These are explicit non-goals. neuron provides building blocks -- build these on
top if you need them:

- CLI, TUI, or GUI applications
- Opinionated agent framework (compose one from the blocks)
- Built-in RAG pipeline (the `VectorStore` trait will exist, but a full loader-splitter-embedder-retriever chain is out of scope)
- Workflow or DAG engine (use Temporal, Restate, or a dedicated orchestrator)
- Graph orchestrator (not our concern)
