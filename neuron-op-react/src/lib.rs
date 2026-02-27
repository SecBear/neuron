#![deny(missing_docs)]
//! ReAct operator — model + tools in a reasoning loop.
//!
//! Implements `layer0::Operator` by running the Reason-Act-Observe cycle:
//! assemble context → call model → execute tools → repeat until done.

use async_trait::async_trait;
use layer0::content::Content;
use layer0::duration::DurationMs;
use layer0::effect::{Effect, Scope, SignalPayload};
use layer0::error::OperatorError;
use layer0::hook::{HookAction, HookContext, HookPoint};
use layer0::id::{AgentId, WorkflowId};
use layer0::operator::{
    ExitReason, Operator, OperatorInput, OperatorMetadata, OperatorOutput, ToolCallRecord,
};
use neuron_hooks::HookRegistry;
use neuron_tool::ToolRegistry;
use neuron_turn::context::ContextStrategy;
use neuron_turn::convert::{content_to_user_message, parts_to_content};
use neuron_turn::provider::Provider;
use neuron_turn::types::*;
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Instant;

/// Static configuration for a ReactOperator instance.
pub struct ReactConfig {
    /// Base system prompt.
    pub system_prompt: String,
    /// Default model identifier.
    pub default_model: String,
    /// Default max tokens per response.
    pub default_max_tokens: u32,
    /// Default max turns before stopping.
    pub default_max_turns: u32,
}

impl Default for ReactConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            default_model: String::new(),
            default_max_tokens: 4096,
            default_max_turns: 10,
        }
    }
}

/// Names of tools that produce Effects instead of executing locally.
const EFFECT_TOOL_NAMES: &[&str] = &[
    "write_memory",
    "delete_memory",
    "delegate",
    "handoff",
    "signal",
];

/// Resolved configuration merging defaults with per-request overrides.
struct ResolvedConfig {
    model: Option<String>,
    system: String,
    max_turns: u32,
    max_cost: Option<Decimal>,
    max_duration: Option<DurationMs>,
    allowed_tools: Option<Vec<String>>,
    max_tokens: u32,
}

/// A full-featured Operator implementation with a ReAct loop.
///
/// Generic over `P: Provider` (not object-safe). The object-safe boundary
/// is `layer0::Operator`, which `ReactOperator<P>` implements via `#[async_trait]`.
pub struct ReactOperator<P: Provider> {
    provider: P,
    tools: ToolRegistry,
    context_strategy: Box<dyn ContextStrategy>,
    hooks: HookRegistry,
    state_reader: Arc<dyn layer0::StateReader>,
    config: ReactConfig,
}

impl<P: Provider> ReactOperator<P> {
    /// Create a new ReactOperator with all dependencies.
    pub fn new(
        provider: P,
        tools: ToolRegistry,
        context_strategy: Box<dyn ContextStrategy>,
        hooks: HookRegistry,
        state_reader: Arc<dyn layer0::StateReader>,
        config: ReactConfig,
    ) -> Self {
        Self {
            provider,
            tools,
            context_strategy,
            hooks,
            state_reader,
            config,
        }
    }

    fn resolve_config(&self, input: &OperatorInput) -> ResolvedConfig {
        let tc = input.config.as_ref();
        let system = match tc.and_then(|c| c.system_addendum.as_ref()) {
            Some(addendum) => format!("{}\n{}", self.config.system_prompt, addendum),
            None => self.config.system_prompt.clone(),
        };
        ResolvedConfig {
            model: tc.and_then(|c| c.model.clone()).or_else(|| {
                if self.config.default_model.is_empty() {
                    None
                } else {
                    Some(self.config.default_model.clone())
                }
            }),
            system,
            max_turns: tc
                .and_then(|c| c.max_turns)
                .unwrap_or(self.config.default_max_turns),
            max_cost: tc.and_then(|c| c.max_cost),
            max_duration: tc.and_then(|c| c.max_duration),
            allowed_tools: tc.and_then(|c| c.allowed_tools.clone()),
            max_tokens: self.config.default_max_tokens,
        }
    }

    fn build_tool_schemas(&self, config: &ResolvedConfig) -> Vec<ToolSchema> {
        let mut schemas: Vec<ToolSchema> = self
            .tools
            .iter()
            .map(|tool| ToolSchema {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
            })
            .collect();

        // Add effect tool schemas
        schemas.extend(effect_tool_schemas());

        // Filter by allowed_tools if specified
        if let Some(allowed) = &config.allowed_tools {
            schemas.retain(|s| allowed.contains(&s.name));
        }

        schemas
    }

    async fn assemble_context(
        &self,
        input: &OperatorInput,
    ) -> Result<Vec<ProviderMessage>, OperatorError> {
        let mut messages = Vec::new();

        // Read history from state if session is present
        if let Some(session) = &input.session {
            let scope = Scope::Session(session.clone());
            match self.state_reader.read(&scope, "messages").await {
                Ok(Some(history)) => {
                    if let Ok(history_messages) =
                        serde_json::from_value::<Vec<ProviderMessage>>(history)
                    {
                        messages = history_messages;
                    }
                }
                Ok(None) => {} // No history yet
                Err(_) => {}   // State read errors are non-fatal
            }
        }

        // Add the new user message
        messages.push(content_to_user_message(&input.message));

        Ok(messages)
    }

    fn try_as_effect(&self, name: &str, input: &serde_json::Value) -> Option<Effect> {
        match name {
            "write_memory" => {
                let scope_str = input.get("scope")?.as_str()?;
                let key = input.get("key")?.as_str()?.to_string();
                let value = input.get("value")?.clone();
                let scope = parse_scope(scope_str);
                Some(Effect::WriteMemory { scope, key, value })
            }
            "delete_memory" => {
                let scope_str = input.get("scope")?.as_str()?;
                let key = input.get("key")?.as_str()?.to_string();
                let scope = parse_scope(scope_str);
                Some(Effect::DeleteMemory { scope, key })
            }
            "delegate" => {
                let agent = input.get("agent")?.as_str()?;
                let message = input.get("message").and_then(|m| m.as_str()).unwrap_or("");
                let delegate_input =
                    OperatorInput::new(Content::text(message), layer0::operator::TriggerType::Task);
                Some(Effect::Delegate {
                    agent: AgentId::new(agent),
                    input: Box::new(delegate_input),
                })
            }
            "handoff" => {
                let agent = input.get("agent")?.as_str()?;
                let state = input
                    .get("state")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Some(Effect::Handoff {
                    agent: AgentId::new(agent),
                    state,
                })
            }
            "signal" => {
                let target = input.get("target")?.as_str()?;
                let signal_type = input
                    .get("signal_type")
                    .and_then(|s| s.as_str())
                    .unwrap_or("default");
                let data = input
                    .get("data")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Some(Effect::Signal {
                    target: WorkflowId::new(target),
                    payload: SignalPayload::new(signal_type, data),
                })
            }
            _ => None,
        }
    }

    fn build_metadata(
        &self,
        tokens_in: u64,
        tokens_out: u64,
        cost: Decimal,
        turns_used: u32,
        tools_called: Vec<ToolCallRecord>,
        duration: DurationMs,
    ) -> OperatorMetadata {
        let mut meta = OperatorMetadata::default();
        meta.tokens_in = tokens_in;
        meta.tokens_out = tokens_out;
        meta.cost = cost;
        meta.turns_used = turns_used;
        meta.tools_called = tools_called;
        meta.duration = duration;
        meta
    }

    fn make_output(
        message: Content,
        exit_reason: ExitReason,
        metadata: OperatorMetadata,
        effects: Vec<Effect>,
    ) -> OperatorOutput {
        let mut output = OperatorOutput::new(message, exit_reason);
        output.metadata = metadata;
        output.effects = effects;
        output
    }

    fn build_hook_context(
        &self,
        point: HookPoint,
        tokens_in: u64,
        tokens_out: u64,
        cost: Decimal,
        turns_completed: u32,
        elapsed: DurationMs,
    ) -> HookContext {
        let mut ctx = HookContext::new(point);
        ctx.tokens_used = tokens_in + tokens_out;
        ctx.cost = cost;
        ctx.turns_completed = turns_completed;
        ctx.elapsed = elapsed;
        ctx
    }
}

#[async_trait]
impl<P: Provider + 'static> Operator for ReactOperator<P> {
    async fn execute(&self, input: OperatorInput) -> Result<OperatorOutput, OperatorError> {
        let start = Instant::now();
        let config = self.resolve_config(&input);
        let mut messages = self.assemble_context(&input).await?;
        let tools = self.build_tool_schemas(&config);

        let mut total_tokens_in: u64 = 0;
        let mut total_tokens_out: u64 = 0;
        let mut total_cost = Decimal::ZERO;
        let mut turns_used: u32 = 0;
        let mut tool_records: Vec<ToolCallRecord> = vec![];
        let mut effects: Vec<Effect> = vec![];
        let mut last_content: Vec<ContentPart> = vec![];

        loop {
            turns_used += 1;

            // 1. Hook: PreInference
            let hook_ctx = self.build_hook_context(
                HookPoint::PreInference,
                total_tokens_in,
                total_tokens_out,
                total_cost,
                turns_used - 1,
                DurationMs::from(start.elapsed()),
            );
            if let HookAction::Halt { reason } = self.hooks.dispatch(&hook_ctx).await {
                return Ok(Self::make_output(
                    parts_to_content(&last_content),
                    ExitReason::ObserverHalt { reason },
                    self.build_metadata(
                        total_tokens_in,
                        total_tokens_out,
                        total_cost,
                        turns_used,
                        tool_records,
                        DurationMs::from(start.elapsed()),
                    ),
                    effects,
                ));
            }

            // 2. Build ProviderRequest
            let request = ProviderRequest {
                model: config.model.clone(),
                messages: messages.clone(),
                tools: tools.clone(),
                max_tokens: Some(config.max_tokens),
                temperature: None,
                system: Some(config.system.clone()),
                extra: input.metadata.clone(),
            };

            // 3. Call provider
            let response = self.provider.complete(request).await.map_err(|e| {
                if e.is_retryable() {
                    OperatorError::Retryable(e.to_string())
                } else {
                    OperatorError::Model(e.to_string())
                }
            })?;

            // 4. Hook: PostInference
            let mut hook_ctx = self.build_hook_context(
                HookPoint::PostInference,
                total_tokens_in + response.usage.input_tokens,
                total_tokens_out + response.usage.output_tokens,
                total_cost + response.cost.unwrap_or(Decimal::ZERO),
                turns_used,
                DurationMs::from(start.elapsed()),
            );
            hook_ctx.model_output = Some(parts_to_content(&response.content));
            if let HookAction::Halt { reason } = self.hooks.dispatch(&hook_ctx).await {
                return Ok(Self::make_output(
                    parts_to_content(&response.content),
                    ExitReason::ObserverHalt { reason },
                    self.build_metadata(
                        total_tokens_in + response.usage.input_tokens,
                        total_tokens_out + response.usage.output_tokens,
                        total_cost + response.cost.unwrap_or(Decimal::ZERO),
                        turns_used,
                        tool_records,
                        DurationMs::from(start.elapsed()),
                    ),
                    effects,
                ));
            }

            // 5. Aggregate tokens + cost
            total_tokens_in += response.usage.input_tokens;
            total_tokens_out += response.usage.output_tokens;
            if let Some(cost) = response.cost {
                total_cost += cost;
            }

            last_content.clone_from(&response.content);

            // 6. Check StopReason
            match response.stop_reason {
                StopReason::MaxTokens => {
                    return Err(OperatorError::Model("output truncated (max_tokens)".into()));
                }
                StopReason::ContentFilter => {
                    return Err(OperatorError::Model("content filtered".into()));
                }
                StopReason::EndTurn => {
                    return Ok(Self::make_output(
                        parts_to_content(&response.content),
                        ExitReason::Complete,
                        self.build_metadata(
                            total_tokens_in,
                            total_tokens_out,
                            total_cost,
                            turns_used,
                            tool_records,
                            DurationMs::from(start.elapsed()),
                        ),
                        effects,
                    ));
                }
                StopReason::ToolUse => {
                    // Continue to tool execution below
                }
            }

            // 7. Tool execution
            // Add assistant message to context
            messages.push(ProviderMessage {
                role: Role::Assistant,
                content: response.content.clone(),
            });

            let tool_uses: Vec<(String, String, serde_json::Value)> = response
                .content
                .iter()
                .filter_map(|part| match part {
                    ContentPart::ToolUse { id, name, input } => {
                        Some((id.clone(), name.clone(), input.clone()))
                    }
                    _ => None,
                })
                .collect();

            let mut tool_results = Vec::new();

            for (id, name, tool_input) in tool_uses {
                // a. Check effect tool
                if EFFECT_TOOL_NAMES.contains(&name.as_str()) {
                    if let Some(effect) = self.try_as_effect(&name, &tool_input) {
                        effects.push(effect);
                    }
                    tool_results.push(ContentPart::ToolResult {
                        tool_use_id: id,
                        content: format!("{name} effect recorded."),
                        is_error: false,
                    });
                    tool_records.push(ToolCallRecord::new(&name, DurationMs::ZERO, true));
                    continue;
                }

                // b. Hook: PreToolUse
                let mut actual_input = tool_input.clone();
                let mut hook_ctx = HookContext::new(HookPoint::PreToolUse);
                hook_ctx.tool_name = Some(name.clone());
                hook_ctx.tool_input = Some(tool_input.clone());
                hook_ctx.tokens_used = total_tokens_in + total_tokens_out;
                hook_ctx.cost = total_cost;
                hook_ctx.turns_completed = turns_used;
                hook_ctx.elapsed = DurationMs::from(start.elapsed());

                match self.hooks.dispatch(&hook_ctx).await {
                    HookAction::Halt { reason } => {
                        return Ok(Self::make_output(
                            parts_to_content(&last_content),
                            ExitReason::ObserverHalt { reason },
                            self.build_metadata(
                                total_tokens_in,
                                total_tokens_out,
                                total_cost,
                                turns_used,
                                tool_records,
                                DurationMs::from(start.elapsed()),
                            ),
                            effects,
                        ));
                    }
                    HookAction::SkipTool { reason } => {
                        tool_results.push(ContentPart::ToolResult {
                            tool_use_id: id,
                            content: format!("Skipped: {reason}"),
                            is_error: false,
                        });
                        tool_records.push(ToolCallRecord::new(&name, DurationMs::ZERO, false));
                        continue;
                    }
                    HookAction::ModifyToolInput { new_input } => {
                        actual_input = new_input;
                    }
                    HookAction::Continue => {}
                    // Handle non_exhaustive
                    _ => {}
                }

                // c. Execute tool
                let tool_start = Instant::now();
                let result = match self.tools.get(&name) {
                    Some(tool) => tool.call(actual_input).await,
                    None => Err(neuron_tool::ToolError::NotFound(name.clone())),
                };
                let tool_duration = DurationMs::from(tool_start.elapsed());

                let (mut result_content, is_error, success) = match result {
                    Ok(value) => (
                        serde_json::to_string(&value).unwrap_or_default(),
                        false,
                        true,
                    ),
                    Err(e) => (e.to_string(), true, false),
                };

                // d. Hook: PostToolUse
                let mut hook_ctx = HookContext::new(HookPoint::PostToolUse);
                hook_ctx.tool_name = Some(name.clone());
                hook_ctx.tool_result = Some(result_content.clone());
                hook_ctx.tokens_used = total_tokens_in + total_tokens_out;
                hook_ctx.cost = total_cost;
                hook_ctx.turns_completed = turns_used;
                hook_ctx.elapsed = DurationMs::from(start.elapsed());

                match self.hooks.dispatch(&hook_ctx).await {
                    HookAction::Halt { reason } => {
                        return Ok(Self::make_output(
                            parts_to_content(&last_content),
                            ExitReason::ObserverHalt { reason },
                            self.build_metadata(
                                total_tokens_in,
                                total_tokens_out,
                                total_cost,
                                turns_used,
                                tool_records,
                                DurationMs::from(start.elapsed()),
                            ),
                            effects,
                        ));
                    }
                    HookAction::ModifyToolOutput { new_output } => {
                        // Replace tool result content before it flows into model context.
                        // HookRegistry short-circuits on first non-Continue, so only one
                        // hook's ModifyToolOutput can be returned per dispatch.
                        result_content = new_output.to_string();
                    }
                    _ => {} // Continue, SkipTool, ModifyToolInput are no-ops at PostToolUse
                }

                // e. Backfill result
                tool_results.push(ContentPart::ToolResult {
                    tool_use_id: id,
                    content: result_content,
                    is_error,
                });

                // f. Record
                tool_records.push(ToolCallRecord::new(name, tool_duration, success));
            }

            // Add tool results as user message
            messages.push(ProviderMessage {
                role: Role::User,
                content: tool_results,
            });

            // 8. Check limits
            if turns_used >= config.max_turns {
                return Ok(Self::make_output(
                    parts_to_content(&last_content),
                    ExitReason::MaxTurns,
                    self.build_metadata(
                        total_tokens_in,
                        total_tokens_out,
                        total_cost,
                        turns_used,
                        tool_records,
                        DurationMs::from(start.elapsed()),
                    ),
                    effects,
                ));
            }

            if let Some(max_cost) = &config.max_cost {
                if total_cost >= *max_cost {
                    return Ok(Self::make_output(
                        parts_to_content(&last_content),
                        ExitReason::BudgetExhausted,
                        self.build_metadata(
                            total_tokens_in,
                            total_tokens_out,
                            total_cost,
                            turns_used,
                            tool_records,
                            DurationMs::from(start.elapsed()),
                        ),
                        effects,
                    ));
                }
            }

            if let Some(max_duration) = &config.max_duration {
                if start.elapsed() >= max_duration.to_std() {
                    return Ok(Self::make_output(
                        parts_to_content(&last_content),
                        ExitReason::Timeout,
                        self.build_metadata(
                            total_tokens_in,
                            total_tokens_out,
                            total_cost,
                            turns_used,
                            tool_records,
                            DurationMs::from(start.elapsed()),
                        ),
                        effects,
                    ));
                }
            }

            // 9. Hook: ExitCheck
            let hook_ctx = self.build_hook_context(
                HookPoint::ExitCheck,
                total_tokens_in,
                total_tokens_out,
                total_cost,
                turns_used,
                DurationMs::from(start.elapsed()),
            );
            if let HookAction::Halt { reason } = self.hooks.dispatch(&hook_ctx).await {
                return Ok(Self::make_output(
                    parts_to_content(&last_content),
                    ExitReason::ObserverHalt { reason },
                    self.build_metadata(
                        total_tokens_in,
                        total_tokens_out,
                        total_cost,
                        turns_used,
                        tool_records,
                        DurationMs::from(start.elapsed()),
                    ),
                    effects,
                ));
            }

            // 10. Context compaction
            let limit = config.max_tokens as usize * 4;
            if self.context_strategy.should_compact(&messages, limit) {
                messages = self.context_strategy.compact(messages);
            }

            // 11. Loop repeats
        }
    }
}

/// Schemas for effect tools that the model can call.
fn effect_tool_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "write_memory".into(),
            description: "Write a value to persistent memory.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "scope": {"type": "string", "description": "Memory scope (e.g. 'global', 'session:id')"},
                    "key": {"type": "string", "description": "Memory key"},
                    "value": {"description": "Value to store"}
                },
                "required": ["scope", "key", "value"]
            }),
        },
        ToolSchema {
            name: "delete_memory".into(),
            description: "Delete a value from persistent memory.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "scope": {"type": "string", "description": "Memory scope"},
                    "key": {"type": "string", "description": "Memory key"}
                },
                "required": ["scope", "key"]
            }),
        },
        ToolSchema {
            name: "delegate".into(),
            description: "Delegate a task to another agent.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent": {"type": "string", "description": "Agent ID to delegate to"},
                    "message": {"type": "string", "description": "Task description for the agent"}
                },
                "required": ["agent", "message"]
            }),
        },
        ToolSchema {
            name: "handoff".into(),
            description: "Hand off the conversation to another agent.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent": {"type": "string", "description": "Agent ID to hand off to"},
                    "state": {"description": "State to pass to the next agent"}
                },
                "required": ["agent"]
            }),
        },
        ToolSchema {
            name: "signal".into(),
            description: "Send a signal to another workflow.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "target": {"type": "string", "description": "Target workflow ID"},
                    "signal_type": {"type": "string", "description": "Signal type identifier"},
                    "data": {"description": "Signal payload data"}
                },
                "required": ["target"]
            }),
        },
    ]
}

/// Parse a scope string into a layer0 Scope.
fn parse_scope(s: &str) -> Scope {
    if s == "global" {
        return Scope::Global;
    }
    if let Some(id) = s.strip_prefix("session:") {
        return Scope::Session(layer0::SessionId::new(id));
    }
    if let Some(id) = s.strip_prefix("workflow:") {
        return Scope::Workflow(layer0::WorkflowId::new(id));
    }
    Scope::Custom(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neuron_hooks::HookRegistry;
    use neuron_tool::ToolRegistry;
    use neuron_turn::context::NoCompaction;
    use neuron_turn::provider::ProviderError;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // -- Mock Provider --

    struct MockProvider {
        responses: Mutex<VecDeque<ProviderResponse>>,
        call_count: AtomicUsize,
    }

    impl MockProvider {
        fn new(responses: Vec<ProviderResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                call_count: AtomicUsize::new(0),
            }
        }
    }

    impl Provider for MockProvider {
        fn complete(
            &self,
            _request: ProviderRequest,
        ) -> impl std::future::Future<Output = Result<ProviderResponse, ProviderError>> + Send
        {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let response = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("MockProvider: no more responses queued");
            async move { Ok(response) }
        }
    }

    // -- Mock StateReader --

    struct NullStateReader;

    #[async_trait]
    impl layer0::StateReader for NullStateReader {
        async fn read(
            &self,
            _scope: &Scope,
            _key: &str,
        ) -> Result<Option<serde_json::Value>, layer0::StateError> {
            Ok(None)
        }
        async fn list(
            &self,
            _scope: &Scope,
            _prefix: &str,
        ) -> Result<Vec<String>, layer0::StateError> {
            Ok(vec![])
        }
        async fn search(
            &self,
            _scope: &Scope,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<layer0::state::SearchResult>, layer0::StateError> {
            Ok(vec![])
        }
    }

    // -- Mock Tool --

    struct EchoTool;

    impl neuron_tool::ToolDyn for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes input"
        }
        fn input_schema(&self) -> serde_json::Value {
            json!({"type": "object"})
        }
        fn call(
            &self,
            input: serde_json::Value,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<serde_json::Value, neuron_tool::ToolError>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async move { Ok(json!({"echoed": input})) })
        }
    }

    // -- Helpers --

    fn simple_text_response(text: &str) -> ProviderResponse {
        ProviderResponse {
            content: vec![ContentPart::Text {
                text: text.to_string(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            },
            model: "mock-model".into(),
            cost: Some(Decimal::new(1, 4)), // $0.0001
            truncated: None,
        }
    }

    fn tool_use_response(
        tool_id: &str,
        tool_name: &str,
        input: serde_json::Value,
    ) -> ProviderResponse {
        ProviderResponse {
            content: vec![ContentPart::ToolUse {
                id: tool_id.to_string(),
                name: tool_name.to_string(),
                input,
            }],
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 15,
                ..Default::default()
            },
            model: "mock-model".into(),
            cost: Some(Decimal::new(2, 4)), // $0.0002
            truncated: None,
        }
    }

    fn make_op<P: Provider>(provider: P) -> ReactOperator<P> {
        ReactOperator::new(
            provider,
            ToolRegistry::new(),
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            ReactConfig::default(),
        )
    }

    fn make_op_with_tools<P: Provider>(provider: P, tools: ToolRegistry) -> ReactOperator<P> {
        ReactOperator::new(
            provider,
            tools,
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            ReactConfig::default(),
        )
    }

    fn simple_input(text: &str) -> OperatorInput {
        OperatorInput::new(Content::text(text), layer0::operator::TriggerType::User)
    }

    // -- Tests --

    #[tokio::test]
    async fn simple_completion() {
        let provider = MockProvider::new(vec![simple_text_response("Hello!")]);
        let op = make_op(provider);

        let output = op.execute(simple_input("Hi")).await.unwrap();

        assert_eq!(output.exit_reason, ExitReason::Complete);
        assert_eq!(output.message.as_text().unwrap(), "Hello!");
        assert_eq!(output.metadata.turns_used, 1);
        assert_eq!(output.metadata.tokens_in, 10);
        assert_eq!(output.metadata.tokens_out, 5);
        assert!(output.effects.is_empty());
    }

    #[tokio::test]
    async fn tool_use_and_followup() {
        let provider = MockProvider::new(vec![
            tool_use_response("tu_1", "echo", json!({"msg": "test"})),
            simple_text_response("Done."),
        ]);
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(EchoTool));
        let op = make_op_with_tools(provider, tools);

        let output = op.execute(simple_input("Use echo")).await.unwrap();

        assert_eq!(output.exit_reason, ExitReason::Complete);
        assert_eq!(output.metadata.turns_used, 2);
        assert_eq!(output.metadata.tools_called.len(), 1);
        assert_eq!(output.metadata.tools_called[0].name, "echo");
    }

    #[tokio::test]
    async fn unknown_tool_returns_error_result() {
        let provider = MockProvider::new(vec![
            tool_use_response("tu_1", "nonexistent_tool", json!({})),
            simple_text_response("Got an error."),
        ]);
        let op = make_op(provider);

        // Should not panic — unknown tool produces an error result but loop continues
        let output = op.execute(simple_input("Use nonexistent")).await.unwrap();
        assert_eq!(output.exit_reason, ExitReason::Complete);
        // The tool call was recorded
        assert_eq!(output.metadata.tools_called.len(), 1);
    }

    #[tokio::test]
    async fn max_turns_enforced() {
        // Provider always returns ToolUse — loop should hit max_turns limit
        let provider = MockProvider::new(vec![
            tool_use_response("tu_1", "echo", json!({})),
            tool_use_response("tu_2", "echo", json!({})),
            tool_use_response("tu_3", "echo", json!({})),
            simple_text_response("never reached"),
        ]);
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(EchoTool));

        let mut op = ReactOperator::new(
            provider,
            tools,
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            ReactConfig {
                default_max_turns: 2,
                ..Default::default()
            },
        );
        // Avoid unused warning
        let _ = &mut op;

        let op = ReactOperator::new(
            MockProvider::new(vec![
                tool_use_response("tu_1", "echo", json!({})),
                tool_use_response("tu_2", "echo", json!({})),
                simple_text_response("never reached"),
            ]),
            {
                let mut t = ToolRegistry::new();
                t.register(Arc::new(EchoTool));
                t
            },
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            ReactConfig {
                default_max_turns: 2,
                ..Default::default()
            },
        );

        let output = op.execute(simple_input("loop")).await.unwrap();
        assert_eq!(output.exit_reason, ExitReason::MaxTurns);
        assert_eq!(output.metadata.turns_used, 2);
    }

    #[tokio::test]
    async fn budget_exhausted() {
        // Two calls, each costing $0.0001, with max_cost = $0.00015
        let provider = MockProvider::new(vec![
            tool_use_response("tu_1", "echo", json!({})),
            simple_text_response("Done"),
        ]);
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(EchoTool));
        let op = ReactOperator::new(
            provider,
            tools,
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            ReactConfig::default(),
        );

        let mut input = simple_input("spend");
        let mut tc = layer0::operator::OperatorConfig::default();
        tc.max_cost = Some(Decimal::new(15, 5)); // $0.00015
        input.config = Some(tc);

        let output = op.execute(input).await.unwrap();
        // First call costs $0.0002 > $0.00015, so BudgetExhausted after second call
        assert_eq!(output.exit_reason, ExitReason::BudgetExhausted);
    }

    #[tokio::test]
    async fn max_tokens_returns_model_error() {
        let provider = MockProvider::new(vec![ProviderResponse {
            content: vec![],
            stop_reason: StopReason::MaxTokens,
            usage: TokenUsage::default(),
            model: "mock".into(),
            cost: None,
            truncated: None,
        }]);
        let op = make_op(provider);

        let result = op.execute(simple_input("Hi")).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            OperatorError::Model(msg) => assert!(msg.contains("max_tokens")),
            other => panic!("expected OperatorError::Model, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn content_filter_returns_model_error() {
        let provider = MockProvider::new(vec![ProviderResponse {
            content: vec![],
            stop_reason: StopReason::ContentFilter,
            usage: TokenUsage::default(),
            model: "mock".into(),
            cost: None,
            truncated: None,
        }]);
        let op = make_op(provider);

        let result = op.execute(simple_input("Hi")).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            OperatorError::Model(msg) => assert!(msg.contains("content filtered")),
            other => panic!("expected OperatorError::Model, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn cost_aggregated_across_turns() {
        let provider = MockProvider::new(vec![
            tool_use_response("tu_1", "echo", json!({})),
            simple_text_response("Done"),
        ]);
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(EchoTool));
        let op = make_op_with_tools(provider, tools);

        let output = op.execute(simple_input("Hi")).await.unwrap();

        // First call: $0.0002, second call: $0.0001
        assert_eq!(output.metadata.cost, Decimal::new(3, 4));
        assert_eq!(output.metadata.tokens_in, 20);
        assert_eq!(output.metadata.tokens_out, 20);
    }

    #[tokio::test]
    async fn operator_config_overrides_defaults() {
        let provider = MockProvider::new(vec![simple_text_response("Hi")]);
        let op = make_op(provider);

        let mut input = simple_input("test");
        let mut tc = layer0::operator::OperatorConfig::default();
        tc.system_addendum = Some("Be concise.".into());
        tc.model = Some("custom-model".into());
        tc.max_turns = Some(5);
        input.config = Some(tc);

        let output = op.execute(input).await.unwrap();
        assert_eq!(output.exit_reason, ExitReason::Complete);
    }

    #[tokio::test]
    async fn effect_tool_write_memory() {
        let provider = MockProvider::new(vec![
            // Model calls write_memory
            ProviderResponse {
                content: vec![ContentPart::ToolUse {
                    id: "tu_1".into(),
                    name: "write_memory".into(),
                    input: json!({"scope": "global", "key": "test", "value": "hello"}),
                }],
                stop_reason: StopReason::ToolUse,
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    ..Default::default()
                },
                model: "mock".into(),
                cost: None,
                truncated: None,
            },
            simple_text_response("Memory written."),
        ]);
        let op = make_op(provider);

        let output = op.execute(simple_input("Write memory")).await.unwrap();

        assert_eq!(output.effects.len(), 1);
        match &output.effects[0] {
            Effect::WriteMemory { key, .. } => assert_eq!(key, "test"),
            _ => panic!("expected WriteMemory"),
        }
    }

    #[test]
    fn parse_scope_variants() {
        assert_eq!(parse_scope("global"), Scope::Global);
        assert_eq!(
            parse_scope("session:abc"),
            Scope::Session(layer0::SessionId::new("abc"))
        );
        assert_eq!(
            parse_scope("workflow:wf1"),
            Scope::Workflow(layer0::WorkflowId::new("wf1"))
        );
        match parse_scope("other") {
            Scope::Custom(s) => assert_eq!(s, "other"),
            _ => panic!("expected Custom"),
        }
    }

    #[tokio::test]
    async fn effect_tool_delete_memory() {
        let provider = MockProvider::new(vec![
            ProviderResponse {
                content: vec![ContentPart::ToolUse {
                    id: "tu_1".into(),
                    name: "delete_memory".into(),
                    input: json!({"scope": "global", "key": "old_key"}),
                }],
                stop_reason: StopReason::ToolUse,
                usage: TokenUsage::default(),
                model: "mock".into(),
                cost: None,
                truncated: None,
            },
            simple_text_response("Deleted."),
        ]);
        let op = make_op(provider);

        let output = op.execute(simple_input("Delete memory")).await.unwrap();
        assert_eq!(output.effects.len(), 1);
        match &output.effects[0] {
            Effect::DeleteMemory { key, .. } => assert_eq!(key, "old_key"),
            _ => panic!("expected DeleteMemory"),
        }
    }

    #[tokio::test]
    async fn effect_tool_delegate() {
        let provider = MockProvider::new(vec![
            ProviderResponse {
                content: vec![ContentPart::ToolUse {
                    id: "tu_1".into(),
                    name: "delegate".into(),
                    input: json!({"agent": "helper", "message": "do this task"}),
                }],
                stop_reason: StopReason::ToolUse,
                usage: TokenUsage::default(),
                model: "mock".into(),
                cost: None,
                truncated: None,
            },
            simple_text_response("Delegated."),
        ]);
        let op = make_op(provider);

        let output = op.execute(simple_input("Delegate task")).await.unwrap();
        assert_eq!(output.effects.len(), 1);
        match &output.effects[0] {
            Effect::Delegate { agent, input } => {
                assert_eq!(agent.as_str(), "helper");
                assert_eq!(input.message.as_text().unwrap(), "do this task");
            }
            _ => panic!("expected Delegate"),
        }
    }

    #[tokio::test]
    async fn effect_tool_handoff() {
        let provider = MockProvider::new(vec![
            ProviderResponse {
                content: vec![ContentPart::ToolUse {
                    id: "tu_1".into(),
                    name: "handoff".into(),
                    input: json!({"agent": "specialist", "state": {"context": "data"}}),
                }],
                stop_reason: StopReason::ToolUse,
                usage: TokenUsage::default(),
                model: "mock".into(),
                cost: None,
                truncated: None,
            },
            simple_text_response("Handed off."),
        ]);
        let op = make_op(provider);

        let output = op.execute(simple_input("Handoff")).await.unwrap();
        assert_eq!(output.effects.len(), 1);
        match &output.effects[0] {
            Effect::Handoff { agent, state } => {
                assert_eq!(agent.as_str(), "specialist");
                assert_eq!(state["context"], "data");
            }
            _ => panic!("expected Handoff"),
        }
    }

    #[tokio::test]
    async fn effect_tool_signal() {
        let provider = MockProvider::new(vec![
            ProviderResponse {
                content: vec![ContentPart::ToolUse {
                    id: "tu_1".into(),
                    name: "signal".into(),
                    input: json!({"target": "workflow_1", "signal_type": "completed", "data": {"result": "ok"}}),
                }],
                stop_reason: StopReason::ToolUse,
                usage: TokenUsage::default(),
                model: "mock".into(),
                cost: None,
                truncated: None,
            },
            simple_text_response("Signal sent."),
        ]);
        let op = make_op(provider);

        let output = op.execute(simple_input("Signal")).await.unwrap();
        assert_eq!(output.effects.len(), 1);
        match &output.effects[0] {
            Effect::Signal { target, payload } => {
                assert_eq!(target.as_str(), "workflow_1");
                assert_eq!(payload.signal_type, "completed");
            }
            _ => panic!("expected Signal"),
        }
    }

    #[test]
    fn effect_tool_schemas_all_present() {
        let schemas = effect_tool_schemas();
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"write_memory"));
        assert!(names.contains(&"delete_memory"));
        assert!(names.contains(&"delegate"));
        assert!(names.contains(&"handoff"));
        assert!(names.contains(&"signal"));
        assert_eq!(schemas.len(), 5);
    }

    #[test]
    fn react_operator_implements_operator_trait() {
        // Compile-time check: ReactOperator<MockProvider> implements Operator
        fn _assert_operator<T: Operator>() {}
        _assert_operator::<ReactOperator<MockProvider>>();
    }

    #[tokio::test]
    async fn react_operator_as_arc_dyn_operator() {
        // ReactOperator<P> can be used as Arc<dyn Operator>
        let provider = MockProvider::new(vec![simple_text_response("Hello!")]);
        let op: Arc<dyn Operator> = Arc::new(ReactOperator::new(
            provider,
            ToolRegistry::new(),
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            ReactConfig::default(),
        ));

        let output = op.execute(simple_input("Hi")).await.unwrap();
        assert_eq!(output.exit_reason, ExitReason::Complete);
    }

    #[tokio::test]
    async fn provider_retryable_error_maps_to_retryable() {
        struct ErrorProvider;
        impl Provider for ErrorProvider {
            #[allow(clippy::manual_async_fn)]
            fn complete(
                &self,
                _request: ProviderRequest,
            ) -> impl std::future::Future<Output = Result<ProviderResponse, ProviderError>> + Send
            {
                async { Err(ProviderError::RateLimited) }
            }
        }

        let op = ReactOperator::new(
            ErrorProvider,
            ToolRegistry::new(),
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            ReactConfig::default(),
        );

        let result = op.execute(simple_input("test")).await;
        assert!(matches!(result, Err(OperatorError::Retryable(_))));
    }

    #[tokio::test]
    async fn provider_call_count() {
        let provider = MockProvider::new(vec![
            tool_use_response("tu_1", "echo", json!({})),
            tool_use_response("tu_2", "echo", json!({})),
            simple_text_response("Done"),
        ]);
        let call_count = std::sync::Arc::new(AtomicUsize::new(0));

        struct CountingProvider {
            inner: MockProvider,
            count: std::sync::Arc<AtomicUsize>,
        }
        impl Provider for CountingProvider {
            #[allow(clippy::manual_async_fn)]
            fn complete(
                &self,
                request: ProviderRequest,
            ) -> impl std::future::Future<Output = Result<ProviderResponse, ProviderError>> + Send
            {
                self.count.fetch_add(1, Ordering::SeqCst);
                self.inner.complete(request)
            }
        }

        let counting_provider = CountingProvider {
            inner: MockProvider::new(vec![
                tool_use_response("tu_1", "echo", json!({})),
                tool_use_response("tu_2", "echo", json!({})),
                simple_text_response("Done"),
            ]),
            count: call_count.clone(),
        };

        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(EchoTool));
        let op = make_op_with_tools(counting_provider, tools);

        op.execute(simple_input("Multi-turn")).await.unwrap();
        // Only counting_provider was called — provider was called 3 times
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        // The unused `provider` variable should not cause issues
        drop(provider);
    }
}
