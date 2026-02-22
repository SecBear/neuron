# Runtime

`neuron-runtime` provides production infrastructure for agents: session
persistence, input/output guardrails, structured observability, durable
execution, and sandboxed tool execution.

## Quick Example

```rust,ignore
use std::path::PathBuf;
use neuron_runtime::*;
use neuron_types::Message;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Persist sessions to disk
    let storage = FileSessionStorage::new(PathBuf::from("./sessions"));
    let mut session = Session::new("s-1", PathBuf::from("."));
    session.messages.push(Message::user("Hello"));
    storage.save(&session).await?;

    // Load it back later
    let loaded = storage.load("s-1").await?;
    println!("{} messages", loaded.messages.len());
    Ok(())
}
```

## Sessions

Sessions store conversation message history along with metadata (timestamps,
token usage, custom state). The `SessionStorage` trait defines how sessions
are persisted.

### Session Type

```rust,ignore
use neuron_runtime::Session;

let mut session = Session::new("chat-42", "/home/user/project".into());
session.messages.push(Message::user("What is Rust?"));
session.state.custom.insert("theme".to_string(), serde_json::json!("dark"));
```

A `Session` contains:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Unique session identifier |
| `messages` | `Vec<Message>` | Conversation history |
| `state` | `SessionState` | Working directory, token usage, event count, custom metadata |
| `created_at` | `DateTime<Utc>` | Creation timestamp |
| `updated_at` | `DateTime<Utc>` | Last update timestamp |

`SessionState` holds mutable runtime data: `cwd`, `token_usage`, `event_count`,
and a `custom` map for arbitrary key-value metadata.

### SessionStorage Trait

```rust,ignore
pub trait SessionStorage: Send + Sync {
    async fn save(&self, session: &Session) -> Result<(), StorageError>;
    async fn load(&self, id: &str) -> Result<Session, StorageError>;
    async fn list(&self) -> Result<Vec<SessionSummary>, StorageError>;
    async fn delete(&self, id: &str) -> Result<(), StorageError>;
}
```

Two implementations ship with the crate:

**`InMemorySessionStorage`** -- backed by `Arc<RwLock<HashMap>>`, suitable for
testing and short-lived processes:

```rust,ignore
let storage = InMemorySessionStorage::new();
storage.save(&session).await?;
```

**`FileSessionStorage`** -- one JSON file per session at
`{directory}/{session_id}.json`. Creates the directory on first save:

```rust,ignore
let storage = FileSessionStorage::new(PathBuf::from("./sessions"));
storage.save(&session).await?;
// Creates ./sessions/chat-42.json
```

### Session Summaries

`Session::summary()` returns a lightweight `SessionSummary` without the full
message history -- useful for listing sessions:

```rust,ignore
let summaries = storage.list().await?;
for s in &summaries {
    println!("{}: {} messages, created {}", s.id, s.message_count, s.created_at);
}
```

## Guardrails

Guardrails are safety checks that run on input (before it reaches the LLM) or
output (before it reaches the user).

### GuardrailResult

Every guardrail check returns one of three outcomes:

- **`Pass`** -- input/output is acceptable
- **`Tripwire(reason)`** -- immediately halt execution
- **`Warn(reason)`** -- allow execution but log a warning

### InputGuardrail and OutputGuardrail

```rust,ignore
use std::future::Future;
use neuron_runtime::{InputGuardrail, GuardrailResult};

struct NoSecrets;
impl InputGuardrail for NoSecrets {
    fn check(&self, input: &str) -> impl Future<Output = GuardrailResult> + Send {
        async move {
            if input.contains("API_KEY") || input.contains("sk-") {
                GuardrailResult::Tripwire("Input contains a secret".to_string())
            } else {
                GuardrailResult::Pass
            }
        }
    }
}
```

Output guardrails use the same pattern via the `OutputGuardrail` trait.

### Running Multiple Guardrails

Use `run_input_guardrails` and `run_output_guardrails` to evaluate a sequence.
They return the first non-`Pass` result, or `Pass` if all checks pass:

```rust,ignore
use neuron_runtime::{run_input_guardrails, ErasedInputGuardrail};

let no_secrets = NoSecrets;
let no_sql = NoSqlInjection;
let guardrails: Vec<&dyn ErasedInputGuardrail> = vec![&no_secrets, &no_sql];

let result = run_input_guardrails(&guardrails, user_input).await;
if result.is_tripwire() {
    // Reject the input
}
```

### GuardrailHook

`GuardrailHook` wraps guardrails as an `ObservabilityHook`, integrating them
directly into the agent loop lifecycle:

- Input guardrails fire on `HookEvent::PreLlmCall`
- Output guardrails fire on `HookEvent::PostLlmCall`
- `Tripwire` maps to `HookAction::Terminate`
- `Warn` logs via `tracing::warn!` and returns `HookAction::Continue`
- `Pass` returns `HookAction::Continue`

```rust,ignore
use neuron_runtime::GuardrailHook;
use neuron_loop::AgentLoop;

let hook = GuardrailHook::new()
    .input_guardrail(NoSecrets)
    .output_guardrail(NoProfanity);

let mut agent = AgentLoop::builder(provider, context)
    .tools(registry)
    .build();
agent.add_hook(hook);
```

## TracingHook

`TracingHook` is a concrete `ObservabilityHook` that emits structured
[`tracing`](https://docs.rs/tracing) events for every stage of the agent loop.
Wire it to any `tracing`-compatible subscriber for stdout logging,
OpenTelemetry export, or custom collectors.

```rust,ignore
use neuron_runtime::TracingHook;

let hook = TracingHook::new();
// Add to agent loop: agent.add_hook(hook);
```

`TracingHook` always returns `HookAction::Continue` -- it observes but never
controls execution. It maps 8 hook events to structured spans:

| Event | Level | Span name |
|-------|-------|-----------|
| `LoopIteration` | DEBUG | `neuron.loop.iteration` |
| `PreLlmCall` | DEBUG | `neuron.llm.pre_call` |
| `PostLlmCall` | DEBUG | `neuron.llm.post_call` |
| `PreToolExecution` | DEBUG | `neuron.tool.pre_execution` |
| `PostToolExecution` | DEBUG | `neuron.tool.post_execution` |
| `ContextCompaction` | INFO | `neuron.context.compaction` |
| `SessionStart` | INFO | `neuron.session.start` |
| `SessionEnd` | INFO | `neuron.session.end` |

Set `RUST_LOG=debug` to see all events:

```sh
RUST_LOG=debug cargo run --example tracing_hook -p neuron-runtime
```

## PermissionPolicy

The `PermissionPolicy` trait approves or denies tool calls before execution.
It returns a `PermissionDecision`:

- **`Allow`** -- proceed with the tool call
- **`Deny(reason)`** -- reject the call
- **`Ask(prompt)`** -- ask the user for confirmation

```rust,ignore
use neuron_types::{PermissionPolicy, PermissionDecision};

struct ReadOnlyPolicy;
impl PermissionPolicy for ReadOnlyPolicy {
    fn check(&self, tool_name: &str, _input: &serde_json::Value) -> PermissionDecision {
        match tool_name {
            "read_file" | "list_dir" => PermissionDecision::Allow,
            _ => PermissionDecision::Deny(format!("{tool_name} is not allowed in read-only mode")),
        }
    }
}
```

## DurableContext

`DurableContext` wraps LLM calls and tool execution so durable engines
(Temporal, Restate, Inngest) can journal, replay, and recover from crashes.

### The Trait

```rust,ignore
pub trait DurableContext: Send + Sync {
    async fn execute_llm_call(&self, request: CompletionRequest, options: ActivityOptions) -> Result<CompletionResponse, DurableError>;
    async fn execute_tool(&self, tool_name: &str, input: Value, ctx: &ToolContext, options: ActivityOptions) -> Result<ToolOutput, DurableError>;
    async fn wait_for_signal<T: DeserializeOwned>(&self, signal_name: &str, timeout: Duration) -> Result<Option<T>, DurableError>;
    fn should_continue_as_new(&self) -> bool;
    async fn continue_as_new(&self, state: Value) -> Result<(), DurableError>;
    async fn sleep(&self, duration: Duration);
    fn now(&self) -> DateTime<Utc>;
}
```

### LocalDurableContext

For local development and testing, `LocalDurableContext` passes through to the
provider and tools directly -- no journaling, no replay:

```rust,ignore
use std::sync::Arc;
use neuron_runtime::LocalDurableContext;
use neuron_tool::ToolRegistry;

let provider = Arc::new(my_provider);
let tools = Arc::new(ToolRegistry::new());
let durable = LocalDurableContext::new(provider, tools);

// Use in the agent loop
agent.set_durability(durable);
```

In production, swap `LocalDurableContext` for a Temporal or Restate
implementation. The calling code stays the same.

### ActivityOptions

Controls timeout and retry behavior for durable activities:

```rust,ignore
use neuron_types::{ActivityOptions, RetryPolicy};
use std::time::Duration;

let options = ActivityOptions {
    start_to_close_timeout: Duration::from_secs(30),
    heartbeat_timeout: Some(Duration::from_secs(10)),
    retry_policy: Some(RetryPolicy {
        initial_interval: Duration::from_secs(1),
        backoff_coefficient: 2.0,
        maximum_attempts: 3,
        maximum_interval: Duration::from_secs(30),
        non_retryable_errors: vec!["Authentication".to_string()],
    }),
};
```

## Sandbox

The `Sandbox` trait wraps tool execution with isolation -- filesystem
restrictions, network limits, or container boundaries:

```rust,ignore
use neuron_runtime::{Sandbox, NoOpSandbox};

// NoOpSandbox passes through directly (no isolation)
let sandbox = NoOpSandbox;
let output = sandbox.execute_tool(&*tool, input, &ctx).await?;
```

Implement `Sandbox` for your own isolation strategy:

```rust,ignore
use neuron_runtime::Sandbox;
use neuron_types::{ToolDyn, ToolContext, ToolOutput, SandboxError};

struct DockerSandbox { image: String }

impl Sandbox for DockerSandbox {
    async fn execute_tool(
        &self,
        tool: &dyn ToolDyn,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, SandboxError> {
        // Spawn a container, execute tool inside, return output
        todo!()
    }
}
```

## API Docs

Full API documentation: [neuron-runtime on docs.rs](https://docs.rs/neuron-runtime)
