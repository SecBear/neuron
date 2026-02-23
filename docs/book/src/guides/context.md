# Context management

neuron-context provides strategies for keeping conversation history within token
limits. When context grows too large, a `ContextStrategy` compacts messages --
dropping old ones, clearing tool results, or summarizing via an LLM. The crate
also includes token estimation, system prompt injection, and persistent context
sections.

## Quick example

```rust,ignore
use neuron_context::{SlidingWindowStrategy, TokenCounter};
use neuron_types::{ContextStrategy, Message};

let strategy = SlidingWindowStrategy::new(
    10,       // keep the last 10 non-system messages
    100_000,  // compact when tokens exceed 100k
);

let messages = vec![
    Message::system("You are a helpful assistant."),
    Message::user("Hello"),
    Message::assistant("Hi there!"),
    // ... many more messages ...
];

let token_count = strategy.token_estimate(&messages);
if strategy.should_compact(&messages, token_count) {
    let compacted = strategy.compact(messages).await?;
    // compacted retains system messages + the last 10 non-system messages
}
```

## The `ContextStrategy` trait

All strategies implement this trait from `neuron-types`:

```rust,ignore
pub trait ContextStrategy: Send + Sync {
    /// Whether compaction should be triggered.
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool;

    /// Compact the message list to reduce token usage.
    fn compact(&self, messages: Vec<Message>) -> impl Future<Output = Result<Vec<Message>, ContextError>> + Send;

    /// Estimate the token count for a list of messages.
    fn token_estimate(&self, messages: &[Message]) -> usize;
}
```

The agentic loop (`AgentLoop`) calls these methods between turns:

1. `token_estimate()` to get the current count
2. `should_compact()` to decide if action is needed
3. `compact()` to reduce the message list

## Built-in strategies

### `SlidingWindowStrategy`

Keeps system messages plus the most recent N non-system messages. Simple and
predictable -- older messages are dropped entirely.

```rust,ignore
use neuron_context::SlidingWindowStrategy;

// Keep last 20 non-system messages, trigger at 100k tokens
let strategy = SlidingWindowStrategy::new(20, 100_000);

// With a custom token counter (e.g. different chars-per-token ratio)
let counter = TokenCounter::with_ratio(3.5);
let strategy = SlidingWindowStrategy::with_counter(20, 100_000, counter);
```

**What compaction actually does.** `SlidingWindowStrategy` partitions messages
by role: system messages are always preserved regardless of the window size,
and the window count applies only to non-system messages. Here is a concrete
before/after showing a compaction with `SlidingWindowStrategy::new(2, 500)`:

```text
Before compaction (7 messages, ~800 tokens):
  [system] "You are a helpful assistant."
  [user]   "What is Rust?"
  [asst]   "Rust is a systems programming language..."
  [user]   "How about memory safety?"
  [asst]   "Rust uses ownership and borrowing..."
  [user]   "What about async?"
  [asst]   "Rust supports async/await via futures..."

After compaction with SlidingWindowStrategy::new(2, 500):
  [system] "You are a helpful assistant."    <- always preserved
  [user]   "What about async?"               <- last 2 non-system messages
  [asst]   "Rust supports async/await..."    <- last 2 non-system messages
```

The first four non-system messages are dropped entirely. The system message
survives because the implementation unconditionally retains all system messages
before applying the sliding window to the remaining conversation. See
[`neuron-context/examples/compaction.rs`](https://github.com/DarkBear0/neuron/blob/main/neuron-context/examples/compaction.rs)
for a runnable demo.

### `ToolResultClearingStrategy`

Replaces old tool result content with `"[tool result cleared]"` while preserving
the `tool_use_id` so the conversation still makes semantic sense. Keeps the most
recent N tool results intact.

This is effective when tool outputs are large (file contents, API responses) but
the model only needs the recent ones to stay coherent.

```rust,ignore
use neuron_context::ToolResultClearingStrategy;

// Keep the 2 most recent tool results intact, clear older ones
let strategy = ToolResultClearingStrategy::new(2, 100_000);
```

### `SummarizationStrategy`

Uses an LLM provider to summarize old messages, replacing them with a single
summary message. Preserves the most recent N messages verbatim.

This produces the highest-quality compaction but costs an additional LLM call.

```rust,ignore
use neuron_context::SummarizationStrategy;

// Summarize old messages, keep the 5 most recent verbatim
let strategy = SummarizationStrategy::new(provider, 5, 100_000);
```

The summarization prompt asks the LLM to summarize concisely, focusing on key
information, decisions made, and tool call results. The summary is wrapped in a
`[Summary of earlier conversation]` prefix.

### `CompositeStrategy`

Chains multiple strategies in order, applying each one until the token budget
is met. After each strategy runs, the token count is re-estimated; iteration
stops early if below the threshold.

Because `ContextStrategy` uses RPITIT (not dyn-compatible), strategies must be
wrapped in `BoxedStrategy` before composing:

```rust,ignore
use neuron_context::{
    CompositeStrategy, SlidingWindowStrategy, ToolResultClearingStrategy,
    strategies::BoxedStrategy,
};

let strategy = CompositeStrategy::new(vec![
    // First: clear old tool results (cheap, often sufficient)
    BoxedStrategy::new(ToolResultClearingStrategy::new(2, 100_000)),
    // Second: drop old messages if still over budget
    BoxedStrategy::new(SlidingWindowStrategy::new(10, 100_000)),
], 100_000);
```

This ordering is a best practice: try cheaper strategies first (clearing tool
results), then progressively more aggressive ones (dropping messages, summarizing).

## `TokenCounter`

A heuristic token estimator using a configurable characters-per-token ratio.
The default ratio of 4.0 characters per token approximates GPT-family and Claude
models.

```rust,ignore
use neuron_context::TokenCounter;

let counter = TokenCounter::new();          // 4.0 chars/token (default)
let counter = TokenCounter::with_ratio(3.5); // Custom ratio

// Estimate tokens for plain text
let tokens = counter.estimate_text("Hello, world!");

// Estimate tokens for a message list
let tokens = counter.estimate_messages(&messages);

// Estimate tokens for tool definitions
let tokens = counter.estimate_tools(&tool_definitions);
```

The counter estimates different content block types:

| Content type | Estimation method |
|---|---|
| `Text` | `len / chars_per_token` |
| `Thinking` | Thinking text length |
| `ToolUse` | Name + serialized input |
| `ToolResult` | Sum of content items |
| `Image` | Fixed 300 tokens |
| `Document` | Fixed 500 tokens |
| `Compaction` | Content text length |

Each message adds a fixed 4-token overhead for role markers.

## `SystemInjector`

Injects additional system prompt content based on turn count or token thresholds.
Useful for reminders ("be concise") or context-aware instructions that only
apply under certain conditions.

```rust,ignore
use neuron_context::{SystemInjector, InjectionTrigger};

let mut injector = SystemInjector::new();

// Remind the model to be concise every 5 turns
injector.add_rule(
    InjectionTrigger::EveryNTurns(5),
    "Reminder: keep responses concise.".into(),
);

// Warn when context is getting large
injector.add_rule(
    InjectionTrigger::OnTokenThreshold(50_000),
    "Context is getting long. Summarize when possible.".into(),
);

// Check each turn -- returns a Vec of triggered content strings
let injections: Vec<String> = injector.check(turn_number, token_count);
```

## `PersistentContext`

Aggregates named context sections and renders them into a single structured
string. Use this to build system prompts from multiple independent sources
(role definition, rules, domain knowledge) with explicit ordering.

```rust,ignore
use neuron_context::{PersistentContext, ContextSection};

let mut ctx = PersistentContext::new();
ctx.add_section(ContextSection {
    label: "Role".into(),
    content: "You are a senior Rust engineer.".into(),
    priority: 0,  // lower = rendered first
});
ctx.add_section(ContextSection {
    label: "Output rules".into(),
    content: "Always include code examples.".into(),
    priority: 10,
});

let system_prompt = ctx.render();
// ## Role
// You are a senior Rust engineer.
//
// ## Output rules
// Always include code examples.
```

## Server-side context management

Some providers (Anthropic) support server-side context compaction. Instead of
the client compacting messages, the server pauses generation, compacts context
internally, and resumes.

neuron supports this via three types in `neuron-types`:

- **`ContextManagement`** -- configuration sent in `CompletionRequest`
  to enable server-side compaction.
- **`ContentBlock::Compaction`** -- a content block containing the compacted
  summary, emitted by the server.
- **`StopReason::Compaction`** -- signals that the server paused to compact.
  The agentic loop automatically continues when it sees this stop reason.

```rust,ignore
use neuron_types::{CompletionRequest, ContextManagement, ContextEdit};

let request = CompletionRequest {
    context_management: Some(ContextManagement {
        edits: vec![ContextEdit::Compact {
            strategy: "compact_20260112".into(),
        }],
    }),
    ..Default::default()
};
```

When `AgentLoop` receives `StopReason::Compaction`, it appends the assistant's
message (which may contain `ContentBlock::Compaction`) and loops again without
treating it as a final response.

## Choosing a strategy

| Strategy | Token cost | Quality | Best for |
|---|---|---|---|
| `SlidingWindowStrategy` | None | Low (drops context) | Short conversations, prototyping |
| `ToolResultClearingStrategy` | None | Medium (preserves flow) | Tool-heavy agents with large outputs |
| `SummarizationStrategy` | 1 LLM call | High (semantic summary) | Long conversations needing continuity |
| `CompositeStrategy` | Varies | High (layered) | Production agents with mixed workloads |
| Server-side compaction | Provider-managed | Provider-dependent | Anthropic users who prefer server management |

## API reference

- [`neuron_context` on docs.rs](https://docs.rs/neuron-context)
- [`ContextStrategy` trait in `neuron_types`](https://docs.rs/neuron-types/latest/neuron_types/trait.ContextStrategy.html)
