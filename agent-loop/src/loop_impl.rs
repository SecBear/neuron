//! Core AgentLoop struct and run methods.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use agent_tool::ToolRegistry;
use agent_types::{
    ActivityOptions, CompletionRequest, CompletionResponse, ContentBlock, ContentItem,
    ContextStrategy, DurableContext, DurableError, HookAction, HookError, HookEvent, LoopError,
    Message, ObservabilityHook, Provider, ProviderError, Role, StopReason, TokenUsage, ToolContext,
    ToolError, ToolOutput,
};

use crate::config::LoopConfig;

// --- Type erasure for ObservabilityHook (RPITIT is not dyn-compatible) ---

/// Type alias for a pinned, boxed, Send future returning a HookAction.
type HookFuture<'a> = Pin<Box<dyn Future<Output = Result<HookAction, HookError>> + Send + 'a>>;

/// Dyn-compatible wrapper for [`ObservabilityHook`].
trait ErasedHook: Send + Sync {
    fn erased_on_event<'a>(&'a self, event: HookEvent<'a>) -> HookFuture<'a>;
}

impl<H: ObservabilityHook> ErasedHook for H {
    fn erased_on_event<'a>(&'a self, event: HookEvent<'a>) -> HookFuture<'a> {
        Box::pin(self.on_event(event))
    }
}

/// A type-erased observability hook for use in [`AgentLoop`].
///
/// Wraps any [`ObservabilityHook`] into a dyn-compatible form.
pub struct BoxedHook(Arc<dyn ErasedHook>);

impl BoxedHook {
    /// Wrap any [`ObservabilityHook`] into a type-erased `BoxedHook`.
    #[must_use]
    pub fn new<H: ObservabilityHook + 'static>(hook: H) -> Self {
        BoxedHook(Arc::new(hook))
    }

    /// Fire this hook with an event.
    async fn fire(&self, event: HookEvent<'_>) -> Result<HookAction, HookError> {
        self.0.erased_on_event(event).await
    }
}

// --- Type erasure for DurableContext (RPITIT is not dyn-compatible) ---

/// Type alias for durable LLM call future.
type DurableLlmFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CompletionResponse, DurableError>> + Send + 'a>>;

/// Type alias for durable tool call future.
type DurableToolFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ToolOutput, DurableError>> + Send + 'a>>;

/// Dyn-compatible wrapper for [`DurableContext`].
pub(crate) trait ErasedDurable: Send + Sync {
    fn erased_execute_llm_call(
        &self,
        request: CompletionRequest,
        options: ActivityOptions,
    ) -> DurableLlmFuture<'_>;

    fn erased_execute_tool<'a>(
        &'a self,
        tool_name: &'a str,
        input: serde_json::Value,
        ctx: &'a ToolContext,
        options: ActivityOptions,
    ) -> DurableToolFuture<'a>;
}

impl<D: DurableContext> ErasedDurable for D {
    fn erased_execute_llm_call(
        &self,
        request: CompletionRequest,
        options: ActivityOptions,
    ) -> DurableLlmFuture<'_> {
        Box::pin(self.execute_llm_call(request, options))
    }

    fn erased_execute_tool<'a>(
        &'a self,
        tool_name: &'a str,
        input: serde_json::Value,
        ctx: &'a ToolContext,
        options: ActivityOptions,
    ) -> DurableToolFuture<'a> {
        Box::pin(self.execute_tool(tool_name, input, ctx, options))
    }
}

/// A type-erased durable context for use in [`AgentLoop`].
///
/// Wraps any [`DurableContext`] into a dyn-compatible form.
pub struct BoxedDurable(pub(crate) Arc<dyn ErasedDurable>);

impl BoxedDurable {
    /// Wrap any [`DurableContext`] into a type-erased `BoxedDurable`.
    #[must_use]
    pub fn new<D: DurableContext + 'static>(durable: D) -> Self {
        BoxedDurable(Arc::new(durable))
    }
}

// --- AgentResult ---

/// The result of a completed agent loop run.
#[derive(Debug)]
pub struct AgentResult {
    /// The final text response from the model.
    pub response: String,
    /// All messages in the conversation (including tool calls/results).
    pub messages: Vec<Message>,
    /// Cumulative token usage across all turns.
    pub usage: TokenUsage,
    /// Number of turns completed.
    pub turns: usize,
}

// --- AgentLoop ---

/// Default activity timeout for durable execution.
pub(crate) const DEFAULT_ACTIVITY_TIMEOUT: Duration = Duration::from_secs(120);

/// The agentic while loop: drives provider + tool + context interactions.
///
/// Generic over `P: Provider` (the LLM backend) and `C: ContextStrategy`
/// (the compaction strategy). Hooks and durability are optional.
pub struct AgentLoop<P: Provider, C: ContextStrategy> {
    pub(crate) provider: P,
    pub(crate) tools: ToolRegistry,
    pub(crate) context: C,
    pub(crate) hooks: Vec<BoxedHook>,
    pub(crate) durability: Option<BoxedDurable>,
    pub(crate) config: LoopConfig,
    pub(crate) messages: Vec<Message>,
}

impl<P: Provider, C: ContextStrategy> AgentLoop<P, C> {
    /// Create a new `AgentLoop` with the given provider, tools, context strategy,
    /// and configuration.
    #[must_use]
    pub fn new(provider: P, tools: ToolRegistry, context: C, config: LoopConfig) -> Self {
        Self {
            provider,
            tools,
            context,
            hooks: Vec::new(),
            durability: None,
            config,
            messages: Vec::new(),
        }
    }

    /// Add an observability hook to the loop.
    ///
    /// Hooks are called in order of registration at each event point.
    pub fn add_hook<H: ObservabilityHook + 'static>(&mut self, hook: H) -> &mut Self {
        self.hooks.push(BoxedHook::new(hook));
        self
    }

    /// Set the durable context for crash-recoverable execution.
    ///
    /// When set, LLM calls and tool executions go through the durable context
    /// so they can be journaled, replayed, and recovered by engines like
    /// Temporal, Restate, or Inngest.
    pub fn set_durability<D: DurableContext + 'static>(&mut self, durable: D) -> &mut Self {
        self.durability = Some(BoxedDurable::new(durable));
        self
    }

    /// Returns a reference to the current configuration.
    #[must_use]
    pub fn config(&self) -> &LoopConfig {
        &self.config
    }

    /// Returns a reference to the current messages.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Returns a mutable reference to the tool registry.
    #[must_use]
    pub fn tools_mut(&mut self) -> &mut ToolRegistry {
        &mut self.tools
    }

    /// Run the agentic loop to completion.
    ///
    /// Appends the user message, then loops: call provider, execute tools if
    /// needed, append results, repeat until the model returns a text-only
    /// response or the turn limit is reached.
    ///
    /// When durability is set, LLM calls go through
    /// [`DurableContext::execute_llm_call`] and tool calls go through
    /// [`DurableContext::execute_tool`].
    ///
    /// Fires [`HookEvent`] at each step. If a hook returns
    /// [`HookAction::Terminate`], the loop stops with
    /// [`LoopError::HookTerminated`].
    ///
    /// # Errors
    ///
    /// Returns `LoopError::MaxTurns` if the turn limit is exceeded,
    /// `LoopError::Provider` on provider failures, `LoopError::Tool`
    /// on tool execution failures, or `LoopError::HookTerminated` if
    /// a hook requests termination.
    #[must_use = "this returns a Result that should be handled"]
    pub async fn run(
        &mut self,
        user_message: Message,
        tool_ctx: &ToolContext,
    ) -> Result<AgentResult, LoopError> {
        self.messages.push(user_message);

        let mut total_usage = TokenUsage::default();
        let mut turns: usize = 0;

        loop {
            // Check max turns
            if let Some(max) = self.config.max_turns
                && turns >= max
            {
                return Err(LoopError::MaxTurns(max));
            }

            // Fire LoopIteration hooks
            if let Some(HookAction::Terminate { reason }) =
                fire_loop_iteration_hooks(&self.hooks, turns).await?
            {
                return Err(LoopError::HookTerminated(reason));
            }

            // Check context compaction
            let token_count = self.context.token_estimate(&self.messages);
            if self.context.should_compact(&self.messages, token_count) {
                let old_tokens = token_count;
                self.messages = self.context.compact(self.messages.clone()).await?;
                let new_tokens = self.context.token_estimate(&self.messages);

                // Fire ContextCompaction hooks
                if let Some(HookAction::Terminate { reason }) =
                    fire_compaction_hooks(&self.hooks, old_tokens, new_tokens).await?
                {
                    return Err(LoopError::HookTerminated(reason));
                }
            }

            // Build completion request
            let request = CompletionRequest {
                model: String::new(), // Provider decides the model
                messages: self.messages.clone(),
                system: Some(self.config.system_prompt.clone()),
                tools: self.tools.definitions(),
                max_tokens: None,
                temperature: None,
                top_p: None,
                stop_sequences: vec![],
                tool_choice: None,
                response_format: None,
                thinking: None,
                reasoning_effort: None,
                extra: None,
            };

            // Fire PreLlmCall hooks
            if let Some(HookAction::Terminate { reason }) =
                fire_pre_llm_hooks(&self.hooks, &request).await?
            {
                return Err(LoopError::HookTerminated(reason));
            }

            // Call provider (via durability wrapper if present)
            let response = if let Some(ref durable) = self.durability {
                let options = ActivityOptions {
                    start_to_close_timeout: DEFAULT_ACTIVITY_TIMEOUT,
                    heartbeat_timeout: None,
                    retry_policy: None,
                };
                durable
                    .0
                    .erased_execute_llm_call(request, options)
                    .await
                    .map_err(|e| ProviderError::Other(Box::new(e)))?
            } else {
                self.provider.complete(request).await?
            };

            // Fire PostLlmCall hooks
            if let Some(HookAction::Terminate { reason }) =
                fire_post_llm_hooks(&self.hooks, &response).await?
            {
                return Err(LoopError::HookTerminated(reason));
            }

            // Accumulate usage
            accumulate_usage(&mut total_usage, &response.usage);
            turns += 1;

            // Check for tool calls in the response
            let tool_calls: Vec<_> = response
                .message
                .content
                .iter()
                .filter_map(|block| {
                    if let ContentBlock::ToolUse { id, name, input } = block {
                        Some((id.clone(), name.clone(), input.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            // Append assistant message to conversation
            self.messages.push(response.message.clone());

            if tool_calls.is_empty() || response.stop_reason == StopReason::EndTurn {
                // No tool calls — extract text and return
                let response_text = extract_text(&response.message);
                return Ok(AgentResult {
                    response: response_text,
                    messages: self.messages.clone(),
                    usage: total_usage,
                    turns,
                });
            }

            // Execute tool calls and collect results
            let mut tool_result_blocks = Vec::new();
            for (call_id, tool_name, input) in &tool_calls {
                // Fire PreToolExecution hooks
                if let Some(action) =
                    fire_pre_tool_hooks(&self.hooks, tool_name, input).await?
                {
                    match action {
                        HookAction::Terminate { reason } => {
                            return Err(LoopError::HookTerminated(reason));
                        }
                        HookAction::Skip { reason } => {
                            // Skip the tool call and return a rejection message
                            tool_result_blocks.push(ContentBlock::ToolResult {
                                tool_use_id: call_id.clone(),
                                content: vec![ContentItem::Text(format!(
                                    "Tool call skipped: {reason}"
                                ))],
                                is_error: true,
                            });
                            continue;
                        }
                        HookAction::Continue => {}
                    }
                }

                // Execute tool (via durability wrapper if present)
                let result = if let Some(ref durable) = self.durability {
                    let options = ActivityOptions {
                        start_to_close_timeout: DEFAULT_ACTIVITY_TIMEOUT,
                        heartbeat_timeout: None,
                        retry_policy: None,
                    };
                    durable
                        .0
                        .erased_execute_tool(tool_name, input.clone(), tool_ctx, options)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed(Box::new(e)))?
                } else {
                    self.tools.execute(tool_name, input.clone(), tool_ctx).await?
                };

                // Fire PostToolExecution hooks
                if let Some(HookAction::Terminate { reason }) =
                    fire_post_tool_hooks(&self.hooks, tool_name, &result).await?
                {
                    return Err(LoopError::HookTerminated(reason));
                }

                tool_result_blocks.push(ContentBlock::ToolResult {
                    tool_use_id: call_id.clone(),
                    content: result.content,
                    is_error: result.is_error,
                });
            }

            // Append tool results as a user message
            self.messages.push(Message {
                role: Role::User,
                content: tool_result_blocks,
            });
        }
    }
}

// --- Hook firing helpers ---

/// Fire all hooks for a PreLlmCall event, returning the first non-Continue action.
pub(crate) async fn fire_pre_llm_hooks(
    hooks: &[BoxedHook],
    request: &CompletionRequest,
) -> Result<Option<HookAction>, LoopError> {
    for hook in hooks {
        let action = hook
            .fire(HookEvent::PreLlmCall { request })
            .await
            .map_err(|e| LoopError::HookTerminated(e.to_string()))?;
        if !matches!(action, HookAction::Continue) {
            return Ok(Some(action));
        }
    }
    Ok(None)
}

/// Fire all hooks for a PostLlmCall event, returning the first non-Continue action.
pub(crate) async fn fire_post_llm_hooks(
    hooks: &[BoxedHook],
    response: &CompletionResponse,
) -> Result<Option<HookAction>, LoopError> {
    for hook in hooks {
        let action = hook
            .fire(HookEvent::PostLlmCall { response })
            .await
            .map_err(|e| LoopError::HookTerminated(e.to_string()))?;
        if !matches!(action, HookAction::Continue) {
            return Ok(Some(action));
        }
    }
    Ok(None)
}

/// Fire all hooks for a PreToolExecution event, returning the first non-Continue action.
pub(crate) async fn fire_pre_tool_hooks(
    hooks: &[BoxedHook],
    tool_name: &str,
    input: &serde_json::Value,
) -> Result<Option<HookAction>, LoopError> {
    for hook in hooks {
        let action = hook
            .fire(HookEvent::PreToolExecution { tool_name, input })
            .await
            .map_err(|e| LoopError::HookTerminated(e.to_string()))?;
        if !matches!(action, HookAction::Continue) {
            return Ok(Some(action));
        }
    }
    Ok(None)
}

/// Fire all hooks for a PostToolExecution event, returning the first non-Continue action.
pub(crate) async fn fire_post_tool_hooks(
    hooks: &[BoxedHook],
    tool_name: &str,
    output: &ToolOutput,
) -> Result<Option<HookAction>, LoopError> {
    for hook in hooks {
        let action = hook
            .fire(HookEvent::PostToolExecution { tool_name, output })
            .await
            .map_err(|e| LoopError::HookTerminated(e.to_string()))?;
        if !matches!(action, HookAction::Continue) {
            return Ok(Some(action));
        }
    }
    Ok(None)
}

/// Fire all hooks for a LoopIteration event, returning the first non-Continue action.
pub(crate) async fn fire_loop_iteration_hooks(
    hooks: &[BoxedHook],
    turn: usize,
) -> Result<Option<HookAction>, LoopError> {
    for hook in hooks {
        let action = hook
            .fire(HookEvent::LoopIteration { turn })
            .await
            .map_err(|e| LoopError::HookTerminated(e.to_string()))?;
        if !matches!(action, HookAction::Continue) {
            return Ok(Some(action));
        }
    }
    Ok(None)
}

/// Fire all hooks for a ContextCompaction event, returning the first non-Continue action.
pub(crate) async fn fire_compaction_hooks(
    hooks: &[BoxedHook],
    old_tokens: usize,
    new_tokens: usize,
) -> Result<Option<HookAction>, LoopError> {
    for hook in hooks {
        let action = hook
            .fire(HookEvent::ContextCompaction {
                old_tokens,
                new_tokens,
            })
            .await
            .map_err(|e| LoopError::HookTerminated(e.to_string()))?;
        if !matches!(action, HookAction::Continue) {
            return Ok(Some(action));
        }
    }
    Ok(None)
}

// --- Utility functions ---

/// Extract text content from a message.
pub(crate) fn extract_text(message: &Message) -> String {
    message
        .content
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(text) = block {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Accumulate token usage from a response into the total.
pub(crate) fn accumulate_usage(total: &mut TokenUsage, delta: &TokenUsage) {
    total.input_tokens += delta.input_tokens;
    total.output_tokens += delta.output_tokens;
    if let Some(cache_read) = delta.cache_read_tokens {
        *total.cache_read_tokens.get_or_insert(0) += cache_read;
    }
    if let Some(cache_creation) = delta.cache_creation_tokens {
        *total.cache_creation_tokens.get_or_insert(0) += cache_creation;
    }
    if let Some(reasoning) = delta.reasoning_tokens {
        *total.reasoning_tokens.get_or_insert(0) += reasoning;
    }
}
