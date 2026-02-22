# neuron

[![crates.io](https://img.shields.io/crates/v/neuron.svg)](https://crates.io/crates/neuron)
[![docs.rs](https://docs.rs/neuron/badge.svg)](https://docs.rs/neuron)
[![license](https://img.shields.io/crates/l/neuron.svg)](LICENSE-MIT)

Rust library for production AI agents. Add one dependency, enable the features
you need, and compose providers, tools, context strategies, and a runtime into
agents that work.

## Why neuron?

Most AI agent libraries are Python-first, framework-shaped, and opinionated.
neuron is none of those.

- **Rust-native** — no Python interop, no runtime overhead
- **Composable** — use one crate or all of them, no buy-in required
- **Model-agnostic** — Anthropic, OpenAI, Ollama, or bring your own
- **Context-aware** — sliding window, compaction, and token counting built in
- **MCP-native** — first-class Model Context Protocol support
- **No magic** — it's a while loop with tools attached, not a framework

## High-Level Features

- **Multi-provider LLM support** — Anthropic Claude, OpenAI GPT, Ollama local models, or implement the [`Provider`](https://docs.rs/neuron-types/latest/neuron_types/trait.Provider.html) trait for your own
- **Composable tool middleware** — axum-style middleware pipeline for tool calls: logging, auth, rate limiting, retries
- **Context compaction** — sliding window, tool result clearing, LLM summarization, and composite strategies to keep conversations within token limits
- **Model Context Protocol** — full MCP client and server, stdio and HTTP transports, automatic tool bridging
- **Input/output guardrails** — safety checks that run before input reaches the LLM or before output reaches the user, with tripwire semantics
- **Sessions and sub-agents** — persist conversations, spawn isolated sub-agents with filtered tool sets and depth guards
- **Durable execution** — wrap side effects for crash recovery via Temporal, Restate, or Inngest
- **Streaming** — real-time token streaming with hook integration across all providers

## Installation

```sh
cargo add neuron                    # Anthropic provider included by default
cargo add neuron --features full    # all providers + MCP + runtime
```

## Quick Start

```rust,no_run
use neuron::prelude::*;
use neuron::anthropic::Anthropic;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Anthropic::new("your-api-key").model("claude-sonnet-4-20250514");
    let context = SlidingWindowStrategy::new(10, 100_000);
    let tools = ToolRegistry::new();

    let mut agent = AgentLoop::builder(provider, context)
        .system_prompt("You are a helpful assistant.")
        .max_turns(10)
        .tools(tools)
        .build();

    let ctx = ToolContext {
        cwd: std::env::current_dir()?,
        session_id: "demo".into(),
        environment: Default::default(),
        cancellation_token: Default::default(),
        progress_reporter: None,
    };
    let result = agent.run_text("Hello!", &ctx).await?;
    println!("{}", result.response);
    Ok(())
}
```

## Feature Flags

| Feature | Enables | Default |
|-----------|------------------------------------------|---------|
| `anthropic` | `neuron::anthropic` (Anthropic Claude) | yes |
| `openai` | `neuron::openai` (OpenAI GPT) | no |
| `ollama` | `neuron::ollama` (Ollama local) | no |
| `mcp` | `neuron::mcp` (Model Context Protocol) | no |
| `runtime` | `neuron::runtime` (sessions, guardrails) | no |
| `full` | All of the above | no |

## Module Map

| Module | Underlying Crate | Contents |
|-----------------|---------------------------|------------------------------------------|
| `neuron::types` | `neuron-types` | Messages, traits, errors, streaming |
| `neuron::tool` | `neuron-tool` | ToolRegistry, middleware pipeline |
| `neuron::context`| `neuron-context` | Token counting, compaction strategies |
| `neuron::r#loop` | `neuron-loop` | AgentLoop, LoopConfig, AgentResult |
| `neuron::anthropic`| `neuron-provider-anthropic`| Anthropic client (feature-gated) |
| `neuron::openai`| `neuron-provider-openai` | OpenAi client (feature-gated) |
| `neuron::ollama`| `neuron-provider-ollama` | Ollama client (feature-gated) |
| `neuron::mcp` | `neuron-mcp` | McpClient, McpToolBridge (feature-gated) |
| `neuron::runtime`| `neuron-runtime` | Sessions, guardrails (feature-gated) |

> **Note:** `loop` is a Rust keyword, so the loop module is accessed as
> `neuron::r#loop`. In practice, import types directly from the prelude or
> from `neuron_loop`.

## Ecosystem

Each block is also available as a standalone crate:

| Crate | docs.rs |
|-------|---------|
| [`neuron-types`](https://docs.rs/neuron-types) | Core traits — Provider, Tool, ContextStrategy |
| [`neuron-tool`](https://docs.rs/neuron-tool) | Tool registry with composable middleware |
| [`neuron-tool-macros`](https://docs.rs/neuron-tool-macros) | `#[neuron_tool]` derive macro |
| [`neuron-context`](https://docs.rs/neuron-context) | Token counting and compaction strategies |
| [`neuron-loop`](https://docs.rs/neuron-loop) | Agentic while loop |
| [`neuron-provider-anthropic`](https://docs.rs/neuron-provider-anthropic) | Anthropic Claude (Messages API, streaming, prompt caching) |
| [`neuron-provider-openai`](https://docs.rs/neuron-provider-openai) | OpenAI GPT (Chat Completions API, streaming) |
| [`neuron-provider-ollama`](https://docs.rs/neuron-provider-ollama) | Ollama (local NDJSON streaming) |
| [`neuron-mcp`](https://docs.rs/neuron-mcp) | MCP client + server |
| [`neuron-runtime`](https://docs.rs/neuron-runtime) | Sessions, sub-agents, guardrails, durability |

## Comparison

How neuron compares to the two most established Rust alternatives:

| Capability | neuron | Rig | genai |
|---|---|---|---|
| Crate independence | One crate per provider | All providers in `rig-core` | Single crate |
| LLM providers | 3 | 18+ | 16 |
| Tool middleware | Composable chain | None | None |
| Context compaction | 4 strategies, token-aware | None | None |
| MCP (full spec) | Client + server + bridge | Client (rmcp) | None |
| Durable execution | `DurableContext` trait | None | None |
| Guardrails / sandbox | `InputGuardrail`, `OutputGuardrail`, `PermissionPolicy`, `Sandbox` | None | None |
| Sessions | `SessionStorage` trait + impls | None | None |
| Vector stores / RAG | None | 13 integrations | None |
| Embeddings | None | `EmbeddingModel` trait | Yes |

**Where others lead today:** Rig ships 18+ providers, 13 vector store
integrations, and 80+ examples. genai covers 16 providers in one ergonomic
crate. neuron ships 3 providers and zero vector stores — the architecture is
ahead, the ecosystem is behind.

## Prelude Contents

The `neuron::prelude` module re-exports the most commonly used types:

- `CompletionRequest`, `CompletionResponse`, `Message`, `Role`, `ContentBlock`,
  `ContentItem`, `SystemPrompt`, `TokenUsage`, `StopReason` — conversation
  primitives.
- `Provider` — the LLM provider trait.
- `Tool`, `ToolDyn`, `ToolDefinition`, `ToolContext`, `ToolOutput`,
  `ToolError` — tool system types.
- `ToolRegistry` — tool registration and dispatch.
- `SlidingWindowStrategy` — context compaction.
- `AgentLoop`, `AgentLoopBuilder`, `AgentResult`, `LoopConfig` — the agentic
  loop.

## Learning Path

Run examples in this order to learn neuron incrementally:

1. `neuron-provider-anthropic/examples/basic.rs` — single completion
2. `neuron-provider-anthropic/examples/streaming.rs` — real-time token streaming
3. `neuron-tool/examples/custom_tool.rs` — define and register tools
4. `neuron-tool/examples/middleware.rs` — composable tool middleware
5. `neuron-loop/examples/agent_loop.rs` — multi-turn agent with tools (no API key)
6. `neuron-loop/examples/multi_turn.rs` — conversation accumulation (no API key)
7. `neuron-context/examples/compaction.rs` — token counting and compaction
8. `neuron/examples/full_agent.rs` — end-to-end production agent
9. `neuron/examples/structured_output.rs` — JSON Schema output
10. `neuron/examples/multi_provider.rs` — swap providers at runtime
11. `neuron-runtime/examples/guardrails.rs` — input/output safety checks
12. `neuron-runtime/examples/sub_agents.rs` — spawn isolated sub-agents
13. `neuron-runtime/examples/sessions.rs` — conversation persistence
14. `neuron-mcp/examples/mcp_client.rs` — MCP server integration

## Part of neuron

This is the root crate of [neuron](https://github.com/secbear/neuron). For
maximum independence, depend on individual block crates (`neuron-types`,
`neuron-provider-anthropic`, etc.) directly.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
