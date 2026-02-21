//! Core AgentLoop struct and run methods.

use agent_tool::ToolRegistry;
use agent_types::{
    CompletionRequest, ContentBlock, ContextStrategy, LoopError, Message, Provider, Role,
    StopReason, TokenUsage, ToolContext,
};

use crate::config::LoopConfig;

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

/// The agentic while loop: drives provider + tool + context interactions.
///
/// Generic over `P: Provider` (the LLM backend) and `C: ContextStrategy`
/// (the compaction strategy). Hooks and durability are optional.
pub struct AgentLoop<P: Provider, C: ContextStrategy> {
    provider: P,
    tools: ToolRegistry,
    context: C,
    config: LoopConfig,
    messages: Vec<Message>,
}

impl<P: Provider, C: ContextStrategy> AgentLoop<P, C> {
    /// Create a new `AgentLoop` with the given provider, tools, context strategy,
    /// and configuration.
    pub fn new(provider: P, tools: ToolRegistry, context: C, config: LoopConfig) -> Self {
        Self {
            provider,
            tools,
            context,
            config,
            messages: Vec::new(),
        }
    }

    /// Returns a reference to the current configuration.
    pub fn config(&self) -> &LoopConfig {
        &self.config
    }

    /// Returns a reference to the current messages.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Returns a mutable reference to the tool registry.
    pub fn tools_mut(&mut self) -> &mut ToolRegistry {
        &mut self.tools
    }

    /// Run the agentic loop to completion.
    ///
    /// Appends the user message, then loops: call provider, execute tools if
    /// needed, append results, repeat until the model returns a text-only
    /// response or the turn limit is reached.
    ///
    /// # Errors
    ///
    /// Returns `LoopError::MaxTurns` if the turn limit is exceeded,
    /// `LoopError::Provider` on provider failures, or `LoopError::Tool`
    /// on tool execution failures.
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
            if let Some(max) = self.config.max_turns {
                if turns >= max {
                    return Err(LoopError::MaxTurns(max));
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

            // Call provider
            let response = self.provider.complete(request).await?;

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
                let result = self.tools.execute(tool_name, input.clone(), tool_ctx).await?;
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

/// Extract text content from a message.
fn extract_text(message: &Message) -> String {
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
fn accumulate_usage(total: &mut TokenUsage, delta: &TokenUsage) {
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
