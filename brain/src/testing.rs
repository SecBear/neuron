//! Offline test harness for Brain.
//!
//! This is intentionally minimal: it provides deterministic scaffolding for
//! the `brain/tests/offline_integration.rs` contract.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

/// A fake tool definition used by offline tests.
#[derive(Debug, Clone)]
pub struct FakeTool {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// Tool call result returned by the fake tool.
    pub result: Value,
}

/// Construct a fake tool definition for offline tests.
pub fn fake_tool(name: &str, description: &str, result: Value) -> FakeTool {
    FakeTool {
        name: name.to_string(),
        description: description.to_string(),
        result,
    }
}

/// A mocked controller response.
#[derive(Debug, Clone)]
pub enum ControllerResponse {
    /// Controller requests a tool call.
    ToolUse {
        /// Tool use id.
        id: String,
        /// Tool name.
        name: String,
        /// Tool input.
        input: Value,
    },
    /// Controller emits final text.
    Text {
        /// Output text.
        text: String,
    },
}

/// Construct a tool-use controller response.
pub fn tool_use_response(id: &str, name: &str, input: Value) -> ControllerResponse {
    ControllerResponse::ToolUse {
        id: id.to_string(),
        name: name.to_string(),
        input,
    }
}

/// Construct a text controller/worker response.
pub fn text_response(text: &str) -> ControllerResponse {
    ControllerResponse::Text {
        text: text.to_string(),
    }
}

/// Offline execution scenario.
#[derive(Debug, Clone)]
pub struct OfflineBrainScenario {
    /// User message that started the run.
    pub user_message: String,
    /// Mocked controller responses in order.
    pub controller_responses: Vec<ControllerResponse>,
    /// Mocked worker responses in order.
    pub worker_responses: Vec<ControllerResponse>,
    /// Path to the `.mcp.json` config.
    pub mcp_path: PathBuf,
    /// Fake MCP tools to expose.
    pub fake_mcp_tools: Vec<FakeTool>,
}

/// Result of an offline run.
#[derive(Debug, Clone)]
pub struct OfflineBrainRun {
    /// Final synthesized answer.
    pub final_answer: String,
    /// Tool names the controller attempted to call.
    pub tool_calls: Vec<String>,
    /// Parsed JSON from the worker tool response (when used).
    pub worker_json: Value,
    /// Tool names that were exposed after policy gating.
    pub exposed_tools: Vec<String>,
}

/// Errors from the offline harness.
#[derive(Debug)]
pub enum OfflineBrainError {
    /// Failed to read `.mcp.json`.
    McpRead(String),
    /// Invalid `.mcp.json`.
    McpParse(String),
    /// Scenario ended without a final text answer.
    MissingFinalAnswer,
    /// Worker tool was invoked but no worker response was provided.
    MissingWorkerResponse,
    /// Worker response was not valid JSON.
    WorkerJsonParse(String),
}

impl std::fmt::Display for OfflineBrainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OfflineBrainError::McpRead(e) => write!(f, "mcp read failed: {e}"),
            OfflineBrainError::McpParse(e) => write!(f, "mcp parse failed: {e}"),
            OfflineBrainError::MissingFinalAnswer => write!(f, "missing final answer"),
            OfflineBrainError::MissingWorkerResponse => write!(f, "missing worker response"),
            OfflineBrainError::WorkerJsonParse(e) => write!(f, "worker json parse failed: {e}"),
        }
    }
}

impl std::error::Error for OfflineBrainError {}

fn parse_allow_deny(mcp_json: &Value) -> (Option<Vec<String>>, Vec<String>) {
    let x = mcp_json.get("x-brain");
    let allowlist = x
        .and_then(|v| v.get("allowlist"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        });
    let denylist = x
        .and_then(|v| v.get("denylist"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (allowlist, denylist)
}

fn gate_tools(
    mut tools: Vec<String>,
    allowlist: &Option<Vec<String>>,
    denylist: &[String],
) -> Vec<String> {
    if let Some(allowed) = allowlist {
        tools.retain(|t| allowed.contains(t));
    }
    tools.retain(|t| !denylist.contains(t));
    tools.sort();
    tools.dedup();
    tools
}

/// Run a deterministic offline Brain scenario.
///
/// The harness:
/// - loads `.mcp.json` and applies `x-brain` allow/deny lists
/// - simulates a controller that calls worker tools
/// - parses the worker JSON output and returns it for assertions
pub async fn run_offline_brain(
    scenario: OfflineBrainScenario,
) -> Result<OfflineBrainRun, OfflineBrainError> {
    let mcp_raw = fs::read_to_string(&scenario.mcp_path)
        .map_err(|e| OfflineBrainError::McpRead(e.to_string()))?;
    let mcp_json: Value =
        serde_json::from_str(&mcp_raw).map_err(|e| OfflineBrainError::McpParse(e.to_string()))?;
    let (allowlist, denylist) = parse_allow_deny(&mcp_json);

    // Worker tool ids are stable names; the offline harness only needs the ones used by tests.
    let worker_tools = vec!["sonnet_summarize".to_string()];
    let mcp_tools = scenario.fake_mcp_tools.iter().map(|t| t.name.clone());

    let exposed_tools = gate_tools(
        worker_tools.into_iter().chain(mcp_tools).collect(),
        &allowlist,
        &denylist,
    );

    let mut tool_calls: Vec<String> = vec![];
    let mut worker_json = Value::Null;
    let mut worker_responses = scenario.worker_responses.into_iter();
    let mut final_answer: Option<String> = None;

    let _ = scenario.user_message;

    for resp in scenario.controller_responses {
        match resp {
            ControllerResponse::ToolUse { name, input, .. } => {
                tool_calls.push(name.clone());

                // The only worker tool exercised by the offline integration test today.
                if name == "sonnet_summarize" {
                    let _ = input;
                    let next = worker_responses
                        .next()
                        .ok_or(OfflineBrainError::MissingWorkerResponse)?;
                    let ControllerResponse::Text { text } = next else {
                        return Err(OfflineBrainError::MissingWorkerResponse);
                    };
                    worker_json = serde_json::from_str(&text)
                        .map_err(|e| OfflineBrainError::WorkerJsonParse(e.to_string()))?;
                }
            }
            ControllerResponse::Text { text } => {
                final_answer = Some(text);
                break;
            }
        }
    }

    Ok(OfflineBrainRun {
        final_answer: final_answer.ok_or(OfflineBrainError::MissingFinalAnswer)?,
        tool_calls,
        worker_json,
        exposed_tools,
    })
}
