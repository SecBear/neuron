# neuron

[![CI](https://github.com/SecBear/neuron/actions/workflows/ci.yml/badge.svg)](https://github.com/SecBear/neuron/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/SecBear/neuron/graph/badge.svg)](https://codecov.io/gh/SecBear/neuron)
[![crates.io](https://img.shields.io/crates/v/neuron.svg)](https://crates.io/crates/neuron)
[![docs.rs](https://docs.rs/neuron/badge.svg)](https://docs.rs/neuron)
[![MSRV](https://img.shields.io/badge/MSRV-1.90-blue.svg)](https://blog.rust-lang.org/)
[![license](https://img.shields.io/crates/l/neuron.svg)](LICENSE-MIT)

Composable building blocks for AI agents in Rust.

Building blocks, not a framework. Each block is an independent Rust crate — pull
one without buying the whole stack.

**[Read the docs](https://secbear.github.io/neuron/)** — quickstart, guides, and architecture.

## Why neuron?

Most AI agent libraries are Python-first, framework-shaped, and opinionated.
neuron is none of those.

- **Rust-native** — no Python interop, no runtime overhead
- **Composable** — use one crate or all of them, no buy-in required
- **Model-agnostic** — Anthropic, OpenAI, Ollama, or bring your own
- **Context-aware** — sliding window, compaction, and token counting built in
- **MCP-native** — first-class Model Context Protocol support
- **No magic** — it's a while loop with tools attached, not a framework

## Quick Start

```rust,no_run
use neuron::prelude::*;
use neuron::anthropic::Anthropic;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Anthropic::from_env()?;
    let context = SlidingWindowStrategy::new(10, 100_000);

    let mut agent = AgentLoop::builder(provider, context)
        .system_prompt("You are a helpful assistant.")
        .build();

    let result = agent.run_text("Hello!", &ToolContext::default()).await?;
    println!("{}", result.response);
    Ok(())
}
```

## Learn more

The [documentation site](https://secbear.github.io/neuron/) has everything you
need to get productive:

- [**Quickstart**](https://secbear.github.io/neuron/getting-started/quickstart.html) — build a working agent in 5 minutes
- [**Tools guide**](https://secbear.github.io/neuron/guides/tools.html) — define tools, compose middleware, validate inputs
- [**Context management**](https://secbear.github.io/neuron/guides/context.html) — keep conversations within token limits
- [**The agent loop**](https://secbear.github.io/neuron/guides/loop.html) — multi-turn tool dispatch, streaming, cancellation
- [**MCP integration**](https://secbear.github.io/neuron/guides/mcp.html) — connect to any MCP server or expose tools as one
- [**Testing agents**](https://secbear.github.io/neuron/guides/testing.html) — mock providers, deterministic tests

## Crates

| Crate                       | Description                                                   |
| --------------------------- | ------------------------------------------------------------- |
| `neuron-types`              | Core traits for AI agents — Provider, Tool, ContextStrategy   |
| `neuron-tool`               | Tool registry with composable middleware pipeline             |
| `neuron-tool-macros`        | Derive macro for LLM tool definitions from Rust structs       |
| `neuron-context`            | LLM context management — sliding window, token counting, compaction |
| `neuron-loop`               | Agentic loop — multi-turn tool dispatch, streaming, conversation management |
| `neuron-provider-anthropic` | Anthropic Claude — Messages API, streaming, prompt caching    |
| `neuron-provider-openai`    | OpenAI — Chat Completions API, streaming, OpenAI-compatible endpoints |
| `neuron-provider-ollama`    | Ollama — local LLM inference with NDJSON streaming            |
| `neuron-mcp`                | MCP client and server — stdio, HTTP, tool bridging            |
| `neuron-runtime`            | Sessions, sub-agents, guardrails, durable execution           |
| `neuron-otel`               | OTel instrumentation — GenAI semantic conventions with tracing spans |
| `neuron`                    | Umbrella crate with feature flags                             |

## Feature Flags (neuron)

| Feature     | Description                        | Default |
| ----------- | ---------------------------------- | ------- |
| `anthropic` | Anthropic Claude provider          | yes     |
| `openai`    | OpenAI provider                    | no      |
| `ollama`    | Ollama local provider              | no      |
| `mcp`       | Model Context Protocol integration | no      |
| `runtime`   | Sessions, sub-agents, guardrails   | no      |
| `otel`      | OpenTelemetry instrumentation       | no      |
| `full`      | All of the above                   | no      |

## Architecture

```
neuron-types                    (zero deps, the foundation)
    ^
    |-- neuron-provider-*       (each implements Provider trait)
    |-- neuron-otel             (OTel instrumentation, GenAI semantic conventions)
    |-- neuron-context          (compaction strategies, token counting)
    +-- neuron-tool             (Tool trait, registry, middleware)
            ^
            |-- neuron-mcp      (wraps rmcp, bridges to Tool trait)
            |-- neuron-loop     (provider loop with tool dispatch)
            +-- neuron-runtime  (sessions, DurableContext, guardrails, sandbox)
                    ^
                neuron          (umbrella re-export)
```

Arrows point up. No circular dependencies.

## Comparison

How neuron compares to the two most established Rust alternatives, based on
[source-level analysis](docs/competitive-analysis.md) of 6 frameworks:

| Capability | neuron | Rig | genai |
|---|---|---|---|
| Crate independence | One crate per provider | All providers in `rig-core` | Single crate |
| LLM providers | Anthropic, OpenAI, Ollama | Many | Many |
| Tool middleware | Composable chain | None | None |
| Context compaction | 4 strategies, token-aware | None | None |
| MCP (full spec) | Client + server + bridge | Client (rmcp) | None |
| Durable execution | `DurableContext` trait | None | None |
| Guardrails / sandbox | `InputGuardrail`, `OutputGuardrail`, `PermissionPolicy`, `Sandbox` | None | None |
| Sessions | `SessionStorage` trait + impls | None | None |
| Vector stores / RAG | None | Many integrations | None |
| Embeddings | None | `EmbeddingModel` trait | Yes |
| Usage limits | `UsageLimits` token/request budget | None | None |
| Tool timeouts | `TimeoutMiddleware` per-tool | None | None |
| Structured output validation | `StructuredOutputValidator` with self-correction | None | None |
| OpenTelemetry | GenAI semantic conventions (`neuron-otel`) | Full integration | None |

**Where others lead today:** Rig has a larger provider and vector store
ecosystem with an extensive example set. genai covers many providers in one
ergonomic crate. neuron's architecture is ahead; the ecosystem is growing.
See the [roadmap](ROADMAP.md) for what comes next.

See [docs/competitive-analysis.md](docs/competitive-analysis.md) for the
full unbiased comparison of 6 frameworks with code-level evidence.

## License

MIT OR Apache-2.0
