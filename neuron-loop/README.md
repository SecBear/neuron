# neuron-loop

The agentic while-loop for the neuron ecosystem. Composes a `Provider`, a
`ToolRegistry`, and a `ContextStrategy` into a loop that sends messages to an
LLM, executes tool calls, manages context compaction, and repeats until the
model produces a final response or a turn limit is reached.

## Key Types

- `AgentLoop<P, C>` -- the core loop, generic over `Provider` and `ContextStrategy`
- `AgentLoopBuilder<P, C>` -- builder for constructing an `AgentLoop` with optional configuration
- `LoopConfig` -- system prompt, max turns, parallel tool execution flag
- `AgentResult` -- final output: response text, all messages, cumulative token usage, turn count
- `TurnResult` -- per-turn result enum for step-by-step iteration
- `BoxedHook` -- type-erased `ObservabilityHook` for dyn-compatible hook storage
- `BoxedDurable` -- type-erased `DurableContext` for dyn-compatible durability

## Usage

Build an `AgentLoop` using the builder pattern. Only `provider` and `context`
are required; tools, config, hooks, and durability are optional with sensible
defaults.

```rust,ignore
use neuron_loop::{AgentLoop, LoopConfig};
use neuron_tool::ToolRegistry;
use neuron_context::SlidingWindowStrategy;
use neuron_types::ToolContext;

// Set up components
let provider = MyProvider::new("claude-sonnet-4-20250514");
let context = SlidingWindowStrategy::new(20, 100_000);

let mut tools = ToolRegistry::new();
tools.register(MyTool);

// Build and run
let mut agent = AgentLoop::builder(provider, context)
    .tools(tools)
    .system_prompt("You are a helpful assistant.")
    .max_turns(10)
    .parallel_tool_execution(true)
    .build();

let tool_ctx = ToolContext { /* ... */ };
let result = agent.run_text("What is 2 + 2?", &tool_ctx).await?;

println!("Response: {}", result.response);
println!("Turns: {}", result.turns);
println!("Tokens used: {} in, {} out",
    result.usage.input_tokens,
    result.usage.output_tokens);
```

For step-by-step iteration (useful for streaming UIs or custom control flow):

```rust,ignore
let mut agent = AgentLoop::builder(provider, context).build();
// Use agent.run_step() to drive one turn at a time
```

Add observability hooks to log, meter, or control loop execution:

```rust,ignore
let agent = AgentLoop::builder(provider, context)
    .hook(my_logging_hook)
    .durability(my_temporal_context)
    .build();
```

## Architecture

`AgentLoop` depends on `neuron-types` (traits) and `neuron-tool`
(`ToolRegistry`). RPITIT traits (`Provider`, `ObservabilityHook`,
`DurableContext`) are type-erased internally via boxed wrappers for
dyn-compatibility. The `ContextStrategy` is used as a generic parameter.

## Part of neuron

This crate is part of [neuron](https://github.com/empathic-ai/neuron), a
composable building-blocks library for AI agents in Rust.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
