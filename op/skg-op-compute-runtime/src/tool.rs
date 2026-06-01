//! [`PythonExecTool`] — Operator adapter for executing Python code via a compute runtime.

use std::sync::Arc;

use async_trait::async_trait;
use layer0::capability::{
    ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityId, CapabilityKind, ExecutionClass,
    SchedulingFacts, StreamingSupport,
};
use layer0::content::{Content, ContentBlock};
use layer0::dispatch::DispatchHandle;
use layer0::dispatch_context::DispatchContext;
use layer0::error::{ErrorCode, ProtocolError};
use layer0::id::SessionId;
use layer0::operator::{
    Operator, OperatorInput, OperatorOutput, Outcome, TerminalOutcome, completed_handle,
    failed_handle,
};
use serde_json::json;

use crate::{ComputeRuntime, ExecutionProfile};

/// Operator adapter that executes Python code via a [`ComputeRuntime`].
///
/// Register this in a Router to make Python execution available as a tool
/// in an AgentLoop. Input must be a JSON object with a required `"code"`
/// field and an optional `"session"` field (string).
///
/// When `"session"` is absent the dispatch ID is used as the session key,
/// so each independent dispatch runs in its own isolated namespace.
pub struct PythonExecTool {
    runtime: Arc<dyn ComputeRuntime>,
    profile: ExecutionProfile,
}

impl PythonExecTool {
    /// Create a new tool backed by `runtime` using `profile` for every execution.
    pub fn new(runtime: Arc<dyn ComputeRuntime>, profile: ExecutionProfile) -> Self {
        Self { runtime, profile }
    }
}

#[async_trait]
impl Operator for PythonExecTool {
    fn descriptor(&self) -> CapabilityDescriptor {
        let mut d = CapabilityDescriptor::new(
            CapabilityId::new("python_exec"),
            CapabilityKind::Tool,
            "python_exec",
            "Execute Python code in a sandboxed runtime session.",
            SchedulingFacts::new(ExecutionClass::Shared, false, false, true, None),
            ApprovalFacts::None,
            AuthFacts::Open,
        );
        d.input_schema = Some(json!({
            "type": "object",
            "properties": {
                "code": { "type": "string" },
                "session": { "type": "string" }
            },
            "required": ["code"]
        }));
        d.streaming = StreamingSupport::None;
        d
    }

    async fn handle(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<DispatchHandle, ProtocolError> {
        let params = parse_input(&input.message)?;

        let code = params
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ProtocolError::new(
                    ErrorCode::InvalidInput,
                    "python_exec: missing required field 'code'",
                    false,
                )
            })?
            .to_owned();

        let session_id = params
            .get("session")
            .and_then(|v| v.as_str())
            .map(SessionId::new)
            .unwrap_or_else(|| SessionId::new(ctx.dispatch_id.as_str()));

        let dispatch_id = ctx.dispatch_id.clone();
        let result = self.runtime.exec(&session_id, &code, &self.profile).await;

        match result {
            Err(e) => Ok(failed_handle(
                dispatch_id,
                ProtocolError::internal(e.to_string()),
            )),
            Ok(report) => {
                let message = if let Some(data) = report.final_result {
                    Content::Blocks(vec![ContentBlock::Data {
                        data,
                        media_type: Some("application/json".into()),
                    }])
                } else {
                    Content::text(report.stdout)
                };
                let output = OperatorOutput::new(
                    message,
                    Outcome::Terminal {
                        terminal: TerminalOutcome::Completed,
                    },
                );
                Ok(completed_handle(dispatch_id, output))
            }
        }
    }
}

/// Extract a JSON-object payload from operator input content.
///
/// Tries, in order:
/// 1. `Content::Text` — parses the string as JSON.
/// 2. `Content::Blocks` with a `Data` block — returns the data value directly.
/// 3. `Content::Blocks` with a `ToolUse` block — returns the tool input.
/// 4. `Content::Blocks` with a `Text` block — parses the string as JSON.
fn parse_input(message: &Content) -> Result<serde_json::Value, ProtocolError> {
    match message {
        Content::Text(s) => json_parse(s),
        Content::Blocks(blocks) => {
            for block in blocks {
                match block {
                    ContentBlock::Data { data, .. } => return Ok(data.clone()),
                    ContentBlock::ToolUse { input, .. } => return Ok(input.clone()),
                    _ => {}
                }
            }
            for block in blocks {
                if let ContentBlock::Text { text } = block {
                    return json_parse(text);
                }
            }
            Err(ProtocolError::new(
                ErrorCode::InvalidInput,
                "python_exec: no parseable content in input blocks",
                false,
            ))
        }
        // Content is #[non_exhaustive]; future variants are unsupported here.
        _ => Err(ProtocolError::new(
            ErrorCode::InvalidInput,
            "python_exec: unsupported content type",
            false,
        )),
    }
}

fn json_parse(s: &str) -> Result<serde_json::Value, ProtocolError> {
    serde_json::from_str(s).map_err(|e| {
        ProtocolError::new(
            ErrorCode::InvalidInput,
            format!("python_exec: input is not valid JSON: {e}"),
            false,
        )
    })
}
