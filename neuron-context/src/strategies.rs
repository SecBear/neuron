//! Context compaction strategies implementing [`ContextStrategy`].

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use neuron_types::{ContextError, ContextStrategy, Message, Provider, Role};

use crate::counter::TokenCounter;

// ---- Dyn-compatible wrapper for CompositeStrategy --------------------------

/// Type alias for a pinned, boxed, `Send` future returning compacted messages.
type CompactFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<Message>, ContextError>> + Send + 'a>>;

/// A dyn-compatible strategy object. Used internally by [`CompositeStrategy`].
///
/// Because `ContextStrategy::compact` returns `impl Future` (RPITIT), the trait
/// is not dyn-compatible. `ErasedStrategy` provides a vtable-friendly equivalent
/// that boxes the future.
trait ErasedStrategy: Send + Sync {
    fn erased_compact<'a>(&'a self, messages: Vec<Message>) -> CompactFuture<'a>;
    fn erased_token_estimate(&self, messages: &[Message]) -> usize;
    fn erased_should_compact(&self, messages: &[Message], token_count: usize) -> bool;
}

impl<S: ContextStrategy> ErasedStrategy for S {
    fn erased_compact<'a>(&'a self, messages: Vec<Message>) -> CompactFuture<'a> {
        Box::pin(self.compact(messages))
    }

    fn erased_token_estimate(&self, messages: &[Message]) -> usize {
        self.token_estimate(messages)
    }

    fn erased_should_compact(&self, messages: &[Message], token_count: usize) -> bool {
        self.should_compact(messages, token_count)
    }
}

/// A type-erased wrapper around a [`ContextStrategy`] for use in [`CompositeStrategy`].
///
/// Use `BoxedStrategy::new(strategy)` to wrap any strategy.
///
/// # Example
///
/// ```
/// use neuron_context::{SlidingWindowStrategy, strategies::BoxedStrategy};
///
/// let boxed = BoxedStrategy::new(SlidingWindowStrategy::new(10, 100_000));
/// ```
pub struct BoxedStrategy(Arc<dyn ErasedStrategy>);

impl BoxedStrategy {
    /// Wrap any [`ContextStrategy`] into a type-erased `BoxedStrategy`.
    #[must_use]
    pub fn new<S: ContextStrategy + 'static>(strategy: S) -> Self {
        BoxedStrategy(Arc::new(strategy))
    }
}

impl ContextStrategy for BoxedStrategy {
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool {
        self.0.erased_should_compact(messages, token_count)
    }

    fn compact(
        &self,
        messages: Vec<Message>,
    ) -> impl Future<Output = Result<Vec<Message>, ContextError>> + neuron_types::WasmCompatSend {
        let inner = Arc::clone(&self.0);
        async move { inner.erased_compact(messages).await }
    }

    fn token_estimate(&self, messages: &[Message]) -> usize {
        self.0.erased_token_estimate(messages)
    }
}

// ---- SlidingWindowStrategy --------------------------------------------------

/// Keeps system messages plus the last `window_size` non-system messages.
///
/// Triggers compaction when the estimated token count exceeds `max_tokens`.
///
/// # Example
///
/// ```
/// use neuron_context::SlidingWindowStrategy;
///
/// let strategy = SlidingWindowStrategy::new(10, 100_000);
/// ```
pub struct SlidingWindowStrategy {
    window_size: usize,
    counter: TokenCounter,
    max_tokens: usize,
}

impl SlidingWindowStrategy {
    /// Creates a new `SlidingWindowStrategy`.
    ///
    /// # Arguments
    /// * `window_size` — maximum number of non-system messages to retain
    /// * `max_tokens` — token threshold above which compaction is triggered
    #[must_use]
    pub fn new(window_size: usize, max_tokens: usize) -> Self {
        Self { window_size, counter: TokenCounter::new(), max_tokens }
    }

    /// Creates a new `SlidingWindowStrategy` with a custom [`TokenCounter`].
    #[must_use]
    pub fn with_counter(window_size: usize, max_tokens: usize, counter: TokenCounter) -> Self {
        Self { window_size, counter, max_tokens }
    }
}

impl ContextStrategy for SlidingWindowStrategy {
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool {
        let _ = messages;
        token_count > self.max_tokens
    }

    fn compact(
        &self,
        messages: Vec<Message>,
    ) -> impl Future<Output = Result<Vec<Message>, ContextError>> + neuron_types::WasmCompatSend
    {
        let window_size = self.window_size;
        async move {
            let (system_msgs, non_system): (Vec<_>, Vec<_>) =
                messages.into_iter().partition(|m| m.role == Role::System);

            let recent: Vec<Message> = non_system
                .into_iter()
                .rev()
                .take(window_size)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            let mut result = system_msgs;
            result.extend(recent);
            Ok(result)
        }
    }

    fn token_estimate(&self, messages: &[Message]) -> usize {
        self.counter.estimate_messages(messages)
    }
}

// ---- ToolResultClearingStrategy ---------------------------------------------

/// Replaces old tool result content with a placeholder to reduce token usage.
///
/// Keeps the most recent `keep_recent_n` tool results intact and replaces
/// older ones with `[tool result cleared]` while preserving the `tool_use_id`
/// so the conversation still makes semantic sense.
///
/// # Example
///
/// ```
/// use neuron_context::ToolResultClearingStrategy;
///
/// let strategy = ToolResultClearingStrategy::new(2, 100_000);
/// ```
pub struct ToolResultClearingStrategy {
    keep_recent_n: usize,
    counter: TokenCounter,
    max_tokens: usize,
}

impl ToolResultClearingStrategy {
    /// Creates a new `ToolResultClearingStrategy`.
    ///
    /// # Arguments
    /// * `keep_recent_n` — number of most-recent tool results to leave untouched
    /// * `max_tokens` — token threshold above which compaction is triggered
    #[must_use]
    pub fn new(keep_recent_n: usize, max_tokens: usize) -> Self {
        Self { keep_recent_n, counter: TokenCounter::new(), max_tokens }
    }

    /// Creates a new `ToolResultClearingStrategy` with a custom [`TokenCounter`].
    #[must_use]
    pub fn with_counter(
        keep_recent_n: usize,
        max_tokens: usize,
        counter: TokenCounter,
    ) -> Self {
        Self { keep_recent_n, counter, max_tokens }
    }
}

impl ContextStrategy for ToolResultClearingStrategy {
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool {
        let _ = messages;
        token_count > self.max_tokens
    }

    fn compact(
        &self,
        messages: Vec<Message>,
    ) -> impl Future<Output = Result<Vec<Message>, ContextError>> + neuron_types::WasmCompatSend
    {
        use neuron_types::{ContentBlock, ContentItem};

        let keep_recent_n = self.keep_recent_n;
        async move {
            // Collect positions of all ToolResult blocks across all messages.
            let mut tool_result_positions: Vec<(usize, usize)> = Vec::new();
            for (msg_idx, msg) in messages.iter().enumerate() {
                for (block_idx, block) in msg.content.iter().enumerate() {
                    if matches!(block, ContentBlock::ToolResult { .. }) {
                        tool_result_positions.push((msg_idx, block_idx));
                    }
                }
            }

            let total = tool_result_positions.len();
            let to_clear_count = total.saturating_sub(keep_recent_n);

            if to_clear_count == 0 {
                return Ok(messages);
            }

            let to_clear = tool_result_positions[..to_clear_count].to_vec();
            let mut messages = messages;
            for (msg_idx, block_idx) in to_clear {
                let block = &mut messages[msg_idx].content[block_idx];
                if let ContentBlock::ToolResult { content, is_error, .. } = block {
                    *content = vec![ContentItem::Text("[tool result cleared]".to_string())];
                    *is_error = false;
                }
            }

            Ok(messages)
        }
    }

    fn token_estimate(&self, messages: &[Message]) -> usize {
        self.counter.estimate_messages(messages)
    }
}

// ---- SummarizationStrategy --------------------------------------------------

/// Summarizes old messages using an LLM provider, preserving recent messages verbatim.
///
/// When compaction is triggered, messages older than `preserve_recent` are sent
/// to the provider with a summarization prompt. The response replaces the old
/// messages with a single `User` message containing the summary, followed by
/// the preserved recent messages.
///
/// # Example
///
/// ```ignore
/// use neuron_context::SummarizationStrategy;
///
/// let strategy = SummarizationStrategy::new(provider, 5, 100_000);
/// ```
pub struct SummarizationStrategy<P: Provider> {
    provider: P,
    preserve_recent: usize,
    counter: TokenCounter,
    max_tokens: usize,
}

impl<P: Provider> SummarizationStrategy<P> {
    /// Creates a new `SummarizationStrategy`.
    ///
    /// # Arguments
    /// * `provider` — the LLM provider used for summarization
    /// * `preserve_recent` — number of most-recent messages to keep verbatim
    /// * `max_tokens` — token threshold above which compaction is triggered
    #[must_use]
    pub fn new(provider: P, preserve_recent: usize, max_tokens: usize) -> Self {
        Self { provider, preserve_recent, counter: TokenCounter::new(), max_tokens }
    }

    /// Creates a new `SummarizationStrategy` with a custom [`TokenCounter`].
    #[must_use]
    pub fn with_counter(
        provider: P,
        preserve_recent: usize,
        max_tokens: usize,
        counter: TokenCounter,
    ) -> Self {
        Self { provider, preserve_recent, counter, max_tokens }
    }
}

impl<P: Provider> ContextStrategy for SummarizationStrategy<P> {
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool {
        let _ = messages;
        token_count > self.max_tokens
    }

    fn compact(
        &self,
        messages: Vec<Message>,
    ) -> impl Future<Output = Result<Vec<Message>, ContextError>> + neuron_types::WasmCompatSend
    {
        use neuron_types::{CompletionRequest, ContentBlock, Role, SystemPrompt};

        let preserve_recent = self.preserve_recent;

        // Partition before entering the async block so we don't borrow `messages`.
        let (system_msgs, non_system): (Vec<Message>, Vec<Message>) =
            messages.into_iter().partition(|m| m.role == Role::System);

        let split_at = non_system.len().saturating_sub(preserve_recent);
        let old_messages = non_system[..split_at].to_vec();
        let recent_messages = non_system[split_at..].to_vec();

        let summarize_request = CompletionRequest {
            model: String::new(),
            messages: old_messages,
            system: Some(SystemPrompt::Text(
                "Summarize the conversation above concisely. Focus on key information, \
                 decisions made, and results from tool calls. Write in third person."
                    .to_string(),
            )),
            tools: vec![],
            max_tokens: Some(1024),
            temperature: Some(0.0),
            top_p: None,
            stop_sequences: vec![],
            tool_choice: None,
            response_format: None,
            thinking: None,
            reasoning_effort: None,
            extra: None,
        };

        async move {
            let response = self.provider.complete(summarize_request).await?;

            let summary_text = response
                .message
                .content
                .into_iter()
                .filter_map(|block| {
                    if let ContentBlock::Text(text) = block {
                        Some(text)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            let summary_message = Message {
                role: Role::User,
                content: vec![ContentBlock::Text(format!(
                    "[Summary of earlier conversation]\n{summary_text}"
                ))],
            };

            let mut result = system_msgs;
            result.push(summary_message);
            result.extend(recent_messages);
            Ok(result)
        }
    }

    fn token_estimate(&self, messages: &[Message]) -> usize {
        self.counter.estimate_messages(messages)
    }
}

// ---- CompositeStrategy ------------------------------------------------------

/// Chains multiple strategies, applying each in order until token budget is met.
///
/// Each strategy is tried in sequence. After each strategy's `compact` runs,
/// the resulting token count is re-estimated. If it falls below `max_tokens`,
/// iteration stops early.
///
/// Use [`BoxedStrategy::new`] to wrap concrete strategies before collecting them.
///
/// # Example
///
/// ```
/// use neuron_context::{CompositeStrategy, SlidingWindowStrategy, ToolResultClearingStrategy};
/// use neuron_context::strategies::BoxedStrategy;
///
/// let strategy = CompositeStrategy::new(vec![
///     BoxedStrategy::new(ToolResultClearingStrategy::new(2, 100_000)),
///     BoxedStrategy::new(SlidingWindowStrategy::new(10, 100_000)),
/// ], 100_000);
/// ```
pub struct CompositeStrategy {
    strategies: Vec<BoxedStrategy>,
    counter: TokenCounter,
    max_tokens: usize,
}

impl CompositeStrategy {
    /// Creates a new `CompositeStrategy`.
    ///
    /// # Arguments
    /// * `strategies` — ordered list of type-erased strategies to apply
    /// * `max_tokens` — token threshold above which compaction is triggered
    #[must_use]
    pub fn new(strategies: Vec<BoxedStrategy>, max_tokens: usize) -> Self {
        Self { strategies, counter: TokenCounter::new(), max_tokens }
    }
}

impl ContextStrategy for CompositeStrategy {
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool {
        let _ = messages;
        token_count > self.max_tokens
    }

    fn compact(
        &self,
        messages: Vec<Message>,
    ) -> impl Future<Output = Result<Vec<Message>, ContextError>> + neuron_types::WasmCompatSend
    {
        // Snapshot what we need before entering the async block.
        let inner_refs: Vec<Arc<dyn ErasedStrategy>> =
            self.strategies.iter().map(|b| Arc::clone(&b.0)).collect();
        let max_tokens = self.max_tokens;
        let counter = TokenCounter::new();

        async move {
            let mut current = messages;
            for strategy in &inner_refs {
                let token_count = counter.estimate_messages(&current);
                if token_count <= max_tokens {
                    break;
                }
                current = strategy.erased_compact(current).await?;
            }
            Ok(current)
        }
    }

    fn token_estimate(&self, messages: &[Message]) -> usize {
        self.counter.estimate_messages(messages)
    }
}
