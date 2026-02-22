# neuron

Composable building blocks for AI agents in Rust.

Building blocks, not a framework. Each block is an independent Rust crate — pull
one without buying the whole stack.

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

```rust
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

    let ctx = ToolContext::default();
    let result = agent.run_text("Hello!", &ctx).await?;
    println!("{}", result.response);
    Ok(())
}
```

## Crates

| Crate                       | Description                                                   |
| --------------------------- | ------------------------------------------------------------- |
| `neuron-types`              | Shared types and traits — the lingua franca of all blocks     |
| `neuron-tool`               | Tool registry, middleware pipeline, and built-in middleware   |
| `neuron-tool-macros`        | `#[neuron_tool]` proc macro for deriving Tool implementations |
| `neuron-context`            | Context management — token counting, compaction strategies    |
| `neuron-loop`               | The agentic while loop — composes provider + tools + context  |
| `neuron-provider-anthropic` | Anthropic Claude provider (Messages API, streaming)           |
| `neuron-provider-openai`    | OpenAI provider (Chat Completions API, streaming)             |
| `neuron-provider-ollama`    | Ollama local provider (Chat API, NDJSON streaming)            |
| `neuron-mcp`                | MCP (Model Context Protocol) integration via rmcp             |
| `neuron-runtime`            | Sessions, sub-agents, guardrails, durability, sandboxing      |
| `neuron`                    | Umbrella crate with feature flags                             |

## Feature Flags (neuron)

| Feature     | Description                        | Default |
| ----------- | ---------------------------------- | ------- |
| `anthropic` | Anthropic Claude provider          | yes     |
| `openai`    | OpenAI provider                    | no      |
| `ollama`    | Ollama local provider              | no      |
| `mcp`       | Model Context Protocol integration | no      |
| `runtime`   | Sessions, sub-agents, guardrails   | no      |
| `full`      | All of the above                   | no      |

## Architecture

```
neuron-types                    (zero deps, the foundation)
    ^
    |-- neuron-provider-*       (each implements Provider trait)
    |-- neuron-tool             (Tool trait, registry, middleware)
    |-- neuron-mcp              (wraps rmcp, bridges to Tool trait)
    +-- neuron-context          (+ optional Provider for summarization)
            ^
        neuron-loop             (composes provider + tool + context)
            ^
        neuron-runtime          (sub-agents, sessions, durability)
            ^
        neuron                  (umbrella re-export)
```

Arrows point up. No circular dependencies.

## License

MIT OR Apache-2.0
