# Quickstart

Build a working AI agent in ~50 lines of Rust.

## Prerequisites

- Rust 1.90+
- An API key for Anthropic or OpenAI (set as `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`)

## The Agent

```rust,no_run
use neuron::prelude::*;
use neuron_provider_anthropic::Anthropic;
use neuron_tool::ToolRegistry;
use neuron_loop::AgentLoop;
use neuron_context::SlidingWindowStrategy;
use neuron_types::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// 1. Define a tool
struct GetWeather;

impl Tool for GetWeather {
    const NAME: &'static str = "get_weather";
    type Args = WeatherArgs;
    type Output = String;
    type Error = std::io::Error;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_weather".to_string(),
            title: None,
            description: "Get the current weather for a city".to_string(),
            input_schema: schemars::schema_for!(WeatherArgs).into(),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(&self, args: WeatherArgs, _ctx: &ToolContext) -> Result<String, std::io::Error> {
        Ok(format!("Weather in {}: 72°F, sunny", args.city))
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WeatherArgs {
    /// The city to get weather for
    city: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. Set up a provider
    let provider = Anthropic::from_env()?;

    // 3. Register tools
    let mut tools = ToolRegistry::new();
    tools.register(GetWeather);

    // 4. Create the context strategy
    let context = SlidingWindowStrategy::new(10, 100_000);

    // 5. Build and run the agent loop
    let mut agent = AgentLoop::builder(provider, context)
        .tools(tools)
        .system_prompt("You are a helpful weather assistant.")
        .max_turns(5)
        .build();

    let ctx = ToolContext::default();
    let result = agent.run(Message::user("What's the weather in San Francisco?"), &ctx).await?;
    println!("{}", result.response);
    Ok(())
}
```

## What Just Happened?

1. **Provider** — `Anthropic::from_env()` creates an API client from `ANTHROPIC_API_KEY`
2. **Tool** — `GetWeather` implements the `Tool` trait with typed args and output
3. **Registry** — `ToolRegistry` stores tools and handles JSON deserialization
4. **Context** — `SlidingWindowStrategy` keeps the conversation within token limits
5. **Loop** — `AgentLoop` drives the conversation: send message, get response, execute tools, repeat

The agent loop handles multi-turn tool use automatically. When Claude calls `get_weather`,
the loop executes the tool and sends the result back. The loop continues until Claude
responds without tool calls or hits `max_turns`.

## Next Steps

- [Core Concepts](concepts.md) — understand Provider, Tool, ContextStrategy, and more
- [Tools Guide](../guides/tools.md) — the `#[neuron_tool]` macro, middleware, and advanced patterns
- [Providers Guide](../guides/providers.md) — switching between Anthropic, OpenAI, and Ollama
