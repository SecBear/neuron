//! LocalDurableContext: passthrough DurableContext for local development.
//!
//! No API key needed — uses a mock provider.
//! In production, replace with a Temporal or Restate DurableContext implementation.
//!
//! Run with: cargo run --example local_durable -p neuron-runtime

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use neuron_runtime::LocalDurableContext;
use neuron_tool::ToolRegistry;
use neuron_types::{
    ActivityOptions, CompletionRequest, CompletionResponse, ContentBlock, ContentItem, DurableContext,
    Message, ProviderError, Role, StopReason, StreamHandle, TokenUsage, Tool, ToolContext,
    ToolDefinition,
};
use tokio_util::sync::CancellationToken;

// --- Mock provider ---

/// Returns pre-configured responses in sequence. No network calls.
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

impl neuron_types::Provider for MockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        let response = {
            let mut responses = self.responses.lock().expect("lock poisoned");
            if responses.is_empty() {
                return std::future::ready(Err(ProviderError::Other(
                    "no more mock responses".into(),
                )));
            }
            responses.remove(0)
        };
        std::future::ready(Ok(response))
    }

    fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        std::future::ready(Err(ProviderError::InvalidRequest(
            "streaming not supported in mock".into(),
        )))
    }
}

// --- Simple tool: echo ---

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
            description: "Echoes input text back to the caller".to_string(),
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

// --- Helpers ---

fn text_response(text: &str) -> CompletionResponse {
    CompletionResponse {
        id: "mock-id".to_string(),
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

fn tool_ctx() -> ToolContext {
    ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "local-durable-example".to_string(),
        environment: HashMap::new(),
        cancellation_token: CancellationToken::new(),
        progress_reporter: None,
    }
}

fn activity_opts(secs: u64) -> ActivityOptions {
    ActivityOptions {
        start_to_close_timeout: Duration::from_secs(secs),
        heartbeat_timeout: None,
        retry_policy: None,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Build a LocalDurableContext wrapping a mock provider and tool registry.
    //    In production, swap LocalDurableContext for a Temporal or Restate
    //    implementation — the rest of the code stays identical.
    let provider = Arc::new(MockProvider::new(vec![
        text_response("Rust is a systems programming language focused on safety and speed."),
    ]));

    let mut registry = ToolRegistry::new();
    registry.register(EchoTool);
    let tools = Arc::new(registry);

    let durable = LocalDurableContext::new(provider, tools);

    // 2. Execute an LLM call through the durable context.
    //    LocalDurableContext passes straight through — no journaling.
    //    A Temporal implementation would journal the request/response for replay.
    println!("=== execute_llm_call ===");

    let request = CompletionRequest {
        model: "mock-model".to_string(),
        messages: vec![Message::user("What is Rust?")],
        ..Default::default()
    };

    let response = durable
        .execute_llm_call(request, activity_opts(30))
        .await?;

    println!(
        "  stop_reason: {:?}",
        response.stop_reason
    );
    println!(
        "  input_tokens: {}, output_tokens: {}",
        response.usage.input_tokens, response.usage.output_tokens
    );
    for block in &response.message.content {
        if let ContentBlock::Text(t) = block {
            println!("  reply: {t}");
        }
    }

    // 3. Execute a tool call through the durable context.
    //    Again, LocalDurableContext delegates directly to the ToolRegistry.
    println!("\n=== execute_tool ===");

    let ctx = tool_ctx();
    let tool_output = durable
        .execute_tool(
            "echo",
            serde_json::json!({"text": "hello from durable context"}),
            &ctx,
            activity_opts(10),
        )
        .await?;

    println!("  is_error: {}", tool_output.is_error);
    for item in &tool_output.content {
        if let ContentItem::Text(t) = item {
            println!("  output: {t}");
        }
    }

    // 4. Other DurableContext methods.
    println!("\n=== other DurableContext methods ===");

    let should_new = durable.should_continue_as_new();
    println!("  should_continue_as_new: {should_new}  (always false locally)");

    durable.continue_as_new(serde_json::json!({})).await?;
    println!("  continue_as_new: no-op (returns Ok(()))");

    let now = durable.now();
    println!("  now: {now}");

    println!(
        "\nLocalDurableContext passes through all calls directly.\n\
         Replace it with a Temporal or Restate implementation for production\n\
         durability without changing any call sites."
    );

    Ok(())
}
