//! Integration tests for agent-runtime.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use agent_context::SlidingWindowStrategy;
use agent_runtime::{
    FileSessionStorage, InMemorySessionStorage, Session, SessionStorage, SubAgentConfig,
    SubAgentManager,
};
use agent_tool::ToolRegistry;
use agent_types::{
    CompletionRequest, CompletionResponse, ContentBlock, Message, ProviderError, Role, StopReason,
    StreamHandle, SystemPrompt, TokenUsage, Tool, ToolContext, ToolDefinition, ToolOutput,
};
use tokio_util::sync::CancellationToken;

// ============================================================================
// Shared test helpers
// ============================================================================

/// A mock provider that returns pre-configured responses in sequence.
struct MockProvider {
    responses: Mutex<Vec<CompletionResponse>>,
}

impl MockProvider {
    fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl agent_types::Provider for MockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        let response = {
            let mut responses = self.responses.lock().expect("test lock poisoned");
            if responses.is_empty() {
                panic!("MockProvider: no more responses configured");
            }
            responses.remove(0)
        };
        async move { Ok(response) }
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::InvalidRequest(
            "streaming not implemented in mock".into(),
        ))
    }
}

/// A mock tool that echoes its input.
struct EchoTool;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EchoArgs {
    text: String,
}

impl Tool for EchoTool {
    const NAME: &'static str = "echo";
    type Args = EchoArgs;
    type Output = String;
    type Error = std::io::Error;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".to_string(),
            title: Some("Echo".to_string()),
            description: "Echoes input text".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(&self, args: EchoArgs, _ctx: &ToolContext) -> Result<String, std::io::Error> {
        Ok(format!("echo: {}", args.text))
    }
}

/// A second mock tool for testing tool filtering.
struct UpperTool;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct UpperArgs {
    text: String,
}

impl Tool for UpperTool {
    const NAME: &'static str = "upper";
    type Args = UpperArgs;
    type Output = String;
    type Error = std::io::Error;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "upper".to_string(),
            title: Some("Upper".to_string()),
            description: "Uppercases input text".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(&self, args: UpperArgs, _ctx: &ToolContext) -> Result<String, std::io::Error> {
        Ok(args.text.to_uppercase())
    }
}

/// Helper to create a default ToolContext for tests.
fn test_tool_context() -> ToolContext {
    ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test-session".to_string(),
        environment: HashMap::new(),
        cancellation_token: CancellationToken::new(),
        progress_reporter: None,
    }
}

/// Helper to create a simple text CompletionResponse.
fn text_response(text: &str) -> CompletionResponse {
    CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text(text.to_string())],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        stop_reason: StopReason::EndTurn,
    }
}

/// Helper to create a tool_use CompletionResponse.
fn tool_use_response(
    tool_id: &str,
    tool_name: &str,
    input: serde_json::Value,
) -> CompletionResponse {
    CompletionResponse {
        id: "test-id".to_string(),
        model: "mock-model".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: tool_id.to_string(),
                name: tool_name.to_string(),
                input,
            }],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        stop_reason: StopReason::ToolUse,
    }
}

// ============================================================================
// Task 9.2 tests: Session and SessionState types
// ============================================================================

#[test]
fn test_session_creation() {
    let session = Session::new("test-session", PathBuf::from("/tmp"));
    assert_eq!(session.id, "test-session");
    assert!(session.messages.is_empty());
    assert_eq!(session.state.cwd, PathBuf::from("/tmp"));
    assert_eq!(session.state.event_count, 0);
    assert!(session.state.custom.is_empty());
}

#[test]
fn test_session_timestamps() {
    let before = chrono::Utc::now();
    let session = Session::new("ts-test", PathBuf::from("/tmp"));
    let after = chrono::Utc::now();

    assert!(session.created_at >= before);
    assert!(session.created_at <= after);
    assert!(session.updated_at >= before);
    assert!(session.updated_at <= after);
}

#[test]
fn test_session_summary() {
    let mut session = Session::new("summary-test", PathBuf::from("/tmp"));
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    });

    let summary = session.summary();
    assert_eq!(summary.id, "summary-test");
    assert_eq!(summary.message_count, 1);
    assert_eq!(summary.created_at, session.created_at);
    assert_eq!(summary.updated_at, session.updated_at);
}

#[test]
fn test_session_serialize_deserialize() {
    let mut session = Session::new("serde-test", PathBuf::from("/home/user"));
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    });
    session.state.token_usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        ..Default::default()
    };
    session.state.event_count = 3;
    session
        .state
        .custom
        .insert("key".to_string(), serde_json::json!("value"));

    let json = serde_json::to_string(&session).expect("serialize");
    let deserialized: Session = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.id, "serde-test");
    assert_eq!(deserialized.messages.len(), 1);
    assert_eq!(deserialized.state.token_usage.input_tokens, 100);
    assert_eq!(deserialized.state.token_usage.output_tokens, 50);
    assert_eq!(deserialized.state.event_count, 3);
    assert_eq!(
        deserialized.state.custom.get("key"),
        Some(&serde_json::json!("value"))
    );
}

// ============================================================================
// Task 9.3 tests: InMemorySessionStorage
// ============================================================================

#[tokio::test]
async fn test_in_memory_save_and_load() {
    let storage = InMemorySessionStorage::new();
    let session = Session::new("mem-1", PathBuf::from("/tmp"));

    storage.save(&session).await.expect("save should succeed");
    let loaded = storage.load("mem-1").await.expect("load should succeed");

    assert_eq!(loaded.id, "mem-1");
    assert_eq!(loaded.state.cwd, PathBuf::from("/tmp"));
}

#[tokio::test]
async fn test_in_memory_load_not_found() {
    let storage = InMemorySessionStorage::new();
    let err = storage.load("nonexistent").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_in_memory_list() {
    let storage = InMemorySessionStorage::new();
    storage
        .save(&Session::new("list-1", PathBuf::from("/a")))
        .await
        .expect("save");
    storage
        .save(&Session::new("list-2", PathBuf::from("/b")))
        .await
        .expect("save");

    let summaries = storage.list().await.expect("list should succeed");
    assert_eq!(summaries.len(), 2);

    let ids: Vec<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"list-1"));
    assert!(ids.contains(&"list-2"));
}

#[tokio::test]
async fn test_in_memory_delete() {
    let storage = InMemorySessionStorage::new();
    storage
        .save(&Session::new("del-1", PathBuf::from("/tmp")))
        .await
        .expect("save");

    storage.delete("del-1").await.expect("delete should succeed");

    let err = storage.load("del-1").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_in_memory_delete_not_found() {
    let storage = InMemorySessionStorage::new();
    let err = storage.delete("nope").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_in_memory_save_overwrites() {
    let storage = InMemorySessionStorage::new();
    let mut session = Session::new("overwrite", PathBuf::from("/tmp"));
    storage.save(&session).await.expect("save");

    session.state.event_count = 42;
    storage.save(&session).await.expect("save again");

    let loaded = storage.load("overwrite").await.expect("load");
    assert_eq!(loaded.state.event_count, 42);
}

// ============================================================================
// Task 9.4 tests: FileSessionStorage
// ============================================================================

#[tokio::test]
async fn test_file_save_and_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    let mut session = Session::new("file-1", PathBuf::from("/work"));
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello file".to_string())],
    });

    storage.save(&session).await.expect("save should succeed");

    // Verify JSON file exists
    let file_path = dir.path().join("file-1.json");
    assert!(file_path.exists());

    let loaded = storage.load("file-1").await.expect("load should succeed");
    assert_eq!(loaded.id, "file-1");
    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(loaded.state.cwd, PathBuf::from("/work"));
}

#[tokio::test]
async fn test_file_load_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    let err = storage.load("nonexistent").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_file_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    storage
        .save(&Session::new("flist-1", PathBuf::from("/a")))
        .await
        .expect("save");
    storage
        .save(&Session::new("flist-2", PathBuf::from("/b")))
        .await
        .expect("save");

    let summaries = storage.list().await.expect("list");
    assert_eq!(summaries.len(), 2);

    let ids: Vec<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"flist-1"));
    assert!(ids.contains(&"flist-2"));
}

#[tokio::test]
async fn test_file_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    storage
        .save(&Session::new("fdel-1", PathBuf::from("/tmp")))
        .await
        .expect("save");
    storage.delete("fdel-1").await.expect("delete");

    let err = storage.load("fdel-1").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
    assert!(!dir.path().join("fdel-1.json").exists());
}

#[tokio::test]
async fn test_file_delete_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    let err = storage.delete("nope").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_file_creates_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("nested").join("sessions");
    let storage = FileSessionStorage::new(nested.clone());

    storage
        .save(&Session::new("nested-1", PathBuf::from("/tmp")))
        .await
        .expect("save should create nested dirs");

    assert!(nested.join("nested-1.json").exists());
}

#[tokio::test]
async fn test_file_list_empty_nonexistent_dir() {
    let storage = FileSessionStorage::new(PathBuf::from("/tmp/nonexistent_agent_runtime_test"));
    let summaries = storage.list().await.expect("list should succeed");
    assert!(summaries.is_empty());
}

// ============================================================================
// Task 9.5 tests: SubAgentConfig and SubAgentManager
// ============================================================================

#[test]
fn test_sub_agent_config_defaults() {
    let config = SubAgentConfig::new(SystemPrompt::Text("You are a helper.".to_string()));
    assert_eq!(config.max_depth, 1);
    assert!(config.max_turns.is_none());
    assert!(config.tools.is_empty());
    assert!(config.model.is_none());
}

#[test]
fn test_sub_agent_config_builder() {
    let config = SubAgentConfig::new(SystemPrompt::Text("Helper".to_string()))
        .with_tools(vec!["echo".to_string()])
        .with_max_depth(3)
        .with_max_turns(10);

    assert_eq!(config.max_depth, 3);
    assert_eq!(config.max_turns, Some(10));
    assert_eq!(config.tools, vec!["echo"]);
}

#[tokio::test]
async fn test_sub_agent_spawn_basic() {
    let provider = MockProvider::new(vec![text_response("Sub-agent done")]);
    let context = SlidingWindowStrategy::new(10, 100_000);

    let mut parent_tools = ToolRegistry::new();
    parent_tools.register(EchoTool);

    let mut manager = SubAgentManager::new();
    manager.register(
        "helper",
        SubAgentConfig::new(SystemPrompt::Text("You help.".to_string()))
            .with_tools(vec!["echo".to_string()]),
    );

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Do something".to_string())],
    };

    let result = manager
        .spawn(
            "helper",
            provider,
            context,
            &parent_tools,
            user_msg,
            &test_tool_context(),
            0,
        )
        .await
        .expect("spawn should succeed");

    assert_eq!(result.response, "Sub-agent done");
}

#[tokio::test]
async fn test_sub_agent_not_found() {
    let provider = MockProvider::new(vec![]);
    let context = SlidingWindowStrategy::new(10, 100_000);
    let parent_tools = ToolRegistry::new();
    let manager = SubAgentManager::new();

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    };

    let err = manager
        .spawn(
            "nonexistent",
            provider,
            context,
            &parent_tools,
            user_msg,
            &test_tool_context(),
            0,
        )
        .await
        .unwrap_err();

    assert!(matches!(err, agent_types::SubAgentError::NotFound(_)));
}

#[tokio::test]
async fn test_sub_agent_max_depth_exceeded() {
    let provider = MockProvider::new(vec![]);
    let context = SlidingWindowStrategy::new(10, 100_000);
    let parent_tools = ToolRegistry::new();

    let mut manager = SubAgentManager::new();
    manager.register(
        "helper",
        SubAgentConfig::new(SystemPrompt::Text("Helper".to_string())).with_max_depth(2),
    );

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    };

    // current_depth = 2, max_depth = 2 -> exceeded
    let err = manager
        .spawn(
            "helper",
            provider,
            context,
            &parent_tools,
            user_msg,
            &test_tool_context(),
            2,
        )
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        agent_types::SubAgentError::MaxDepthExceeded(2)
    ));
}

#[tokio::test]
async fn test_sub_agent_tool_filtering() {
    // Sub-agent only gets "echo", not "upper"
    let provider = MockProvider::new(vec![
        tool_use_response("call-1", "echo", serde_json::json!({"text": "hi"})),
        text_response("Done"),
    ]);
    let context = SlidingWindowStrategy::new(10, 100_000);

    let mut parent_tools = ToolRegistry::new();
    parent_tools.register(EchoTool);
    parent_tools.register(UpperTool);

    let mut manager = SubAgentManager::new();
    manager.register(
        "echo-only",
        SubAgentConfig::new(SystemPrompt::Text("Only echo".to_string()))
            .with_tools(vec!["echo".to_string()]),
    );

    let user_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Echo something".to_string())],
    };

    let result = manager
        .spawn(
            "echo-only",
            provider,
            context,
            &parent_tools,
            user_msg,
            &test_tool_context(),
            0,
        )
        .await
        .expect("spawn should succeed");

    assert_eq!(result.response, "Done");
    assert_eq!(result.turns, 2);
}

// ============================================================================
// Task 9.6 tests: spawn_parallel
// ============================================================================

#[tokio::test]
async fn test_sub_agent_spawn_parallel() {
    let mut parent_tools = ToolRegistry::new();
    parent_tools.register(EchoTool);

    let mut manager = SubAgentManager::new();
    manager.register(
        "helper",
        SubAgentConfig::new(SystemPrompt::Text("You help.".to_string()))
            .with_tools(vec!["echo".to_string()]),
    );

    let tasks = vec![
        (
            "helper".to_string(),
            MockProvider::new(vec![text_response("Result 1")]),
            SlidingWindowStrategy::new(10, 100_000),
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("Task 1".to_string())],
            },
        ),
        (
            "helper".to_string(),
            MockProvider::new(vec![text_response("Result 2")]),
            SlidingWindowStrategy::new(10, 100_000),
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("Task 2".to_string())],
            },
        ),
        (
            "helper".to_string(),
            MockProvider::new(vec![text_response("Result 3")]),
            SlidingWindowStrategy::new(10, 100_000),
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text("Task 3".to_string())],
            },
        ),
    ];

    let results = manager
        .spawn_parallel(tasks, &parent_tools, &test_tool_context(), 0)
        .await;

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].as_ref().unwrap().response, "Result 1");
    assert_eq!(results[1].as_ref().unwrap().response, "Result 2");
    assert_eq!(results[2].as_ref().unwrap().response, "Result 3");
}

// ============================================================================
// Task 9.7 tests: Guardrails
// ============================================================================

use agent_runtime::{
    run_input_guardrails, run_output_guardrails, ErasedInputGuardrail, ErasedOutputGuardrail,
    GuardrailResult, InputGuardrail, OutputGuardrail,
};

/// Input guardrail that rejects messages containing "DROP TABLE".
struct SqlInjectionGuard;

impl InputGuardrail for SqlInjectionGuard {
    fn check(&self, input: &str) -> impl Future<Output = GuardrailResult> + Send {
        let result = if input.contains("DROP TABLE") {
            GuardrailResult::Tripwire("SQL injection detected".to_string())
        } else {
            GuardrailResult::Pass
        };
        std::future::ready(result)
    }
}

/// Output guardrail that flags secrets in output.
struct SecretLeakGuard;

impl OutputGuardrail for SecretLeakGuard {
    fn check(&self, output: &str) -> impl Future<Output = GuardrailResult> + Send {
        let result = if output.contains("sk-") || output.contains("API_KEY=") {
            GuardrailResult::Tripwire("Secret detected in output".to_string())
        } else if output.contains("password") {
            GuardrailResult::Warn("Output mentions 'password'".to_string())
        } else {
            GuardrailResult::Pass
        };
        std::future::ready(result)
    }
}

#[tokio::test]
async fn test_input_guardrail_pass() {
    let guard = SqlInjectionGuard;
    let result = guard.check("SELECT * FROM users").await;
    assert!(result.is_pass());
}

#[tokio::test]
async fn test_input_guardrail_tripwire() {
    let guard = SqlInjectionGuard;
    let result = guard.check("DROP TABLE users").await;
    assert!(result.is_tripwire());
    match result {
        GuardrailResult::Tripwire(reason) => {
            assert!(reason.contains("SQL injection"));
        }
        other => panic!("expected Tripwire, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_output_guardrail_tripwire_on_secret() {
    let guard = SecretLeakGuard;
    let result = guard.check("Here is the key: sk-abc123").await;
    assert!(result.is_tripwire());
}

#[tokio::test]
async fn test_output_guardrail_warn() {
    let guard = SecretLeakGuard;
    let result = guard.check("Please update your password").await;
    assert!(result.is_warn());
    match result {
        GuardrailResult::Warn(msg) => {
            assert!(msg.contains("password"));
        }
        other => panic!("expected Warn, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_output_guardrail_pass() {
    let guard = SecretLeakGuard;
    let result = guard.check("Everything looks good").await;
    assert!(result.is_pass());
}

#[tokio::test]
async fn test_run_input_guardrails_all_pass() {
    let guard = SqlInjectionGuard;
    let guardrails: Vec<&dyn ErasedInputGuardrail> = vec![&guard];
    let result = run_input_guardrails(&guardrails, "safe input").await;
    assert!(result.is_pass());
}

#[tokio::test]
async fn test_run_input_guardrails_first_tripwire_stops() {
    let guard = SqlInjectionGuard;
    let guardrails: Vec<&dyn ErasedInputGuardrail> = vec![&guard];
    let result = run_input_guardrails(&guardrails, "DROP TABLE users").await;
    assert!(result.is_tripwire());
}

#[tokio::test]
async fn test_run_output_guardrails_all_pass() {
    let guard = SecretLeakGuard;
    let guardrails: Vec<&dyn ErasedOutputGuardrail> = vec![&guard];
    let result = run_output_guardrails(&guardrails, "clean output").await;
    assert!(result.is_pass());
}

#[tokio::test]
async fn test_run_output_guardrails_tripwire() {
    let guard = SecretLeakGuard;
    let guardrails: Vec<&dyn ErasedOutputGuardrail> = vec![&guard];
    let result = run_output_guardrails(&guardrails, "key is sk-secret").await;
    assert!(result.is_tripwire());
}
