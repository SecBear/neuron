use neuron_types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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

#[tokio::test]
async fn tool_dyn_blanket_impl() {
    let tool = ReadFileTool;
    let dyn_tool: &dyn ToolDyn = &tool;

    assert_eq!(dyn_tool.name(), "read_file");

    let ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test".into(),
        environment: HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    };

    let input = serde_json::json!({"path": "/tmp/test.txt"});
    let result = dyn_tool.call_dyn(input, &ctx).await.unwrap();
    assert!(!result.is_error);

    // Verify structured_content round-trips
    let value = result.structured_content.unwrap();
    assert!(value.to_string().contains("contents of /tmp/test.txt"));
}

#[tokio::test]
async fn tool_dyn_invalid_input() {
    let tool = ReadFileTool;
    let dyn_tool: &dyn ToolDyn = &tool;

    let ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test".into(),
        environment: HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    };

    let input = serde_json::json!({"wrong_field": 42});
    let result = dyn_tool.call_dyn(input, &ctx).await;
    assert!(result.is_err());
}

#[test]
fn schemars_generates_schema() {
    let schema = schemars::schema_for!(ReadFileArgs);
    let json = serde_json::to_value(schema).unwrap();
    let props = json["properties"].as_object().unwrap();
    assert!(props.contains_key("path"));
}

#[test]
fn tool_definition_from_impl() {
    let tool = ReadFileTool;
    let def = Tool::definition(&tool);
    assert_eq!(def.name, "read_file");
    assert_eq!(def.description, "Read a file");
}
