# neuron-context

Context management crate for the neuron ecosystem. Provides token estimation,
context compaction strategies, persistent context sections, and system prompt
injection. These are the building blocks that keep an agent's conversation
within token limits without losing critical information.

## Key Types

- `TokenCounter` -- heuristic token estimator using a configurable chars-per-token ratio (default 4.0)
- `SlidingWindowStrategy` -- keeps system messages plus the last N non-system messages
- `ToolResultClearingStrategy` -- replaces old tool results with `[tool result cleared]` placeholders
- `SummarizationStrategy<P: Provider>` -- summarizes old messages via an LLM provider
- `CompositeStrategy` -- chains multiple strategies in order using `BoxedStrategy` for type erasure
- `PersistentContext` -- manages named `ContextSection` entries that persist across compaction
- `SystemInjector` -- injects system reminders at configurable triggers (turn count, token threshold)

## Strategies

All strategies implement the `ContextStrategy` trait from `neuron-types`:

| Strategy | Mechanism | Use case |
|---|---|---|
| `SlidingWindowStrategy` | Drop oldest non-system messages | Simple agents with short context needs |
| `ToolResultClearingStrategy` | Replace old tool outputs with placeholders | Tool-heavy agents where results grow large |
| `SummarizationStrategy` | LLM-powered summarization of old messages | Long-running agents needing full context awareness |
| `CompositeStrategy` | Apply strategies in sequence | Combine clearing + sliding window, etc. |

## Usage

```rust,no_run
use neuron_context::{TokenCounter, SlidingWindowStrategy, ToolResultClearingStrategy};
use neuron_types::{ContextStrategy, Message, Role, ContentBlock};

// Token estimation
let counter = TokenCounter::new(); // 4.0 chars/token default
let tokens = counter.estimate_text("Hello, world!");

let custom = TokenCounter::with_ratio(3.5); // adjust for specific models

// Sliding window: keep last 20 messages, compact above 100k tokens
let strategy = SlidingWindowStrategy::new(20, 100_000);

let messages = vec![
    Message { role: Role::User, content: vec![ContentBlock::Text("Hi".into())] },
    Message { role: Role::Assistant, content: vec![ContentBlock::Text("Hello!".into())] },
];

let token_count = strategy.token_estimate(&messages);
if strategy.should_compact(&messages, token_count) {
    // let compacted = strategy.compact(messages).await?;
}

// Tool result clearing: keep 3 most recent tool results, compact above 80k tokens
let clearing = ToolResultClearingStrategy::new(3, 80_000);
```

For `SummarizationStrategy`, pass an LLM provider to summarize old messages:

```rust,ignore
use neuron_context::SummarizationStrategy;

let strategy = SummarizationStrategy::new(provider, 5, 100_000);
// Keeps 5 most recent messages verbatim, summarizes everything older
```

## Part of neuron

This crate is part of [neuron](https://github.com/empathic-ai/neuron), a
composable building-blocks library for AI agents in Rust.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
