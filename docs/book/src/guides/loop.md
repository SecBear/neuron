# The agent loop

`AgentLoop` is the commodity while loop at the center of every agent. It
composes a `Provider`, a `ToolRegistry`, and a `ContextStrategy` into a loop
that calls the LLM, executes tools, manages context, and repeats until the model
returns a final text response or a limit is reached.

## Quick example

```rust,ignore
use neuron_context::SlidingWindowStrategy;
use neuron_loop::AgentLoop;
use neuron_tool::ToolRegistry;
use neuron_types::ToolContext;

let provider = Anthropic::from_env()?;
let context = SlidingWindowStrategy::new(20, 100_000);

let mut tools = ToolRegistry::new();
tools.register(MySearchTool);
tools.register(MyCalculateTool);

let mut agent = AgentLoop::builder(provider, context)
    .tools(tools)
    .system_prompt("You are a helpful research assistant.")
    .max_turns(15)
    .parallel_tool_execution(true)
    .build();

let ctx = ToolContext::default();
let result = agent.run_text("Find the population of Tokyo", &ctx).await?;
println!("Response: {}", result.response);
println!("Turns: {}, Tokens: {} in / {} out",
    result.turns, result.usage.input_tokens, result.usage.output_tokens);
```

## Building an `AgentLoop`

### The builder pattern

`AgentLoop::builder(provider, context)` returns an `AgentLoopBuilder` with
sensible defaults. Only the provider and context strategy are required.

```rust,ignore
let agent = AgentLoop::builder(provider, context)
    .tools(registry)                    // ToolRegistry (default: empty)
    .system_prompt("You are helpful.")  // SystemPrompt (default: empty)
    .max_turns(10)                      // Option<usize> (default: None = unlimited)
    .parallel_tool_execution(true)      // bool (default: false)
    .hook(my_logging_hook)              // ObservabilityHook (can add multiple)
    .durability(my_durable_ctx)         // DurableContext (optional)
    .build();
```

### Direct construction

You can also construct directly when you need to set the full `LoopConfig`:

```rust,ignore
use neuron_loop::{AgentLoop, LoopConfig};
use neuron_types::SystemPrompt;

let config = LoopConfig {
    system_prompt: SystemPrompt::Text("You are a code reviewer.".into()),
    max_turns: Some(20),
    parallel_tool_execution: true,
};

let agent = AgentLoop::new(provider, tools, context, config);
```

## Running the loop

### `run()` -- drive to completion

Appends the user message, then loops until the model returns a text-only
response or the turn limit is reached.

```rust,ignore
let result = agent.run(Message::user("Hello!"), &tool_ctx).await?;
// result: AgentResult { response, messages, usage, turns }
```

### `run_text()` -- convenience for text input

Wraps a `&str` into a `Message::user()` and calls `run()`:

```rust,ignore
let result = agent.run_text("What is 2 + 2?", &tool_ctx).await?;
```

### `run_stream()` -- streaming output

Uses `provider.complete_stream()` for real-time token output. Returns a
channel receiver that yields `StreamEvent`s:

```rust,ignore
let mut rx = agent.run_stream(Message::user("Explain Rust ownership"), &tool_ctx).await;

while let Some(event) = rx.recv().await {
    match event {
        StreamEvent::TextDelta(text) => print!("{text}"),
        StreamEvent::ToolUse { name, .. } => println!("\n[calling {name}...]"),
        StreamEvent::Usage(usage) => println!("\n[{} tokens]", usage.output_tokens),
        StreamEvent::MessageComplete(_) => println!("\n[done]"),
        StreamEvent::Error(err) => eprintln!("Error: {err}"),
        _ => {}
    }
}
```

Tool execution is handled between streaming turns. The loop streams the LLM
response, executes any tool calls, appends results, and streams the next turn.

### `run_step()` -- one turn at a time

Returns a `StepIterator` that lets you advance the loop manually. Between turns
you can inspect messages, inject new ones, and modify the tool registry.

```rust,ignore
let mut steps = agent.run_step(Message::user("Plan a trip"), &tool_ctx);

while let Some(turn) = steps.next().await {
    match turn {
        TurnResult::ToolsExecuted { calls, results } => {
            println!("Executed {} tools", calls.len());
            // Optionally inject guidance between turns
            steps.inject_message(Message::user("Focus on budget options."));
        }
        TurnResult::FinalResponse(result) => {
            println!("Final: {}", result.response);
        }
        TurnResult::CompactionOccurred { old_tokens, new_tokens } => {
            println!("Compacted: {old_tokens} -> {new_tokens} tokens");
        }
        TurnResult::MaxTurnsReached => {
            println!("Hit turn limit");
        }
        TurnResult::Error(e) => {
            eprintln!("Error: {e}");
        }
    }
}
```

`StepIterator` exposes:

- `next()` -- advance one turn
- `messages()` -- view current conversation
- `inject_message(msg)` -- add a message between turns
- `tools_mut()` -- modify the tool registry between turns

## `AgentResult`

Returned by `run()`, `run_text()`, and `TurnResult::FinalResponse`:

```rust,ignore
pub struct AgentResult {
    pub response: String,       // Final text response from the model
    pub messages: Vec<Message>, // Full conversation history
    pub usage: TokenUsage,      // Cumulative token usage across all turns
    pub turns: usize,           // Number of turns completed
}
```

## Loop lifecycle

Each iteration of the loop follows this sequence:

1. **Check cancellation** -- if `tool_ctx.cancellation_token` is cancelled,
   return `LoopError::Cancelled`
2. **Check max turns** -- if the turn limit is reached, return
   `LoopError::MaxTurns`
3. **Fire `LoopIteration` hooks**
4. **Check context compaction** -- call `context.should_compact()` and
   `context.compact()` if needed
5. **Build `CompletionRequest`** from current messages, system prompt, and tool
   definitions
6. **Fire `PreLlmCall` hooks**
7. **Call the provider** (or durable context if set)
8. **Fire `PostLlmCall` hooks**
9. **Accumulate token usage**
10. **Check stop reason**:
    - `StopReason::Compaction` -- append message and continue the loop
    - `StopReason::EndTurn` or no tool calls -- extract text and return
      `AgentResult`
    - `StopReason::ToolUse` -- proceed to tool execution
11. **Check cancellation** again before tool execution
12. **Execute tool calls** (parallel or sequential), firing `PreToolExecution`
    and `PostToolExecution` hooks for each
13. **Append tool results** as a user message and loop back to step 1

## Cancellation

The loop checks `ToolContext.cancellation_token` at two points:

1. Top of each iteration (before the max turns check)
2. Before tool execution (after the LLM returns tool calls)

```rust,ignore
use tokio_util::sync::CancellationToken;

let token = CancellationToken::new();
let ctx = ToolContext {
    cancellation_token: token.clone(),
    ..Default::default()
};

// Cancel from another task
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(30)).await;
    token.cancel();
});

match agent.run_text("Long task...", &ctx).await {
    Err(LoopError::Cancelled) => println!("Cancelled!"),
    Ok(result) => println!("{}", result.response),
    Err(e) => eprintln!("{e}"),
}
```

## Parallel tool execution

When `LoopConfig.parallel_tool_execution` is `true` and the LLM returns multiple
tool calls in a single response, all calls execute concurrently via
`futures::future::join_all`. When `false` (the default), tools execute
sequentially in order.

```rust,ignore
let agent = AgentLoop::builder(provider, context)
    .parallel_tool_execution(true)
    .tools(registry)
    .build();
```

Parallel execution applies to `run()` and `run_step()`. Streaming (`run_stream()`)
always executes tools sequentially.

## Context compaction

The loop supports two independent compaction mechanisms:

### Client-side compaction

Uses the `ContextStrategy` you provide. Between turns, the loop calls
`should_compact()` and `compact()` to reduce message history when tokens exceed
the configured threshold.

```rust,ignore
// SlidingWindow compacts by dropping old messages
let agent = AgentLoop::builder(provider, SlidingWindowStrategy::new(20, 100_000))
    .build();
```

### Server-side compaction

When the provider returns `StopReason::Compaction`, the loop automatically
continues without treating it as a final response. The compacted content
arrives in `ContentBlock::Compaction` within the assistant's message.

No configuration is needed in the loop -- it handles this transparently. Set
`CompletionRequest.context_management` on the provider side to enable it.

## `ToolError::ModelRetry`

When a tool returns `Err(ToolError::ModelRetry(hint))`, the loop converts it
to a `ToolOutput` with `is_error: true` and the hint as content. The model
receives the hint and can retry with corrected arguments.

This does **not** propagate as `LoopError::Tool`. The loop continues normally,
giving the model a chance to self-correct.

## Observability hooks

Add hooks to observe or control loop behavior. Hooks receive events at each step
and return `HookAction::Continue`, `HookAction::Skip`, or `HookAction::Terminate`.

```rust,ignore
use neuron_types::{ObservabilityHook, HookEvent, HookAction, HookError};

struct TokenBudgetHook { max_tokens: usize }

impl ObservabilityHook for TokenBudgetHook {
    async fn on_event(&self, event: HookEvent<'_>) -> Result<HookAction, HookError> {
        match event {
            HookEvent::PostLlmCall { response } => {
                if response.usage.output_tokens > self.max_tokens {
                    return Ok(HookAction::Terminate {
                        reason: "token budget exceeded".into(),
                    });
                }
            }
            _ => {}
        }
        Ok(HookAction::Continue)
    }
}

let agent = AgentLoop::builder(provider, context)
    .hook(TokenBudgetHook { max_tokens: 10_000 })
    .build();
```

### Hook events

| Event | Fired when | Skip/Terminate behavior |
|---|---|---|
| `LoopIteration { turn }` | Start of each turn | Terminate stops the loop |
| `PreLlmCall { request }` | Before calling the provider | Terminate stops the loop |
| `PostLlmCall { response }` | After receiving the response | Terminate stops the loop |
| `PreToolExecution { tool_name, input }` | Before each tool call | Skip returns rejection as tool result |
| `PostToolExecution { tool_name, output }` | After each tool call | Terminate stops the loop |
| `ContextCompaction { old_tokens, new_tokens }` | After context is compacted | Terminate stops the loop |

## Durable execution

For crash-recoverable agents, set a `DurableContext` on the loop. When present,
LLM calls go through `DurableContext::execute_llm_call` and tool calls go
through `DurableContext::execute_tool`, enabling journaling and replay by engines
like Temporal, Restate, or Inngest.

```rust,ignore
let agent = AgentLoop::builder(provider, context)
    .durability(my_temporal_context)
    .build();
```

The loop handles the durable/non-durable split transparently. All other behavior
(hooks, compaction, cancellation) works the same way.

## Error handling

`run()` and `run_text()` return `Result<AgentResult, LoopError>`:

| Variant | Cause |
|---|---|
| `LoopError::Provider(e)` | LLM call failed |
| `LoopError::Tool(e)` | Tool execution failed (except `ModelRetry`) |
| `LoopError::Context(e)` | Context compaction failed |
| `LoopError::MaxTurns(n)` | Turn limit reached |
| `LoopError::HookTerminated(reason)` | A hook returned `Terminate` |
| `LoopError::Cancelled` | Cancellation token was triggered |

`run_stream()` sends errors as `StreamEvent::Error` on the channel instead of
returning them as `Result`.

## API reference

- [`neuron_loop` on docs.rs](https://docs.rs/neuron-loop)
- [`AgentLoop`](https://docs.rs/neuron-loop/latest/neuron_loop/struct.AgentLoop.html)
- [`AgentLoopBuilder`](https://docs.rs/neuron-loop/latest/neuron_loop/struct.AgentLoopBuilder.html)
- [`StepIterator`](https://docs.rs/neuron-loop/latest/neuron_loop/struct.StepIterator.html)
- [`LoopConfig`](https://docs.rs/neuron-loop/latest/neuron_loop/struct.LoopConfig.html)
- [`LoopError` in `neuron_types`](https://docs.rs/neuron-types/latest/neuron_types/enum.LoopError.html)
