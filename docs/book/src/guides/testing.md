# Testing Agents

neuron is designed for testability. Every block -- providers, tools, context
strategies, guardrails -- can be tested independently without real API calls.

## Quick Example

```rust,ignore
use std::sync::Mutex;
use neuron_types::*;

struct MockProvider {
    responses: Mutex<Vec<CompletionResponse>>,
}

impl Provider for MockProvider {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let mut responses = self.responses.lock().unwrap();
        Ok(responses.remove(0))
    }
    async fn complete_stream(&self, _req: CompletionRequest) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::InvalidRequest("mock does not stream".into()))
    }
}
```

## Testing Strategies

### 1. Mock Providers

A mock provider returns fixed `CompletionResponse` values in sequence. This
lets you test agent behavior without network calls or API keys.

**Single-turn response** (model ends the conversation):

```rust,ignore
fn end_turn_response(text: &str) -> CompletionResponse {
    CompletionResponse {
        id: "mock-1".to_string(),
        model: "mock".to_string(),
        message: Message::assistant(text),
        usage: TokenUsage::default(),
        stop_reason: StopReason::EndTurn,
    }
}
```

**Tool-calling response** (model requests a tool call):

```rust,ignore
fn tool_call_response(tool_name: &str, tool_id: &str, args: serde_json::Value) -> CompletionResponse {
    CompletionResponse {
        id: "mock-2".to_string(),
        model: "mock".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: tool_id.to_string(),
                name: tool_name.to_string(),
                input: args,
            }],
        },
        usage: TokenUsage::default(),
        stop_reason: StopReason::ToolUse,
    }
}
```

**Multi-turn mock** -- queue responses to simulate a full conversation:

```rust,ignore
let provider = MockProvider {
    responses: Mutex::new(vec![
        // Turn 1: model calls a tool
        tool_call_response("get_weather", "call-1", serde_json::json!({"city": "Tokyo"})),
        // Turn 2: model responds with the final answer
        end_turn_response("The weather in Tokyo is 72F and sunny."),
    ]),
};
```

### 2. Testing Tools Independently

Tools implement a trait with typed arguments and outputs. Test them directly
without involving a provider or loop:

```rust,ignore
use neuron_types::{Tool, ToolContext};

#[tokio::test]
async fn test_weather_tool() {
    let tool = GetWeather;
    let ctx = ToolContext::default();

    let result = tool.call(WeatherArgs { city: "Tokyo".to_string() }, &ctx).await;
    assert!(result.is_ok());
    assert!(result.unwrap().contains("Tokyo"));
}
```

`ToolContext::default()` provides sensible defaults (cwd from the environment,
empty session ID, fresh cancellation token). Override fields when your tool
depends on them:

```rust,ignore
let ctx = ToolContext {
    session_id: "test-session".to_string(),
    cwd: PathBuf::from("/tmp/test"),
    ..Default::default()
};
```

### 3. Testing Tools via the Registry

To test the full JSON serialization/deserialization path through the
`ToolRegistry`:

```rust,ignore
use neuron_tool::ToolRegistry;
use neuron_types::ToolContext;

#[tokio::test]
async fn test_tool_via_registry() {
    let mut registry = ToolRegistry::new();
    registry.register(GetWeather);

    let ctx = ToolContext::default();
    let input = serde_json::json!({"city": "London"});

    let output = registry.execute("get_weather", input, &ctx).await.unwrap();
    assert!(!output.is_error);

    // Check structured output
    let text = &output.content[0];
    match text {
        neuron_types::ContentItem::Text(t) => assert!(t.contains("London")),
        _ => panic!("expected text content"),
    }
}
```

### 4. Testing Context Strategies

Context strategies are pure functions on message lists. Test them with
synthetic data:

```rust,ignore
use neuron_context::SlidingWindowStrategy;
use neuron_types::{ContextStrategy, Message};

#[tokio::test]
async fn test_sliding_window() {
    let strategy = SlidingWindowStrategy::new(3, 100_000);

    // Create a long conversation
    let messages: Vec<Message> = (0..10)
        .map(|i| Message::user(format!("Message {i}")))
        .collect();

    assert!(strategy.should_compact(&messages, 150_000));

    let compacted = strategy.compact(messages).await.unwrap();
    assert!(compacted.len() <= 3);
}
```

### 5. Testing Guardrails

Guardrails are async functions on strings -- no provider needed:

```rust,ignore
use neuron_runtime::{InputGuardrail, GuardrailResult};

#[tokio::test]
async fn test_no_secrets_guardrail() {
    let guardrail = NoSecrets;

    let result = guardrail.check("What is Rust?").await;
    assert!(result.is_pass());

    let result = guardrail.check("My API_KEY is abc123").await;
    assert!(result.is_tripwire());
}
```

### 6. Testing the Full Agent Loop

Combine a mock provider with real tools to test the complete agent loop:

```rust,ignore
use neuron_loop::AgentLoop;
use neuron_tool::ToolRegistry;
use neuron_context::SlidingWindowStrategy;
use neuron_types::*;

#[tokio::test]
async fn test_agent_loop_with_tool_call() {
    // Set up mock provider with two responses:
    // 1. Model calls the echo tool
    // 2. Model produces a final answer
    let provider = MockProvider {
        responses: Mutex::new(vec![
            tool_call_response("echo", "call-1", serde_json::json!({"text": "hello"})),
            end_turn_response("The echo tool returned: hello"),
        ]),
    };

    let mut tools = ToolRegistry::new();
    tools.register(EchoTool);

    let context = SlidingWindowStrategy::new(10, 100_000);

    let mut agent = AgentLoop::builder(provider, context)
        .tools(tools)
        .system_prompt("You are a test agent.")
        .max_turns(5)
        .build();

    let ctx = ToolContext::default();
    let result = agent.run(Message::user("Echo hello"), &ctx).await.unwrap();

    assert_eq!(result.turns, 2);
    assert!(result.response.contains("hello"));
}
```

### 7. HTTP-Level Integration Tests with wiremock

For testing actual HTTP request/response mapping without calling the real API,
use `wiremock` to stand up a local mock server:

```rust,ignore
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path};
use neuron_provider_openai::OpenAi;
use neuron_types::*;

#[tokio::test]
async fn test_openai_provider_http() {
    let server = MockServer::start().await;

    // Mock the OpenAI completions endpoint
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-123",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hello!" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 }
        })))
        .mount(&server)
        .await;

    let client = OpenAi::new("test-key").base_url(server.uri());

    let response = client.complete(CompletionRequest {
        model: "gpt-4o".to_string(),
        messages: vec![Message::user("Hi")],
        ..Default::default()
    }).await.unwrap();

    assert_eq!(response.stop_reason, StopReason::EndTurn);
}
```

This tests the full serialization/deserialization path through the provider
implementation without any network calls to OpenAI.

## Testing Patterns Summary

| What to test | Approach | Needs API key? |
|-------------|----------|----------------|
| Individual tools | Call `tool.call(args, ctx)` directly | No |
| Tool JSON path | Use `ToolRegistry::execute()` | No |
| Context strategy | Call `should_compact()` / `compact()` with synthetic messages | No |
| Guardrails | Call `guardrail.check(text)` | No |
| Single-turn agent | Mock provider + `AgentLoop::run()` | No |
| Multi-turn agent | Mock provider with queued responses | No |
| Provider HTTP mapping | wiremock + real provider | No |
| End-to-end integration | Real provider + real tools | Yes |

## Tips

- Use `..Default::default()` on `CompletionRequest`, `TokenUsage`, and
  `ToolContext` to avoid breaking tests when new fields are added.
- Keep mock providers simple: `Mutex<Vec<CompletionResponse>>` covers most
  patterns.
- Test `ToolError::ModelRetry` by returning it from a mock tool -- verify the
  loop converts it to an error tool result and the model gets another chance.
- Use `StopReason::EndTurn` for final responses and `StopReason::ToolUse` for
  tool-calling turns in your mock data.

## API Docs

Full API documentation:
- Types: [neuron-types on docs.rs](https://docs.rs/neuron-types)
- Tool registry: [neuron-tool on docs.rs](https://docs.rs/neuron-tool)
- Agent loop: [neuron-loop on docs.rs](https://docs.rs/neuron-loop)
