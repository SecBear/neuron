# Introduction

**neuron** provides composable building blocks for AI agents in Rust. Each block
is an independent crate, versioned and published separately. Pull one block
without buying the whole stack.

## Philosophy: serde, not serde_json

neuron is to agent frameworks what `serde` is to `serde_json`. It defines traits
(`Provider`, `Tool`, `ContextStrategy`) and provides foundational implementations.
An SDK layer composes these blocks into opinionated workflows.

Every Rust and Python agent framework converges on the same ~300-line while loop.
The differentiation is never the loop — it's the blocks around it: context
management, tool pipelines, durability, runtime. Nobody ships those blocks
independently. That's the gap neuron fills.

## What's Included

neuron ships the following crates:

| Crate | Purpose |
|-------|---------|
| `neuron-types` | Core traits and types — `Provider`, `Tool`, `ContextStrategy`, `Message` |
| `neuron-provider-anthropic` | Anthropic Messages API (streaming, tool use, server-side compaction) |
| `neuron-provider-openai` | OpenAI Chat Completions + Embeddings API |
| `neuron-provider-ollama` | Ollama local inference API |
| `neuron-tool` | `ToolRegistry` with composable middleware pipeline |
| `neuron-tool-macros` | `#[neuron_tool]` derive macro |
| `neuron-context` | Compaction strategies, token counting, system prompt injection |
| `neuron-loop` | Configurable `AgentLoop` with streaming, cancellation, parallel tools |
| `neuron-mcp` | Model Context Protocol client and server (stdio + Streamable HTTP) |
| `neuron-runtime` | Sessions, guardrails, `TracingHook`, `GuardrailHook`, `DurableContext` |
| `neuron-otel` | OpenTelemetry instrumentation with GenAI semantic conventions (`gen_ai.*` spans) |
| `neuron` | Umbrella crate with feature flags for all of the above |

## Who Is This For?

- **Rust developers** building AI-powered applications who want control over
  each layer of the stack
- **Framework authors** who need well-tested building blocks to compose into
  higher-level abstractions
- **AI agents** (like Claude Code) that need to understand, evaluate, and work
  with the codebase

## What neuron Is NOT

neuron is the layer below frameworks. It does not provide:

- CLI, TUI, or GUI applications
- Opinionated agent framework (compose one from the blocks)
- RAG pipeline (use the `EmbeddingProvider` trait with your own retrieval)
- Workflow engine (integrate with Temporal/Restate via `DurableContext`)
- Retry middleware (use tower or your durable engine's retry policy)

## Next Steps

- [Installation](getting-started/installation.md) — add neuron to your project
- [Quickstart](getting-started/quickstart.md) — build your first agent in 50 lines
- [Core Concepts](getting-started/concepts.md) — understand the five key abstractions
