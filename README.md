# rust-agent-blocks

Composable building blocks for AI agents in Rust.

Building blocks, not a framework. Each block is an independent Rust crate —
pull one without buying the whole stack.

## Quick Start

```rust
use agent_blocks::prelude::*;
use agent_blocks::anthropic::Anthropic;

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

| Crate | Description |
|-------|-------------|
| `agent-types` | Shared types and traits — the lingua franca of all blocks |
| `agent-tool` | Tool registry, middleware pipeline, and built-in middleware |
| `agent-tool-macros` | `#[agent_tool]` proc macro for deriving Tool implementations |
| `agent-context` | Context management — token counting, compaction strategies |
| `agent-loop` | The agentic while loop — composes provider + tools + context |
| `agent-provider-anthropic` | Anthropic Claude provider (Messages API, streaming) |
| `agent-provider-openai` | OpenAI provider (Chat Completions API, streaming) |
| `agent-provider-ollama` | Ollama local provider (Chat API, NDJSON streaming) |
| `agent-mcp` | MCP (Model Context Protocol) integration via rmcp |
| `agent-runtime` | Sessions, sub-agents, guardrails, durability, sandboxing |
| `agent-blocks` | Umbrella crate with feature flags |

## Feature Flags (agent-blocks)

| Feature | Description | Default |
|---------|-------------|---------|
| `anthropic` | Anthropic Claude provider | yes |
| `openai` | OpenAI provider | no |
| `ollama` | Ollama local provider | no |
| `mcp` | Model Context Protocol integration | no |
| `runtime` | Sessions, sub-agents, guardrails | no |
| `full` | All of the above | no |

## Architecture

```
agent-types                     (zero deps, the foundation)
    ^
    |-- agent-provider-*        (each implements Provider trait)
    |-- agent-tool              (Tool trait, registry, middleware)
    |-- agent-mcp               (wraps rmcp, bridges to Tool trait)
    +-- agent-context           (+ optional Provider for summarization)
            ^
        agent-loop              (composes provider + tool + context)
            ^
        agent-runtime           (sub-agents, sessions, durability)
            ^
        agent-blocks            (umbrella re-export)
```

Arrows point up. No circular dependencies.

## License

MIT OR Apache-2.0
