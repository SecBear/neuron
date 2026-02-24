# neuron-loop

[![crates.io](https://img.shields.io/crates/v/neuron-loop.svg)](https://crates.io/crates/neuron-loop)
[![docs.rs](https://docs.rs/neuron-loop/badge.svg)](https://docs.rs/neuron-loop)
[![license](https://img.shields.io/crates/l/neuron-loop.svg)](LICENSE-MIT)

The agentic while-loop for the neuron ecosystem. Composes a `Provider`, a
`ToolRegistry`, and a `ContextStrategy` into a loop that sends messages to an
LLM, executes tool calls, manages context compaction, and repeats until the
model produces a final response or a turn limit is reached.

## Installation

```sh
cargo add neuron-loop
```

## Key Types

- `AgentLoop<P, C>` -- the core loop, generic over [`Provider`](https://docs.rs/neuron-types/latest/neuron_types/trait.Provider.html) and [`ContextStrategy`](https://docs.rs/neuron-types/latest/neuron_types/trait.ContextStrategy.html)
- `AgentLoopBuilder<P, C>` -- builder for constructing an `AgentLoop` with optional configuration
- `LoopConfig` -- system prompt, max turns, parallel tool execution flag, optional `UsageLimits`
- `AgentResult` -- final output: response text, all messages, cumulative token usage, turn count
- `TurnResult` -- per-turn result for step-by-step iteration:
  - `ToolsExecuted { calls, results }` — tool calls were made and executed
  - `FinalResponse(AgentResult)` — model produced a final text response
  - `CompactionOccurred { old_tokens, new_tokens }` — context was compacted
  - `MaxTurnsReached` — turn limit hit
  - `Error(LoopError)` — something failed

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

For step-by-step iteration (streaming UIs, custom control flow, or injecting
messages between turns):

```rust,ignore
use neuron_loop::{AgentLoop, TurnResult};
use neuron_types::{Message, Role, ContentBlock, ToolContext};

let mut agent = AgentLoop::builder(provider, context).tools(tools).build();
let user_msg = Message {
    role: Role::User,
    content: vec![ContentBlock::Text("What is 2 + 2?".into())],
};
let tool_ctx = ToolContext::default();
let mut steps = agent.run_step(user_msg, &tool_ctx);

while let Some(turn) = steps.next().await {
    match turn {
        TurnResult::ToolsExecuted { calls, .. } => {
            println!("executed {} tool calls", calls.len());
        }
        TurnResult::FinalResponse(result) => {
            println!("final: {}", result.response);
            break;
        }
        TurnResult::CompactionOccurred { old_tokens, new_tokens } => {
            println!("compacted {old_tokens} -> {new_tokens} tokens");
        }
        TurnResult::MaxTurnsReached => break,
        TurnResult::Error(e) => { eprintln!("error: {e}"); break; }
    }
}
```

Add observability hooks to log, meter, or control loop execution:

```rust,ignore
let agent = AgentLoop::builder(provider, context)
    .hook(my_logging_hook)
    .durability(my_temporal_context)
    .build();
```

## Cancellation

The loop respects `ToolContext.cancellation_token` (a `tokio_util::sync::CancellationToken`).
Cancellation is checked at the top of each iteration and before tool execution.
When cancelled, the loop returns `LoopError::Cancelled`.

```rust,ignore
use tokio_util::sync::CancellationToken;

let token = CancellationToken::new();
let tool_ctx = ToolContext {
    cancellation_token: token.clone(),
    // ...
};

// Cancel from another task
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(5)).await;
    token.cancel();
});

let result = agent.run_text("Long task", &tool_ctx).await;
// Returns Err(LoopError::Cancelled) if cancelled
```

## Parallel Tool Execution

When `LoopConfig.parallel_tool_execution` is `true` and the model returns
multiple tool calls in a single response, all calls execute concurrently.
When `false` (the default), tool calls execute sequentially in order.

```rust,ignore
let mut agent = AgentLoop::builder(provider, context)
    .parallel_tool_execution(true)
    .build();
```

## Usage Limits

Set `LoopConfig.usage_limits` to enforce token budget constraints across the
entire loop. When cumulative usage exceeds any configured limit, the loop
returns `LoopError::UsageLimitExceeded`.

```rust,ignore
use neuron_loop::{AgentLoop, LoopConfig};
use neuron_types::UsageLimits;

let limits = UsageLimits::default()
    .with_input_tokens_limit(50_000)
    .with_output_tokens_limit(10_000)
    .with_total_tokens_limit(60_000);

let mut agent = AgentLoop::builder(provider, context)
    .usage_limits(limits)
    .build();

// The loop will return Err(LoopError::UsageLimitExceeded(_))
// if any limit is exceeded during execution.
```

Limits are checked after each LLM response and after tool execution. This is
inspired by Pydantic AI's usage limit pattern, ported to Rust with compile-time
type safety.

## Architecture

`AgentLoop` depends on [`neuron-types`](https://docs.rs/neuron-types) (traits) and
[`neuron-tool`](https://docs.rs/neuron-tool) ([`ToolRegistry`](https://docs.rs/neuron-tool/latest/neuron_tool/struct.ToolRegistry.html)).
RPITIT traits (`Provider`, `ObservabilityHook`, `DurableContext`) are
type-erased internally via `BoxedHook` and `BoxedDurable` wrappers for
dyn-compatibility — you never construct these directly; they're created by
`.hook()` and `.durability()` on the builder. The `ContextStrategy` is used as
a generic parameter.

## Part of neuron

This crate is part of [neuron](https://github.com/secbear/neuron), a
composable building-blocks library for AI agents in Rust.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
