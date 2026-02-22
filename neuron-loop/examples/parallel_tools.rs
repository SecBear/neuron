//! Example: parallel tool execution.
//!
//! When `parallel_tool_execution` is enabled and the model returns multiple
//! tool calls, they execute concurrently via `join_all`. No API key needed.
//!
//! Run with: `cargo run --example parallel_tools -p neuron-loop`

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use neuron_context::SlidingWindowStrategy;
use neuron_loop::AgentLoop;
use neuron_tool::ToolRegistry;
use neuron_types::*;

// --- Mock provider: returns 3 tool calls, then a final response ---

struct MultiToolProvider {
    call_count: AtomicUsize,
}

impl MultiToolProvider {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }
}

impl Provider for MultiToolProvider {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let call = self.call_count.fetch_add(1, Ordering::SeqCst);

        if call == 0 {
            // First call: return 3 parallel tool calls
            Ok(CompletionResponse {
                id: "resp-1".to_string(),
                model: "mock".to_string(),
                message: Message {
                    role: Role::Assistant,
                    content: vec![
                        ContentBlock::ToolUse {
                            id: "call-a".to_string(),
                            name: "fetch_data".to_string(),
                            input: serde_json::json!({"source": "database"}),
                        },
                        ContentBlock::ToolUse {
                            id: "call-b".to_string(),
                            name: "fetch_data".to_string(),
                            input: serde_json::json!({"source": "cache"}),
                        },
                        ContentBlock::ToolUse {
                            id: "call-c".to_string(),
                            name: "fetch_data".to_string(),
                            input: serde_json::json!({"source": "api"}),
                        },
                    ],
                },
                usage: TokenUsage::default(),
                stop_reason: StopReason::ToolUse,
            })
        } else {
            // Second call: final response
            Ok(CompletionResponse {
                id: "resp-2".to_string(),
                model: "mock".to_string(),
                message: Message::assistant("All 3 data sources fetched successfully."),
                usage: TokenUsage::default(),
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::InvalidRequest("not supported".into()))
    }
}

// --- Tool that records execution timestamps ---

struct FetchDataTool {
    timestamps: Arc<Mutex<Vec<(String, std::time::Instant)>>>,
}

impl Tool for FetchDataTool {
    const NAME: &'static str = "fetch_data";
    type Args = serde_json::Value;
    type Output = String;
    type Error = ToolError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "fetch_data".to_string(),
            title: None,
            description: "Fetch data from a source".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    async fn call(&self, args: Self::Args, _ctx: &ToolContext) -> Result<String, ToolError> {
        let source = args["source"].as_str().unwrap_or("unknown");
        let start = std::time::Instant::now();

        // Simulate work
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        self.timestamps
            .lock()
            .unwrap()
            .push((source.to_string(), start));
        Ok(format!("Data from {source}: [mock data]"))
    }
}

#[tokio::main]
async fn main() {
    let timestamps = Arc::new(Mutex::new(Vec::new()));
    let context = SlidingWindowStrategy::new(100, 100_000);

    let mut tools = ToolRegistry::new();
    tools.register(FetchDataTool {
        timestamps: timestamps.clone(),
    });

    // Enable parallel tool execution
    let mut agent = AgentLoop::builder(MultiToolProvider::new(), context)
        .tools(tools)
        .parallel_tool_execution(true)
        .max_turns(5)
        .build();

    let ctx = ToolContext::default();
    let start = std::time::Instant::now();
    let result = agent
        .run(Message::user("Fetch all data"), &ctx)
        .await
        .unwrap();

    let elapsed = start.elapsed();
    println!("Response: {}", result.response);
    println!("Turns: {}", result.turns);
    println!("Total time: {elapsed:?}");

    // Show that tools ran concurrently
    let ts = timestamps.lock().unwrap();
    println!("\nTool execution times:");
    let first_start = ts.iter().map(|(_, t)| *t).min().unwrap();
    for (source, t) in ts.iter() {
        let offset = t.duration_since(first_start);
        println!("  {source}: started at +{offset:?}");
    }

    // With parallel execution, all 3 should start within ~1ms of each other
    // Total time should be ~200ms (not 600ms)
    println!("\nWith parallel execution, all 3 tools started near-simultaneously.");
    println!("Total time (~200ms) is much less than sequential (~600ms).");
}
