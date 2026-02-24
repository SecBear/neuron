use neuron_tool::*;
use neuron_types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ReadFileArgs {
    path: String,
}

#[derive(Debug, Serialize)]
struct ReadFileOutput {
    content: String,
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
enum ReadFileError {
    #[error("file not found: {0}")]
    NotFound(String),
}

struct ReadFileTool;

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Args = ReadFileArgs;
    type Output = ReadFileOutput;
    type Error = ReadFileError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            title: None,
            description: "Read a file".into(),
            input_schema: serde_json::to_value(schemars::schema_for!(ReadFileArgs)).unwrap(),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(
        &self,
        args: Self::Args,
        _ctx: &ToolContext,
    ) -> Result<Self::Output, Self::Error> {
        Ok(ReadFileOutput {
            content: format!("contents of {}", args.path),
        })
    }
}

fn test_ctx() -> ToolContext {
    ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test".into(),
        environment: HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    }
}

#[tokio::test]
async fn register_and_execute_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);

    let ctx = test_ctx();
    let result = registry
        .execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx)
        .await
        .unwrap();
    assert!(!result.is_error);
}

#[test]
fn definitions_lists_all_tools() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    let defs = registry.definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "read_file");
}

#[tokio::test]
async fn execute_unknown_tool_returns_not_found() {
    let registry = ToolRegistry::new();
    let ctx = test_ctx();
    let err = registry
        .execute("nonexistent", serde_json::json!({}), &ctx)
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::NotFound(_)));
}

#[tokio::test]
async fn get_returns_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    assert!(registry.get("read_file").is_some());
    assert!(registry.get("nonexistent").is_none());
}

// --- register_dyn test ---

#[tokio::test]
async fn register_dyn_and_execute() {
    // Create a ToolDyn manually (using the blanket impl from Tool)
    let tool: Arc<dyn ToolDyn> = Arc::new(ReadFileTool);
    assert_eq!(tool.name(), "read_file");

    let mut registry = ToolRegistry::new();
    registry.register_dyn(tool);

    // Verify it appears in definitions
    let defs = registry.definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "read_file");

    // Verify it can be looked up
    assert!(registry.get("read_file").is_some());

    // Verify execution works
    let ctx = test_ctx();
    let result = registry
        .execute(
            "read_file",
            serde_json::json!({"path": "/tmp/dyn_test"}),
            &ctx,
        )
        .await
        .unwrap();
    assert!(!result.is_error);

    if let Some(ContentItem::Text(text)) = result.content.first() {
        assert!(
            text.contains("contents of /tmp/dyn_test"),
            "expected tool output, got: {text}"
        );
    } else {
        panic!("expected text content");
    }
}
