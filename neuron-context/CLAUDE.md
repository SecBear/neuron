# neuron-context

Context management strategies for long-running agents -- token counting,
compaction, injection, and persistent context.

## Key types

- `TokenCounter` -- heuristic token estimator using chars-per-token ratio
  (default 4.0). Estimates messages, tool definitions, and raw text.
- `InjectionTrigger` -- enum (`EveryNTurns`, `OnTokenThreshold`) controlling
  when a `SystemInjector` rule fires.
- `SystemInjector` -- injects system prompt content based on turn count or
  token thresholds. Rule-based, checked each turn.
- `ContextSection` -- a named, prioritized text section (label, content,
  priority) for structured system prompts.
- `PersistentContext` -- aggregates `ContextSection`s and renders them sorted
  by priority into a single string.
- `SlidingWindowStrategy` -- `ContextStrategy` impl that keeps system messages
  plus the last N non-system messages. Triggers on token threshold.
- `SummarizationStrategy<P: Provider>` -- `ContextStrategy` impl that
  summarizes old messages via an LLM provider, preserving recent messages
  verbatim. Generic over `Provider`.
- `ToolResultClearingStrategy` -- `ContextStrategy` impl that replaces old
  tool result content with `[tool result cleared]`, keeping the most recent N
  intact.
- `CompositeStrategy` -- chains multiple strategies in order, stopping early
  when the token count drops below the threshold.
- `BoxedStrategy` -- type-erased wrapper around any `ContextStrategy` for use
  in `CompositeStrategy`. Uses `Arc<dyn ErasedStrategy>` internally because
  `ContextStrategy` is RPITIT and not dyn-compatible.

## Key design decisions

- **Token counting is heuristic.** `TokenCounter` uses a chars-per-token ratio,
  not a real tokenizer. Good enough for compaction thresholds; avoids a
  tokenizer dependency.
- **All strategies implement `ContextStrategy`** from `neuron-types`. The loop
  calls `should_compact()` / `compact()` / `token_estimate()` uniformly.
- **`BoxedStrategy` exists for type erasure.** `ContextStrategy` uses RPITIT
  (`compact` returns `impl Future`), making it not dyn-compatible.
  `BoxedStrategy` wraps an internal `ErasedStrategy` trait that boxes the
  future, enabling heterogeneous strategy collections in `CompositeStrategy`.
- **`SummarizationStrategy` is the only strategy with a `Provider` dependency.**
  It takes a generic `P: Provider` and calls `complete()` to summarize old
  messages. All other strategies are pure computation.
- **Fixed cost estimates for images (300) and documents (500).** These are rough
  approximations in `TokenCounter` since binary content has no char-based
  estimate.

## Dependencies

- `neuron-types` -- `ContextStrategy` trait, `Message`, `ContentBlock`,
  `Provider`, `ContextError`
- `serde`, `serde_json` -- serialization
- `tracing` -- instrumentation

Dev-only: `tokio`, `proptest`, `criterion`

## Structure

```
neuron-context/
    CLAUDE.md
    Cargo.toml
    src/
        lib.rs           # Re-exports all public types
        counter.rs       # TokenCounter
        injector.rs      # InjectionTrigger, SystemInjector
        persistent.rs    # ContextSection, PersistentContext
        strategies.rs    # SlidingWindowStrategy, SummarizationStrategy,
                         # ToolResultClearingStrategy, CompositeStrategy,
                         # BoxedStrategy
    benches/
        compaction.rs    # Criterion benchmarks
```
