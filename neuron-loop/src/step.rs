//! Step-by-step iteration types for the agentic loop.
//!
//! [`StepIterator`] lets you drive the loop one turn at a time, inspect
//! intermediate state, inject messages, and modify the tool registry
//! between turns.

use neuron_tool::ToolRegistry;
use futures::StreamExt;

use neuron_types::{
    CompletionRequest, ContentBlock, ContentItem, ContextStrategy, HookAction, LoopError, Message,
    Provider, Role, StopReason, StreamError, StreamEvent, TokenUsage, ToolContext, ToolOutput,
};

use crate::loop_impl::{
    accumulate_usage, extract_text, fire_compaction_hooks, fire_loop_iteration_hooks,
    fire_post_llm_hooks, fire_post_tool_hooks, fire_pre_llm_hooks, fire_pre_tool_hooks,
    AgentLoop, AgentResult, DEFAULT_ACTIVITY_TIMEOUT,
};

/// The result of a single turn in the agentic loop.
#[derive(Debug)]
pub enum TurnResult {
    /// Tool calls were executed and results appended.
    ToolsExecuted {
        /// The tool calls made by the model.
        calls: Vec<(String, String, serde_json::Value)>,
        /// The tool outputs.
        results: Vec<ToolOutput>,
    },
    /// The model returned a final text response.
    FinalResponse(AgentResult),
    /// Context compaction occurred.
    CompactionOccurred {
        /// Token count before compaction.
        old_tokens: usize,
        /// Token count after compaction.
        new_tokens: usize,
    },
    /// The turn limit was reached.
    MaxTurnsReached,
    /// An error occurred.
    Error(LoopError),
}

/// Step-by-step iterator over the agentic loop.
///
/// Allows driving the loop one turn at a time with full control
/// between turns: inspect messages, inject new messages, modify
/// tools.
///
/// Created via [`AgentLoop::run_step`].
pub struct StepIterator<'a, P: Provider, C: ContextStrategy> {
    loop_ref: &'a mut AgentLoop<P, C>,
    tool_ctx: &'a ToolContext,
    total_usage: TokenUsage,
    turns: usize,
    finished: bool,
}

impl<'a, P: Provider, C: ContextStrategy> StepIterator<'a, P, C> {
    /// Advance the loop by one turn.
    ///
    /// Returns `None` if the loop has already completed (final response
    /// was returned or an error occurred).
    pub async fn next(&mut self) -> Option<TurnResult> {
        if self.finished {
            return None;
        }

        // Check cancellation
        if self.tool_ctx.cancellation_token.is_cancelled() {
            self.finished = true;
            return Some(TurnResult::Error(LoopError::Cancelled));
        }

        // Check max turns
        if let Some(max) = self.loop_ref.config.max_turns
            && self.turns >= max
        {
            self.finished = true;
            return Some(TurnResult::MaxTurnsReached);
        }

        // Fire LoopIteration hooks
        match fire_loop_iteration_hooks(&self.loop_ref.hooks, self.turns).await {
            Ok(Some(HookAction::Terminate { reason })) => {
                self.finished = true;
                return Some(TurnResult::Error(LoopError::HookTerminated(reason)));
            }
            Err(e) => {
                self.finished = true;
                return Some(TurnResult::Error(e));
            }
            _ => {}
        }

        // Check context compaction
        let token_count = self.loop_ref.context.token_estimate(&self.loop_ref.messages);
        if self
            .loop_ref
            .context
            .should_compact(&self.loop_ref.messages, token_count)
        {
            let old_tokens = token_count;
            match self
                .loop_ref
                .context
                .compact(self.loop_ref.messages.clone())
                .await
            {
                Ok(compacted) => {
                    self.loop_ref.messages = compacted;
                    let new_tokens =
                        self.loop_ref.context.token_estimate(&self.loop_ref.messages);

                    // Fire compaction hooks
                    match fire_compaction_hooks(&self.loop_ref.hooks, old_tokens, new_tokens).await
                    {
                        Ok(Some(HookAction::Terminate { reason })) => {
                            self.finished = true;
                            return Some(TurnResult::Error(LoopError::HookTerminated(reason)));
                        }
                        Err(e) => {
                            self.finished = true;
                            return Some(TurnResult::Error(e));
                        }
                        _ => {}
                    }

                    return Some(TurnResult::CompactionOccurred {
                        old_tokens,
                        new_tokens,
                    });
                }
                Err(e) => {
                    self.finished = true;
                    return Some(TurnResult::Error(e.into()));
                }
            }
        }

        // Build completion request
        let request = CompletionRequest {
            model: String::new(),
            messages: self.loop_ref.messages.clone(),
            system: Some(self.loop_ref.config.system_prompt.clone()),
            tools: self.loop_ref.tools.definitions(),
            ..Default::default()
        };

        // Fire PreLlmCall hooks
        match fire_pre_llm_hooks(&self.loop_ref.hooks, &request).await {
            Ok(Some(HookAction::Terminate { reason })) => {
                self.finished = true;
                return Some(TurnResult::Error(LoopError::HookTerminated(reason)));
            }
            Err(e) => {
                self.finished = true;
                return Some(TurnResult::Error(e));
            }
            _ => {}
        }

        // Call provider (via durability if set)
        let response = if let Some(ref durable) = self.loop_ref.durability {
            let options = neuron_types::ActivityOptions {
                start_to_close_timeout: DEFAULT_ACTIVITY_TIMEOUT,
                heartbeat_timeout: None,
                retry_policy: None,
            };
            match durable.0.erased_execute_llm_call(request, options).await {
                Ok(r) => r,
                Err(e) => {
                    self.finished = true;
                    return Some(TurnResult::Error(
                        neuron_types::ProviderError::Other(Box::new(e)).into(),
                    ));
                }
            }
        } else {
            match self.loop_ref.provider.complete(request).await {
                Ok(r) => r,
                Err(e) => {
                    self.finished = true;
                    return Some(TurnResult::Error(e.into()));
                }
            }
        };

        // Fire PostLlmCall hooks
        match fire_post_llm_hooks(&self.loop_ref.hooks, &response).await {
            Ok(Some(HookAction::Terminate { reason })) => {
                self.finished = true;
                return Some(TurnResult::Error(LoopError::HookTerminated(reason)));
            }
            Err(e) => {
                self.finished = true;
                return Some(TurnResult::Error(e));
            }
            _ => {}
        }

        // Accumulate usage
        accumulate_usage(&mut self.total_usage, &response.usage);
        self.turns += 1;

        // Check for tool calls
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

        // Append assistant message
        self.loop_ref.messages.push(response.message.clone());

        // Server-side compaction: the provider paused to compact context.
        // Report as a compaction event so the caller can continue stepping.
        if response.stop_reason == StopReason::Compaction {
            return Some(TurnResult::CompactionOccurred {
                old_tokens: 0,
                new_tokens: 0,
            });
        }

        if tool_calls.is_empty() || response.stop_reason == StopReason::EndTurn {
            self.finished = true;
            let response_text = extract_text(&response.message);
            return Some(TurnResult::FinalResponse(AgentResult {
                response: response_text,
                messages: self.loop_ref.messages.clone(),
                usage: self.total_usage.clone(),
                turns: self.turns,
            }));
        }

        // Check cancellation before tool execution
        if self.tool_ctx.cancellation_token.is_cancelled() {
            self.finished = true;
            return Some(TurnResult::Error(LoopError::Cancelled));
        }

        // Execute tool calls (parallel or sequential)
        let mut tool_result_blocks = Vec::new();
        let mut tool_outputs = Vec::new();

        if self.loop_ref.config.parallel_tool_execution && tool_calls.len() > 1 {
            let futs = tool_calls.iter().map(|(call_id, tool_name, input)| {
                self.loop_ref.execute_single_tool(call_id, tool_name, input, self.tool_ctx)
            });
            let results = futures::future::join_all(futs).await;
            for result in results {
                match result {
                    Ok(block) => {
                        // Extract ToolOutput from the ContentBlock for TurnResult
                        if let ContentBlock::ToolResult { content, is_error, .. } = &block {
                            tool_outputs.push(ToolOutput {
                                content: content.clone(),
                                structured_content: None,
                                is_error: *is_error,
                            });
                        }
                        tool_result_blocks.push(block);
                    }
                    Err(e) => {
                        self.finished = true;
                        return Some(TurnResult::Error(e));
                    }
                }
            }
        } else {
            for (call_id, tool_name, input) in &tool_calls {
                match self.loop_ref.execute_single_tool(call_id, tool_name, input, self.tool_ctx).await {
                    Ok(block) => {
                        if let ContentBlock::ToolResult { content, is_error, .. } = &block {
                            tool_outputs.push(ToolOutput {
                                content: content.clone(),
                                structured_content: None,
                                is_error: *is_error,
                            });
                        }
                        tool_result_blocks.push(block);
                    }
                    Err(e) => {
                        self.finished = true;
                        return Some(TurnResult::Error(e));
                    }
                }
            }
        }

        // Append tool results
        self.loop_ref.messages.push(Message {
            role: Role::User,
            content: tool_result_blocks,
        });

        Some(TurnResult::ToolsExecuted {
            calls: tool_calls,
            results: tool_outputs,
        })
    }

    /// Returns a reference to the current messages.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.loop_ref.messages
    }

    /// Inject a message into the conversation between turns.
    pub fn inject_message(&mut self, message: Message) {
        self.loop_ref.messages.push(message);
    }

    /// Returns a mutable reference to the tool registry.
    #[must_use]
    pub fn tools_mut(&mut self) -> &mut ToolRegistry {
        &mut self.loop_ref.tools
    }
}

impl<P: Provider, C: ContextStrategy> AgentLoop<P, C> {
    /// Create a step-by-step iterator over the loop.
    ///
    /// Unlike [`run`](AgentLoop::run) which drives to completion, this
    /// lets you advance one turn at a time, inspect state, inject messages,
    /// and modify tools between turns.
    ///
    /// The user message is appended immediately. Call
    /// [`StepIterator::next`] to advance.
    #[must_use]
    pub fn run_step<'a>(
        &'a mut self,
        user_message: Message,
        tool_ctx: &'a ToolContext,
    ) -> StepIterator<'a, P, C> {
        self.messages.push(user_message);
        StepIterator {
            loop_ref: self,
            tool_ctx,
            total_usage: TokenUsage::default(),
            turns: 0,
            finished: false,
        }
    }

    /// Run the loop with streaming, forwarding [`StreamEvent`]s through a channel.
    ///
    /// Uses `provider.complete_stream()` instead of `provider.complete()` for
    /// each LLM turn. When durability is set, falls back to `DurableContext::execute_llm_call`
    /// (full response) and synthesizes stream events from the result.
    ///
    /// Tool execution is handled identically to [`run`](AgentLoop::run).
    /// Fires the same hook events as `run()`: `LoopIteration`, `PreLlmCall`,
    /// `PostLlmCall`, `PreToolExecution`, `PostToolExecution`, and
    /// `ContextCompaction`.
    ///
    /// Returns a receiver that yields `StreamEvent`s. The final
    /// `StreamEvent::MessageComplete` on the last turn signals the loop
    /// has finished.
    ///
    /// # Errors
    ///
    /// Errors are sent as `StreamEvent::Error` on the channel.
    pub async fn run_stream(
        &mut self,
        user_message: Message,
        tool_ctx: &ToolContext,
    ) -> tokio::sync::mpsc::Receiver<StreamEvent> {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        self.messages.push(user_message);

        let mut turns: usize = 0;

        loop {
            // Check cancellation
            if tool_ctx.cancellation_token.is_cancelled() {
                let _ = tx
                    .send(StreamEvent::Error(StreamError::non_retryable(
                        "cancelled",
                    )))
                    .await;
                break;
            }

            // Check max turns
            if let Some(max) = self.config.max_turns
                && turns >= max
            {
                let _ = tx
                    .send(StreamEvent::Error(StreamError::non_retryable(format!(
                        "max turns reached ({max})"
                    ))))
                    .await;
                break;
            }

            // Fire LoopIteration hooks
            match fire_loop_iteration_hooks(&self.hooks, turns).await {
                Ok(Some(HookAction::Terminate { reason })) => {
                    let _ = tx
                        .send(StreamEvent::Error(StreamError::non_retryable(format!(
                            "hook terminated: {reason}"
                        ))))
                        .await;
                    break;
                }
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Error(StreamError::non_retryable(format!(
                            "hook error: {e}"
                        ))))
                        .await;
                    break;
                }
                _ => {}
            }

            // Check context compaction
            let token_count = self.context.token_estimate(&self.messages);
            if self.context.should_compact(&self.messages, token_count) {
                let old_tokens = token_count;
                match self.context.compact(self.messages.clone()).await {
                    Ok(compacted) => {
                        self.messages = compacted;
                        let new_tokens = self.context.token_estimate(&self.messages);

                        // Fire ContextCompaction hooks
                        match fire_compaction_hooks(&self.hooks, old_tokens, new_tokens).await {
                            Ok(Some(HookAction::Terminate { reason })) => {
                                let _ = tx
                                    .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                        "hook terminated: {reason}"
                                    ))))
                                    .await;
                                break;
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                        "hook error: {e}"
                                    ))))
                                    .await;
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                "compaction error: {e}"
                            ))))
                            .await;
                        break;
                    }
                }
            }

            // Build completion request
            let request = CompletionRequest {
                model: String::new(),
                messages: self.messages.clone(),
                system: Some(self.config.system_prompt.clone()),
                tools: self.tools.definitions(),
                ..Default::default()
            };

            // Fire PreLlmCall hooks
            match fire_pre_llm_hooks(&self.hooks, &request).await {
                Ok(Some(HookAction::Terminate { reason })) => {
                    let _ = tx
                        .send(StreamEvent::Error(StreamError::non_retryable(format!(
                            "hook terminated: {reason}"
                        ))))
                        .await;
                    break;
                }
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Error(StreamError::non_retryable(format!(
                            "hook error: {e}"
                        ))))
                        .await;
                    break;
                }
                _ => {}
            }

            // Call provider: durable path uses complete() with synthesized events,
            // non-durable path uses complete_stream() for real streaming.
            let message = if let Some(ref durable) = self.durability {
                // Durable path: use execute_llm_call for journaling/replay
                let options = neuron_types::ActivityOptions {
                    start_to_close_timeout: DEFAULT_ACTIVITY_TIMEOUT,
                    heartbeat_timeout: None,
                    retry_policy: None,
                };
                let response = match durable.0.erased_execute_llm_call(request, options).await {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                "durable error: {e}"
                            ))))
                            .await;
                        break;
                    }
                };

                // Synthesize stream events from the durable response
                for block in &response.message.content {
                    if let ContentBlock::Text(text) = block
                        && tx.send(StreamEvent::TextDelta(text.clone())).await.is_err()
                    {
                        return rx;
                    }
                }
                if tx.send(StreamEvent::Usage(response.usage.clone())).await.is_err() {
                    return rx;
                }
                if tx
                    .send(StreamEvent::MessageComplete(response.message.clone()))
                    .await
                    .is_err()
                {
                    return rx;
                }

                // Fire PostLlmCall hooks
                match fire_post_llm_hooks(&self.hooks, &response).await {
                    Ok(Some(HookAction::Terminate { reason })) => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                "hook terminated: {reason}"
                            ))))
                            .await;
                        break;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                "hook error: {e}"
                            ))))
                            .await;
                        break;
                    }
                    _ => {}
                }

                response.message
            } else {
                // Non-durable path: use streaming provider call
                let stream_handle = match self.provider.complete_stream(request).await {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                "provider error: {e}"
                            ))))
                            .await;
                        break;
                    }
                };

                // Forward all stream events to the channel, collect the assembled message
                let mut assembled_message: Option<Message> = None;
                let mut assembled_response: Option<neuron_types::CompletionResponse> = None;
                let mut stream = stream_handle.receiver;

                while let Some(event) = stream.next().await {
                    match &event {
                        StreamEvent::MessageComplete(msg) => {
                            assembled_message = Some(msg.clone());
                        }
                        StreamEvent::Usage(u) => {
                            // Build a partial CompletionResponse for PostLlmCall
                            assembled_response = Some(neuron_types::CompletionResponse {
                                id: String::new(),
                                model: String::new(),
                                message: assembled_message.clone().unwrap_or(Message {
                                    role: Role::Assistant,
                                    content: vec![],
                                }),
                                usage: u.clone(),
                                stop_reason: StopReason::EndTurn,
                            });
                        }
                        _ => {}
                    }
                    // Forward event to caller
                    if tx.send(event).await.is_err() {
                        // Receiver dropped, stop
                        return rx;
                    }
                }

                // Process the assembled message
                let msg = match assembled_message {
                    Some(m) => m,
                    None => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(
                                "stream ended without MessageComplete",
                            )))
                            .await;
                        break;
                    }
                };

                // Fire PostLlmCall hooks with the assembled response
                if let Some(mut resp) = assembled_response {
                    resp.message = msg.clone();
                    match fire_post_llm_hooks(&self.hooks, &resp).await {
                        Ok(Some(HookAction::Terminate { reason })) => {
                            let _ = tx
                                .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                    "hook terminated: {reason}"
                                ))))
                                .await;
                            break;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                    "hook error: {e}"
                                ))))
                                .await;
                            break;
                        }
                        _ => {}
                    }
                } else {
                    // No usage event received; construct a minimal response for the hook
                    let resp = neuron_types::CompletionResponse {
                        id: String::new(),
                        model: String::new(),
                        message: msg.clone(),
                        usage: TokenUsage::default(),
                        stop_reason: StopReason::EndTurn,
                    };
                    match fire_post_llm_hooks(&self.hooks, &resp).await {
                        Ok(Some(HookAction::Terminate { reason })) => {
                            let _ = tx
                                .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                    "hook terminated: {reason}"
                                ))))
                                .await;
                            break;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                    "hook error: {e}"
                                ))))
                                .await;
                            break;
                        }
                        _ => {}
                    }
                }

                msg
            };

            turns += 1;

            // Check for tool calls
            let tool_calls: Vec<_> = message
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

            self.messages.push(message.clone());

            // Server-side compaction: continue the loop for the next iteration.
            // The compacted context is already in the message history.
            // (In streaming mode we don't have a StopReason directly, but
            // compaction blocks in the content signal this condition.)

            if tool_calls.is_empty() {
                // Done — final response was already streamed
                break;
            }

            // Check cancellation before tool execution
            if tool_ctx.cancellation_token.is_cancelled() {
                let _ = tx
                    .send(StreamEvent::Error(StreamError::non_retryable(
                        "cancelled",
                    )))
                    .await;
                break;
            }

            // Execute tool calls with hooks and durability
            let mut tool_result_blocks = Vec::new();
            for (call_id, tool_name, input) in &tool_calls {
                // Fire PreToolExecution hooks
                match fire_pre_tool_hooks(&self.hooks, tool_name, input).await {
                    Ok(Some(HookAction::Terminate { reason })) => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                "hook terminated: {reason}"
                            ))))
                            .await;
                        return rx;
                    }
                    Ok(Some(HookAction::Skip { reason })) => {
                        tool_result_blocks.push(ContentBlock::ToolResult {
                            tool_use_id: call_id.clone(),
                            content: vec![ContentItem::Text(format!(
                                "Tool call skipped: {reason}"
                            ))],
                            is_error: true,
                        });
                        continue;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                "hook error: {e}"
                            ))))
                            .await;
                        return rx;
                    }
                    _ => {}
                }

                // Execute tool (via durability wrapper if present)
                let result = if let Some(ref durable) = self.durability {
                    let options = neuron_types::ActivityOptions {
                        start_to_close_timeout: DEFAULT_ACTIVITY_TIMEOUT,
                        heartbeat_timeout: None,
                        retry_policy: None,
                    };
                    match durable
                        .0
                        .erased_execute_tool(tool_name, input.clone(), tool_ctx, options)
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = tx
                                .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                    "durable tool error: {e}"
                                ))))
                                .await;
                            return rx;
                        }
                    }
                } else {
                    match self.tools.execute(tool_name, input.clone(), tool_ctx).await {
                        Ok(r) => r,
                        Err(neuron_types::ToolError::ModelRetry(hint)) => {
                            // Convert ModelRetry into an error tool result so the
                            // model can self-correct on the next iteration.
                            ToolOutput {
                                content: vec![ContentItem::Text(hint)],
                                structured_content: None,
                                is_error: true,
                            }
                        }
                        Err(e) => {
                            let _ = tx
                                .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                    "tool error: {e}"
                                ))))
                                .await;
                            return rx;
                        }
                    }
                };

                // Fire PostToolExecution hooks
                match fire_post_tool_hooks(&self.hooks, tool_name, &result).await {
                    Ok(Some(HookAction::Terminate { reason })) => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                "hook terminated: {reason}"
                            ))))
                            .await;
                        return rx;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error(StreamError::non_retryable(format!(
                                "hook error: {e}"
                            ))))
                            .await;
                        return rx;
                    }
                    _ => {}
                }

                tool_result_blocks.push(ContentBlock::ToolResult {
                    tool_use_id: call_id.clone(),
                    content: result.content,
                    is_error: result.is_error,
                });
            }

            self.messages.push(Message {
                role: Role::User,
                content: tool_result_blocks,
            });
        }

        rx
    }
}
