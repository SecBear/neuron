// Integration tests for the `#[skg_tool]` proc macro.
// Each test exercises the generated struct through the `SyncOperator` trait.

use layer0::capability::CapabilityKind;
use layer0::content::Content;
use layer0::error::ProtocolError;
use layer0::operator::{OperatorInput, TriggerType};
use layer0::{DispatchContext, DispatchId, OperatorId};
use serde_json::{Value, json};
use skg_context_engine::SyncOperator;
use skg_tool_macro::skg_tool;

fn test_ctx() -> DispatchContext {
    DispatchContext::new(DispatchId::new("test"), OperatorId::new("test-agent"))
}

fn json_input(value: Value) -> OperatorInput {
    OperatorInput::new(Content::text(value.to_string()), TriggerType::User)
}

// ── Test 1: basic required-parameter tool ─────────────────────────────────────

#[skg_tool(name = "get_weather", description = "Get current weather")]
async fn get_weather(location: String) -> Result<Value, ProtocolError> {
    Ok(json!({"location": location, "temp": 72}))
}

#[test]
fn test_basic_tool_descriptor() {
    let tool = GetWeatherTool::new();
    let desc = SyncOperator::descriptor(&tool);
    assert_eq!(desc.name, "get_weather");
    assert_eq!(desc.description, "Get current weather");
    assert_eq!(desc.kind, CapabilityKind::Tool);
}

#[tokio::test]
async fn test_basic_tool_execute() {
    let tool = GetWeatherTool::new();
    let ctx = test_ctx();
    let input = json_input(json!({"location": "San Francisco"}));
    let output = tool
        .execute(input, &ctx)
        .await
        .expect("execute must succeed");
    let result: Value =
        serde_json::from_str(output.message.as_text().unwrap()).expect("output must be valid JSON");
    assert_eq!(result["location"], "San Francisco");
    assert_eq!(result["temp"], 72);
}

// ── Test 2: optional parameter ────────────────────────────────────────────────

#[skg_tool(name = "search", description = "Search for things")]
async fn search(query: String, limit: Option<i32>) -> Result<Value, ProtocolError> {
    Ok(json!({"query": query, "limit": limit}))
}

#[tokio::test]
async fn test_optional_param_absent() {
    let tool = SearchTool::new();
    let ctx = test_ctx();
    let input = json_input(json!({"query": "rust proc macros"}));
    let output = tool
        .execute(input, &ctx)
        .await
        .expect("execute must succeed");
    let result: Value = serde_json::from_str(output.message.as_text().unwrap()).unwrap();
    assert_eq!(result["query"], "rust proc macros");
    assert!(result["limit"].is_null());
}

#[tokio::test]
async fn test_optional_param_present() {
    let tool = SearchTool::new();
    let ctx = test_ctx();
    let input = json_input(json!({"query": "rust", "limit": 10}));
    let output = tool
        .execute(input, &ctx)
        .await
        .expect("execute must succeed");
    let result: Value = serde_json::from_str(output.message.as_text().unwrap()).unwrap();
    assert_eq!(result["query"], "rust");
    assert_eq!(result["limit"], 10);
}

// ── Test 3: DispatchContext parameter ─────────────────────────────────────────

#[skg_tool(name = "agent_info", description = "Returns agent info from context")]
async fn agent_info(ctx: &DispatchContext, label: String) -> Result<Value, ProtocolError> {
    let operator_str = ctx.operator_id.to_string();
    Ok(json!({"agent": operator_str, "label": label}))
}

#[test]
fn test_ctx_param_excluded_from_descriptor() {
    let tool = AgentInfoTool::new();
    let desc = SyncOperator::descriptor(&tool);
    assert_eq!(desc.name, "agent_info");
    assert_eq!(desc.kind, CapabilityKind::Tool);
}

#[tokio::test]
async fn test_ctx_param_passed_through() {
    let ctx = DispatchContext::new(DispatchId::new("test"), OperatorId::new("my-agent"));
    let tool = AgentInfoTool::new();
    let input = json_input(json!({"label": "hello"}));
    let output = tool
        .execute(input, &ctx)
        .await
        .expect("execute must succeed");
    let result: Value = serde_json::from_str(output.message.as_text().unwrap()).unwrap();
    assert_eq!(result["label"], "hello");
    assert_eq!(result["agent"], "my-agent");
}

// ── Test 4: concurrent flag ───────────────────────────────────────────────────

#[skg_tool(
    name = "parallel_op",
    description = "Safe to run concurrently",
    concurrent
)]
async fn parallel_op(value: i64) -> Result<Value, ProtocolError> {
    Ok(json!({"value": value}))
}

#[test]
fn test_concurrent_flag_sets_shared_execution_class() {
    use layer0::capability::ExecutionClass;
    let tool = ParallelOpTool::new();
    let desc = SyncOperator::descriptor(&tool);
    assert_eq!(desc.scheduling.execution_class, ExecutionClass::Shared);
}

#[test]
fn test_default_is_exclusive_execution_class() {
    use layer0::capability::ExecutionClass;
    let tool = GetWeatherTool::new();
    let desc = SyncOperator::descriptor(&tool);
    assert_eq!(desc.scheduling.execution_class, ExecutionClass::Exclusive);
}

// ── Test 5: zero-parameter function ──────────────────────────────────────────

#[skg_tool(name = "ping", description = "Ping the tool")]
async fn ping() -> Result<Value, ProtocolError> {
    Ok(json!({"pong": true}))
}

#[tokio::test]
async fn test_zero_param_execute() {
    let tool = PingTool::new();
    let ctx = test_ctx();
    let input = json_input(json!({}));
    let output = tool
        .execute(input, &ctx)
        .await
        .expect("execute must succeed");
    let result: Value = serde_json::from_str(output.message.as_text().unwrap()).unwrap();
    assert_eq!(result["pong"], true);
}

// ── Test 6: Default trait ─────────────────────────────────────────────────────

#[test]
fn test_default_trait_equivalent_to_new() {
    let a = GetWeatherTool::new();
    let b = GetWeatherTool;
    assert_eq!(
        SyncOperator::descriptor(&a).name,
        SyncOperator::descriptor(&b).name,
    );
}

// ── Test 7: object safety (SyncOperator usable as dyn SyncOperator) ───────────

#[test]
fn test_generated_struct_usable_as_dyn_sync_operator() {
    use std::sync::Arc;
    let _: Arc<dyn SyncOperator> = Arc::new(GetWeatherTool::new());
}
