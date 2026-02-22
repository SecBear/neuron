# neuron: Design Document

**Date:** 2026-02-21
**Status:** Post-validation — ready for implementation planning
**Rust edition:** 2024 (resolver 3, minimum Rust 1.90)

---

## 1. Vision

`neuron` is a set of independent, composable Rust crates that
provide the building blocks for constructing AI agent systems. Not a framework.
Not an SDK. Building blocks.

Anyone can pick the blocks they need and compose them into an agent loop, an
SDK, a CLI, a TUI, a GUI, or a workflow orchestration engine. Each block is its
own repository, its own crate, publishable independently.

### Design principles

1. **Composable, not opinionated.** Blocks compose via traits. No block dictates
   how you build your agent.
2. **The loop is boring.** The agentic while loop is ~300 lines. All value lives
   in the blocks around it: context, tools, runtime.
3. **Rust type safety.** Invalid states are unrepresentable. Compile-time
   verification of tool schemas, message structure, provider contracts.
4. **Built for agents to work on.** Flat files, obvious names, inline docs, no
   hidden control flow.
5. **Provider-agnostic from the ground up.** The `Provider` trait lives in the
   types crate. Any LLM — API, local, remote — just implements the trait.
6. **Durability is wrapping, not observing.** `DurableContext` wraps side effects
   so durable engines can journal, replay, and recover. Separate
   `ObservabilityHook` for logging/metrics/telemetry.

### Rust conventions

- Native async in traits: `-> impl Future<Output = T> + WasmCompatSend`
- No `#[async_trait]` — use Rust 2024 native async
- `WasmCompatSend`/`WasmCompatSync` conditional bounds for WASM compatibility
- `schemars` for JSON Schema derivation
- `thiserror` for error types, 2 levels max
- `IntoFuture` on builders so `.await` sends the request

---

## 2. Prior art

See CLAUDE.md for the full landscape comparison. Summary:

| Framework | What we take | What we leave |
|-----------|-------------|---------------|
| **Rig** | Native async traits, `ToolDyn` type erasure, `IntoFuture`, `WasmCompatSend`, `schemars`, hook control flow (Continue/Skip/Terminate) | Clone on models, OneOrMany, ToolServer+mpsc, variant-per-role messages |
| **ADK-Rust** | Telemetry as cross-cutting concern, umbrella crate pattern | Fat core, all-providers-in-one-crate, stringly-typed callbacks |
| **Claude Code** | Loop pattern, compaction triggers, tool middleware, sub-agent isolation, system reminders | TypeScript monolith |
| **Pydantic AI** | Generics, ModelRetry, step-by-step iter() | pydantic-graph FSM |
| **OpenAI Agents SDK** | Handoff protocol, guardrails with tripwires, streaming events | Single package |
| **Temporal** | Workflow/Activity model, Signals, Continue-As-New | SDK wrapping required (not hooks) |
| **tower/axum** | `from_fn` middleware pattern (identical to our ToolMiddleware) | Service/Layer boilerplate, poll_ready |

---

## 3. Block decomposition

### 3.1 Overview

```
neuron/
|
|-- neuron-types/                      # Shared types, traits, serde
|-- neuron-provider-anthropic/         # Anthropic Messages API
|-- neuron-provider-openai/            # OpenAI Chat/Responses API
|-- neuron-provider-ollama/            # Ollama (local/remote)
|-- neuron-tool/                       # Tool registry + middleware pipeline
|-- neuron-mcp/                        # MCP client + server (wraps rmcp)
|-- neuron-context/                    # Context engine (compaction, memory)
|-- neuron-loop/                       # The agentic while loop
|-- neuron-runtime/                    # Sub-agents, sessions, DurableContext, guardrails
+-- neuron/                     # Umbrella re-export (build LAST)
```

### 3.2 Dependency graph

```
neuron-types                     (zero internal deps, the foundation)
    ^
    |-- neuron-provider-*        (each implements Provider trait)
    |-- neuron-tool              (Tool trait, registry, middleware)
    |-- neuron-mcp               (wraps rmcp, bridges to Tool trait)
    +-- neuron-context           (+ optional Provider for summarization)
            ^
        neuron-loop              (composes provider + tool + context)
            ^
        neuron-runtime           (sub-agents, sessions, DurableContext, guardrails)
            ^
        neuron            (umbrella, feature-gated re-exports)
            ^
        YOUR PROJECTS           (sdk, cli, tui, gui, gh-aw)
```

Rule: arrows only point up. No circular dependencies.

### 3.3 Build order

1. `neuron-types` — foundation, everything depends on this
2. `neuron-tool` — tool registry and middleware, needed early for testing
3. `neuron-context` — compaction strategies
4. `neuron-provider-anthropic` — first real provider, validates Provider trait
5. `neuron-loop` — composes the above, validates the composition model
6. `neuron-mcp` — wraps rmcp, bridges MCP tools to ToolRegistry
7. `neuron-provider-openai` — second provider, validates trait generality
8. `neuron-provider-ollama` — local inference provider
9. `neuron-runtime` — DurableContext, sessions, sub-agents, guardrails
10. `neuron` — umbrella crate, last

### 3.4 Size estimates

| Block | Purpose | Est. lines |
|-------|---------|------------|
| `neuron-types` | Shared types, traits, serde | ~800 |
| `neuron-provider-anthropic` | Anthropic Messages API | ~800 |
| `neuron-provider-openai` | OpenAI Chat/Responses API | ~900 |
| `neuron-provider-ollama` | Ollama API | ~500 |
| `neuron-tool` | Tool registry, middleware, derive macro | ~1500 |
| `neuron-mcp` | MCP client + server (wrapping rmcp) | ~1200 |
| `neuron-context` | Compaction, memory, token mgmt | ~1200 |
| `neuron-loop` | The agentic while loop | ~400 |
| `neuron-runtime` | DurableContext, sub-agents, sessions, guardrails | ~2500 |
| `neuron` | Umbrella re-exports | ~100 |

---

## 4. Block specifications

### 4.1 `neuron-types` — The lingua franca

Zero logic. Pure types + traits + serde. Every other block depends on this.

#### Messages

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role { User, Assistant, System }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    Text(String),
    Thinking { thinking: String, signature: String },
    RedactedThinking { data: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult {
        tool_use_id: String,
        content: Vec<ContentItem>,
        is_error: bool,
    },
    Image { source: ImageSource },
    Document { source: DocumentSource },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentItem {
    Text(String),
    Image { source: ImageSource },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentSource {
    Base64Pdf { data: String },
    PlainText { data: String },
    Url { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}
```

Conversation state is `Vec<Message>`. Flat. No threading.

#### Provider trait

```rust
pub trait Provider: WasmCompatSend + WasmCompatSync {
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + WasmCompatSend;

    fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + WasmCompatSend;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub system: Option<SystemPrompt>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stop_sequences: Vec<String>,
    pub tool_choice: Option<ToolChoice>,
    pub response_format: Option<ResponseFormat>,
    pub thinking: Option<ThinkingConfig>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemPrompt {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemBlock {
    pub text: String,
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    pub ttl: Option<CacheTtl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheTtl { FiveMinutes, OneHour }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Specific { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseFormat {
    Text,
    JsonObject,
    JsonSchema { name: String, schema: serde_json::Value, strict: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThinkingConfig {
    Enabled { budget_tokens: usize },
    Disabled,
    Adaptive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReasoningEffort { None, Low, Medium, High }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub model: String,
    pub message: Message,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    ContentFilter,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cache_read_tokens: Option<usize>,
    pub cache_creation_tokens: Option<usize>,
    pub reasoning_tokens: Option<usize>,
}
```

#### Tool trait (strongly typed + type-erased)

```rust
/// Strongly-typed tool trait. Implement this for your tools.
pub trait Tool: WasmCompatSend + WasmCompatSync {
    const NAME: &'static str;
    type Args: DeserializeOwned + schemars::JsonSchema + WasmCompatSend;
    type Output: Serialize;
    type Error: std::error::Error + WasmCompatSend + 'static;

    fn definition(&self) -> ToolDefinition;
    fn call(
        &self,
        args: Self::Args,
        ctx: &ToolContext,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + WasmCompatSend;
}

/// Type-erased tool for dynamic dispatch. Blanket-implemented for all Tool impls.
pub trait ToolDyn: WasmCompatSend + WasmCompatSync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    fn call_dyn(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> WasmBoxedFuture<'_, Result<ToolOutput, ToolError>>;
}

// Blanket impl: any Tool automatically becomes a ToolDyn.
// Handles deserialization of Args and serialization of Output.
impl<T: Tool> ToolDyn for T { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub title: Option<String>,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub annotations: Option<ToolAnnotations>,
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAnnotations {
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    pub idempotent_hint: Option<bool>,
    pub open_world_hint: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: Vec<ContentItem>,
    pub structured_content: Option<serde_json::Value>,
    pub is_error: bool,
}

pub struct ToolContext {
    pub cwd: PathBuf,
    pub session_id: String,
    pub environment: HashMap<String, String>,
    pub cancellation_token: CancellationToken,
    pub progress_reporter: Option<Arc<dyn ProgressReporter>>,
}

pub trait ProgressReporter: WasmCompatSend + WasmCompatSync {
    fn report(&self, progress: f64, total: Option<f64>, message: Option<&str>);
}
```

#### Stream events

```rust
#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
    SignatureDelta(String),
    ToolUseStart { id: String, name: String },
    ToolUseInputDelta { id: String, delta: String },
    ToolUseEnd { id: String },
    MessageComplete(Message),
    Usage(TokenUsage),
    Error(AgentError),
}

pub struct StreamHandle {
    pub receiver: Pin<Box<dyn Stream<Item = StreamEvent> + Send>>,
}
```

#### Context strategy trait

```rust
pub trait ContextStrategy: WasmCompatSend + WasmCompatSync {
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool;
    fn compact(
        &self,
        messages: Vec<Message>,
    ) -> impl Future<Output = Result<Vec<Message>, ContextError>> + WasmCompatSend;
    fn token_estimate(&self, messages: &[Message]) -> usize;
}
```

#### Observability hooks

For logging, metrics, telemetry. Does NOT control execution (use DurableContext
for that).

```rust
pub enum HookEvent<'a> {
    LoopIteration { turn: usize },
    PreLlmCall { request: &'a CompletionRequest },
    PostLlmCall { response: &'a CompletionResponse },
    PreToolExecution { tool_name: &'a str, input: &'a serde_json::Value },
    PostToolExecution { tool_name: &'a str, output: &'a ToolOutput },
    ContextCompaction { old_tokens: usize, new_tokens: usize },
    SessionStart { session_id: &'a str },
    SessionEnd { session_id: &'a str },
}

pub trait ObservabilityHook: WasmCompatSend + WasmCompatSync {
    fn on_event(
        &self,
        event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + WasmCompatSend;
}

pub enum HookAction {
    Continue,
    Skip { reason: String },
    Terminate { reason: String },
}
```

#### Durable context

Wraps side effects for durable execution engines (Temporal, Restate, Inngest).
When present, the loop calls through this instead of directly calling
provider/tools.

```rust
pub trait DurableContext: WasmCompatSend + WasmCompatSync {
    fn execute_llm_call(
        &self,
        request: CompletionRequest,
        options: ActivityOptions,
    ) -> impl Future<Output = Result<CompletionResponse, DurableError>> + WasmCompatSend;

    fn execute_tool(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
        options: ActivityOptions,
    ) -> impl Future<Output = Result<ToolOutput, DurableError>> + WasmCompatSend;

    fn wait_for_signal<T: DeserializeOwned + WasmCompatSend>(
        &self,
        signal_name: &str,
        timeout: Duration,
    ) -> impl Future<Output = Result<Option<T>, DurableError>> + WasmCompatSend;

    fn should_continue_as_new(&self) -> bool;

    fn continue_as_new(
        &self,
        state: SessionState,
    ) -> impl Future<Output = Result<(), DurableError>> + WasmCompatSend;

    fn sleep(
        &self,
        duration: Duration,
    ) -> impl Future<Output = ()> + WasmCompatSend;

    fn now(&self) -> chrono::DateTime<chrono::Utc>;
}

pub struct ActivityOptions {
    pub start_to_close_timeout: Duration,
    pub heartbeat_timeout: Option<Duration>,
    pub retry_policy: Option<RetryPolicy>,
}

pub struct RetryPolicy {
    pub initial_interval: Duration,
    pub backoff_coefficient: f64,
    pub maximum_attempts: u32,
    pub maximum_interval: Duration,
    pub non_retryable_errors: Vec<String>,
}
```

#### Permission policy

```rust
pub trait PermissionPolicy: WasmCompatSend + WasmCompatSync {
    fn check(&self, tool_name: &str, input: &serde_json::Value) -> PermissionDecision;
}

pub enum PermissionDecision {
    Allow,
    Deny(String),
    Ask(String),
}
```

#### Error types

```rust
// Provider errors — separate retryable from terminal
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    // Retryable
    #[error("network error: {0}")]
    Network(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("rate limited, retry after {retry_after:?}")]
    RateLimit { retry_after: Option<Duration> },
    #[error("model loading: {0}")]
    ModelLoading(String),
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    // Terminal
    #[error("authentication failed: {0}")]
    Authentication(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("insufficient resources: {0}")]
    InsufficientResources(String),

    // Catch-all
    #[error("stream error: {0}")]
    StreamError(String),
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("execution failed: {0}")]
    ExecutionFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("cancelled")]
    Cancelled,
}

#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("compaction failed: {0}")]
    CompactionFailed(String),
    #[error("provider error during summarization: {0}")]
    Provider(#[from] ProviderError),
}

#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error("tool error: {0}")]
    Tool(#[from] ToolError),
    #[error("context error: {0}")]
    Context(#[from] ContextError),
    #[error("max turns reached ({0})")]
    MaxTurns(usize),
    #[error("terminated by hook: {0}")]
    HookTerminated(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DurableError {
    #[error("activity failed: {0}")]
    ActivityFailed(String),
    #[error("workflow cancelled")]
    Cancelled,
    #[error("signal timeout")]
    SignalTimeout,
    #[error("continue as new: {0}")]
    ContinueAsNew(String),
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("connection failed: {0}")]
    Connection(String),
    #[error("initialization failed: {0}")]
    Initialization(String),
    #[error("tool call failed: {0}")]
    ToolCall(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}
```

#### WASM compatibility

```rust
// In non-WASM builds:
pub trait WasmCompatSend: Send {}
impl<T: Send> WasmCompatSend for T {}
pub trait WasmCompatSync: Sync {}
impl<T: Sync> WasmCompatSync for T {}
pub type WasmBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// In WASM builds (cfg(target_arch = "wasm32")):
pub trait WasmCompatSend {}
impl<T> WasmCompatSend for T {}
pub trait WasmCompatSync {}
impl<T> WasmCompatSync for T {}
pub type WasmBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
```

### 4.2 `neuron-provider-*` — Provider implementations

Each provider is its own crate implementing `Provider` from `neuron-types`.
Depends only on `neuron-types` + `reqwest` + `serde` + `futures`.

**Anthropic** (`neuron-provider-anthropic`):
- Messages API (`/v1/messages`)
- Streaming via SSE
- Prompt caching (`cache_control` on system blocks, content blocks, tools)
- Extended thinking (Thinking/RedactedThinking content blocks)
- Model constants (`claude-sonnet-4-20250514`, `claude-opus-4-20250514`, etc.)
- Maps `SystemPrompt::Blocks` to array-form system parameter

**OpenAI** (`neuron-provider-openai`):
- Chat Completions API (`/v1/chat/completions`)
- Streaming via SSE
- Structured output (`response_format: json_schema`)
- Reasoning models (`reasoning_effort` maps to API parameter)
- Maps `SystemPrompt` to `role: "developer"` message
- Parallel tool calls (multiple `ToolUse` blocks in one response)
- `extra` field forwarded as additional request body fields

**Ollama** (`neuron-provider-ollama`):
- Chat API (`/api/chat`)
- Streaming via newline-delimited JSON (not SSE)
- Maps `max_tokens` to `options.num_predict`, `temperature` to `options.temperature`
- Synthesizes tool use IDs (Ollama does not provide them)
- `extra` field forwarded into `options` object
- `keep_alive` as provider constructor config, not per-request

### 4.3 `neuron-tool` — Tool system

Registry, middleware pipeline, derive macro.

#### Tool registry

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolDyn>>,
    global_middleware: Vec<Arc<dyn ToolMiddleware>>,
    tool_middleware: HashMap<String, Vec<Arc<dyn ToolMiddleware>>>,
}

impl ToolRegistry {
    pub fn new() -> Self;
    pub fn register<T: Tool + 'static>(&mut self, tool: T);
    pub fn register_dyn(&mut self, tool: Arc<dyn ToolDyn>);
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolDyn>>;
    pub fn definitions(&self) -> Vec<ToolDefinition>;
    pub fn add_middleware(&mut self, m: impl ToolMiddleware + 'static) -> &mut Self;
    pub fn add_tool_middleware(&mut self, tool: &str, m: impl ToolMiddleware + 'static) -> &mut Self;
    pub fn execute(
        &self,
        name: &str,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> impl Future<Output = Result<ToolOutput, ToolError>> + Send;
}
```

#### Middleware

```rust
pub trait ToolMiddleware: WasmCompatSend + WasmCompatSync {
    fn process(
        &self,
        call: &ToolCall,
        ctx: &ToolContext,
        next: Next<'_>,
    ) -> impl Future<Output = Result<ToolOutput, ToolError>> + WasmCompatSend;
}

/// Remaining middleware chain + tool. Consumed on call to prevent double-invoke.
pub struct Next<'a> { /* internal */ }
impl<'a> Next<'a> {
    pub async fn run(self, call: &ToolCall, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Convenience: create middleware from a closure (like axum's from_fn).
pub fn tool_middleware_fn<F, Fut>(f: F) -> impl ToolMiddleware
where
    F: Fn(&ToolCall, &ToolContext, Next<'_>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<ToolOutput, ToolError>> + Send;
```

Built-in middleware:
- `SchemaValidator` — validate input against `ToolDefinition.input_schema`
- `PermissionChecker` — delegates to `PermissionPolicy` trait
- `OutputFormatter` — truncate/format output for model consumption

#### Derive macro

```rust
#[derive(Tool, schemars::JsonSchema)]
#[tool(name = "read_file", description = "Read contents of a file")]
struct ReadFile {
    /// Absolute path to the file to read
    path: String,
    /// Maximum number of lines to read
    #[serde(default = "default_max_lines")]
    max_lines: Option<usize>,
}

// Generated: impl Tool for ReadFile with input_schema from JsonSchema derive
```

### 4.4 `neuron-mcp` — MCP integration

Wraps `rmcp` (official Rust MCP SDK, 3.8M downloads). Does NOT reimplement the
protocol. Our value-add: bridging MCP to our `Tool`/`ToolDyn` trait and
providing ergonomic lifecycle management.

**Depends on:** `neuron-types`, `rmcp` (with features: `client`, `server`,
`transport-io`, `transport-child-process`,
`transport-streamable-http-client`, `transport-streamable-http-server`)

#### Client

```rust
pub struct McpClient {
    inner: rmcp::Client,
    server_info: ServerInfo,
}

impl McpClient {
    /// Connect via stdio (spawn child process). Handles initialization.
    pub async fn connect_stdio(command: &str, args: &[&str]) -> Result<Self, McpError>;

    /// Connect via Streamable HTTP (current spec, replaces deprecated SSE).
    pub async fn connect_http(url: &str) -> Result<Self, McpError>;

    // Tools
    pub async fn list_tools(&self, cursor: Option<&str>) -> Result<PaginatedList<ToolDefinition>, McpError>;
    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<ToolOutput, McpError>;

    /// Bridge: wrap all MCP tools as ToolDyn for use in ToolRegistry.
    pub async fn discover_tools(&self) -> Result<Vec<Arc<dyn ToolDyn>>, McpError>;

    // Resources
    pub async fn list_resources(&self, cursor: Option<&str>) -> Result<PaginatedList<McpResource>, McpError>;
    pub async fn read_resource(&self, uri: &str) -> Result<Vec<ResourceContent>, McpError>;

    // Prompts
    pub async fn list_prompts(&self, cursor: Option<&str>) -> Result<PaginatedList<McpPrompt>, McpError>;
    pub async fn get_prompt(&self, name: &str, args: HashMap<String, String>) -> Result<PromptResult, McpError>;
}

pub struct PaginatedList<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

/// Handler for server-to-client requests (sampling, roots, elicitation).
pub trait McpClientHandler: WasmCompatSend + WasmCompatSync {
    fn handle_sampling_request(
        &self,
        req: SamplingRequest,
    ) -> impl Future<Output = Result<SamplingResponse, McpError>> + WasmCompatSend;
}
```

#### Server

```rust
pub struct McpServer {
    tool_registry: ToolRegistry,
}

impl McpServer {
    pub fn new(registry: ToolRegistry) -> Self;
    pub async fn serve_stdio(&self) -> Result<(), McpError>;
    pub async fn serve_http(&self, addr: SocketAddr) -> Result<(), McpError>;
}
```

#### Bridge (MCP tool -> ToolDyn)

```rust
/// Wraps an MCP tool from a remote server as a ToolDyn.
struct McpToolBridge {
    client: Arc<McpClient>,
    definition: ToolDefinition,
}

impl ToolDyn for McpToolBridge {
    fn name(&self) -> &str { &self.definition.name }
    fn definition(&self) -> ToolDefinition { self.definition.clone() }
    fn call_dyn(&self, input: Value, ctx: &ToolContext) -> WasmBoxedFuture<'_, Result<ToolOutput, ToolError>> {
        // Forwards to client.call_tool()
        // Maps progress/cancellation from ToolContext to MCP protocol
    }
}
```

### 4.5 `neuron-context` — Context engine

Token management, compaction strategies, persistent memory, system injection.

```rust
pub struct TokenCounter {
    chars_per_token: f32, // default 4.0, configurable per model
}

impl TokenCounter {
    pub fn estimate_messages(&self, messages: &[Message]) -> usize;
    pub fn estimate_text(&self, text: &str) -> usize;
    pub fn estimate_tools(&self, tools: &[ToolDefinition]) -> usize;
}
```

#### Built-in compaction strategies

```rust
/// Drop oldest messages, keep system + recent N
pub struct SlidingWindowStrategy { pub window_size: usize }

/// Summarize old messages using an LLM (can use a cheap model)
pub struct SummarizationStrategy<P: Provider> {
    pub provider: P,
    pub preserve_recent: usize,
}

/// Clear tool results deep in history (safest per Anthropic)
pub struct ToolResultClearingStrategy { pub keep_recent_n: usize }

/// Chain multiple: try each until under threshold
pub struct CompositeStrategy {
    pub strategies: Vec<Box<dyn ContextStrategy>>,
}
```

All implement `ContextStrategy` from `neuron-types`.

#### Persistent context

Content that survives compaction (equivalent to CLAUDE.md):

```rust
pub struct PersistentContext {
    sections: Vec<ContextSection>,
}

pub struct ContextSection {
    pub label: String,
    pub content: String,
    pub priority: usize, // lower = higher priority, never dropped
}
```

#### System injection

Reminders injected into conversation to prevent drift:

```rust
pub struct SystemInjector {
    rules: Vec<InjectionRule>,
}

pub enum InjectionTrigger {
    EveryNTurns(usize),
    OnTokenThreshold(usize),
    Custom(Box<dyn Fn(&SessionState) -> bool + Send + Sync>),
}
```

### 4.6 `neuron-loop` — The loop

Deliberately small. Composes Provider + Tools + Context via traits.

```rust
pub struct AgentLoop<P: Provider, C: ContextStrategy> {
    provider: P,
    tools: ToolRegistry,
    context: C,
    hooks: Vec<Box<dyn ObservabilityHook>>,
    durability: Option<Box<dyn DurableContext>>,
    config: LoopConfig,
    messages: Vec<Message>,
}

pub struct LoopConfig {
    pub system_prompt: SystemPrompt,
    pub max_turns: Option<usize>,
    pub parallel_tool_execution: bool,
}

pub struct AgentResult {
    pub response: String,
    pub messages: Vec<Message>,
    pub usage: TokenUsage,
    pub turns: usize,
}
```

When `durability` is `Some`, the loop calls `ctx.execute_llm_call()` and
`ctx.execute_tool()` instead of calling provider/tools directly. When `None`,
calls go through directly. The loop does not know which durable engine is
behind the context.

Three execution modes:

```rust
impl<P: Provider, C: ContextStrategy> AgentLoop<P, C> {
    /// Run to completion, return final response.
    pub fn run(
        &mut self,
        prompt: &str,
    ) -> impl Future<Output = Result<AgentResult, LoopError>> + WasmCompatSend;

    /// Stream events as they occur.
    pub fn run_stream(
        &mut self,
        prompt: &str,
    ) -> impl Stream<Item = StreamEvent> + WasmCompatSend;

    /// Yield control after each turn for inspection/modification.
    pub fn run_step(&mut self, prompt: &str) -> StepIterator<'_, P, C>;
}
```

Step-by-step mode (building block, not black box):

```rust
pub struct StepIterator<'a, P: Provider, C: ContextStrategy> {
    loop_ref: &'a mut AgentLoop<P, C>,
}

impl<P: Provider, C: ContextStrategy> StepIterator<'_, P, C> {
    pub async fn next(&mut self) -> Option<TurnResult>;
    pub fn messages(&self) -> &[Message];
    pub fn inject_message(&mut self, message: Message);
    pub fn tools_mut(&mut self) -> &mut ToolRegistry;
}

pub enum TurnResult {
    ToolsExecuted { calls: Vec<ToolCall>, results: Vec<ToolOutput> },
    FinalResponse(AgentResult),
    CompactionOccurred { old_tokens: usize, new_tokens: usize },
    MaxTurnsReached,
    Error(LoopError),
}
```

The loop pseudocode:

```
loop:
    if durability.should_continue_as_new():
        durability.continue_as_new(state)

    check context, compact if needed
    fire hooks(PreLlmCall)

    response = if durability:
        durability.execute_llm_call(request, activity_options)
    else:
        provider.complete(request)

    fire hooks(PostLlmCall)

    if response has tool calls:
        for each tool call:
            fire hooks(PreToolExecution)
            result = if durability:
                durability.execute_tool(name, input, ctx, activity_options)
            else:
                tools.execute(name, input, ctx)
            fire hooks(PostToolExecution)
        append to messages
    else:
        return response.text
```

### 4.7 `neuron-runtime` — Production layer

Sub-agents, sessions, guardrails, DurableContext implementations.

#### Sub-agents

```rust
pub struct SubAgentConfig {
    pub system_prompt: SystemPrompt,
    pub tools: Vec<String>,
    pub model: Option<String>,
    pub max_depth: usize,       // default 1
    pub max_turns: Option<usize>,
}

pub struct SubAgentManager { configs: HashMap<String, SubAgentConfig> }

impl SubAgentManager {
    pub async fn spawn(
        &self,
        name: &str,
        prompt: &str,
        parent_tools: &ToolRegistry,
        provider: &dyn Provider,
    ) -> Result<AgentResult, SubAgentError>;

    pub async fn spawn_parallel(
        &self,
        tasks: Vec<(&str, &str)>,
        parent_tools: &ToolRegistry,
        provider: &dyn Provider,
    ) -> Result<Vec<AgentResult>, SubAgentError>;
}
```

#### Session management

```rust
pub struct Session {
    pub id: String,
    pub messages: Vec<Message>,
    pub state: SessionState,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct SessionState {
    pub cwd: PathBuf,
    pub token_usage: TokenUsage,
    pub event_count: u64,
    pub custom: HashMap<String, serde_json::Value>,
}

pub trait SessionStorage: WasmCompatSend + WasmCompatSync {
    fn save(&self, session: &Session) -> impl Future<Output = Result<(), StorageError>> + WasmCompatSend;
    fn load(&self, id: &str) -> impl Future<Output = Result<Session, StorageError>> + WasmCompatSend;
    fn list(&self) -> impl Future<Output = Result<Vec<SessionSummary>, StorageError>> + WasmCompatSend;
    fn delete(&self, id: &str) -> impl Future<Output = Result<(), StorageError>> + WasmCompatSend;
}
```

#### Guardrails

```rust
pub trait InputGuardrail: WasmCompatSend + WasmCompatSync {
    fn check(
        &self,
        input: &str,
    ) -> impl Future<Output = GuardrailResult> + WasmCompatSend;
}

pub trait OutputGuardrail: WasmCompatSend + WasmCompatSync {
    fn check(
        &self,
        output: &str,
    ) -> impl Future<Output = GuardrailResult> + WasmCompatSend;
}

pub enum GuardrailResult {
    Pass,
    Tripwire(String),
    Warn(String),
}
```

#### DurableContext implementations

```rust
/// For Temporal: wraps WorkflowContext, each LLM/tool call = Activity
pub struct TemporalDurableContext { /* temporal_sdk_core types */ }

/// For Restate: wraps restate_sdk::Context, each call journaled via ctx.run()
pub struct RestateDurableContext { /* restate_sdk types */ }

/// For local dev/testing: direct passthrough, no journaling
pub struct LocalDurableContext {
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
}
```

#### Sandboxing

```rust
pub trait Sandbox: WasmCompatSend + WasmCompatSync {
    fn execute_tool(
        &self,
        tool: &dyn ToolDyn,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> impl Future<Output = Result<ToolOutput, SandboxError>> + WasmCompatSend;
}
```

#### Tracing

Built on Rust's `tracing` crate. Compatible with any OpenTelemetry collector.

```rust
// Span conventions (documented, not a crate):
// - neuron_loop { session_id }
//   - llm_call { model, input_tokens, output_tokens }
//   - tool_execution { tool_name, duration_ms }
//   - context_compaction { old_tokens, new_tokens }
//   - sub_agent { agent_name, depth }
//   - guardrail_check { guardrail_name, result }
```

---

## 5. Composition examples

### Minimal agent (3 blocks)

```rust
use neuron_types::*;
use neuron_provider_anthropic::Anthropic;
use neuron_loop::AgentLoop;

let provider = Anthropic::new(api_key).model("claude-sonnet-4-20250514");
let context = neuron_context::SlidingWindowStrategy::new(100_000);

let mut agent = AgentLoop::new(provider, neuron_tool::ToolRegistry::new(), context)
    .system_prompt("You are a helpful assistant.");

let result = agent.run("What is the capital of France?").await?;
println!("{}", result.response);
```

### Coding agent with MCP tools (6 blocks)

```rust
use neuron_types::*;
use neuron_provider_anthropic::Anthropic;
use neuron_tool::ToolRegistry;
use neuron_mcp::McpClient;
use neuron_context::CompositeStrategy;
use neuron_loop::AgentLoop;

let provider = Anthropic::new(api_key).model("claude-sonnet-4-20250514");

let mut tools = ToolRegistry::new();
tools.register(my_tools::ReadFile);
tools.register(my_tools::EditFile);
tools.register(my_tools::Bash);

// MCP tools integrate seamlessly
let github = McpClient::connect_stdio("npx", &["@mcp/server-github"]).await?;
for tool in github.discover_tools().await? {
    tools.register_dyn(tool);
}

// Per-tool middleware
tools.add_tool_middleware("bash", my_middleware::PermissionRequired);

let context = CompositeStrategy::new(vec![
    Box::new(neuron_context::ToolResultClearingStrategy::new(10)),
    Box::new(neuron_context::SummarizationStrategy::new(
        Anthropic::new(api_key).model("claude-haiku-4-5-20251001"), 20,
    )),
]);

let mut agent = AgentLoop::new(provider, tools, context)
    .system_prompt("You are a coding assistant.")
    .max_turns(50);

let result = agent.run("Fix the bug in src/main.rs").await?;
```

### Durable production agent (all blocks)

```rust
use neuron_types::*;
use neuron_provider_anthropic::Anthropic;
use neuron_tool::ToolRegistry;
use neuron_context::CompositeStrategy;
use neuron_loop::AgentLoop;
use neuron_runtime::*;

let provider = Anthropic::new(api_key).model("claude-sonnet-4-20250514");
let tools = /* ... */;
let context = /* ... */;

// Durable execution via Temporal
let durability = TemporalDurableContext::connect("localhost:7233", "agent-tasks").await?;

let mut agent = AgentLoop::new(provider, tools, context)
    .system_prompt("You are a production agent.")
    .max_turns(100)
    .with_durability(durability)
    .with_guardrail(NoSecretsGuardrail::new());

// Step-by-step execution for framework authors
let mut steps = agent.run_step("Deploy the new feature");
while let Some(turn) = steps.next().await {
    match turn {
        TurnResult::ToolsExecuted { calls, .. } => {
            for call in &calls {
                println!("[tool: {}]", call.name);
            }
        }
        TurnResult::FinalResponse(result) => {
            println!("{}", result.response);
            break;
        }
        _ => {}
    }
}
```

---

## 6. Per-repo structure

Every block follows the same layout:

```
neuron-{block}/
    CLAUDE.md              # Agent instructions for this crate
    Cargo.toml
    src/
        lib.rs             # Public API, re-exports, module docs
        types.rs           # All types in one place
        traits.rs          # All traits in one place
        {feature}.rs       # One file per feature, not nested dirs
        error.rs           # Error types
    tests/
        integration.rs
    examples/
        basic.rs
```

Rules:
- Flat file structure, no deep nesting
- One concept per file, named obviously
- All public types re-exported from `lib.rs`
- Inline doc comments on every public item
- Every trait has a doc example
- Error types are enums with descriptive variants
- No `unwrap()` in library code
- `#[must_use]` on Result-returning functions
- No macro magic that hides control flow

---

## 7. What's NOT in scope

- A CLI, TUI, or GUI (compose from blocks)
- Opinionated agent behaviors (compose from blocks)
- Embedding/RAG (that's a tool or context strategy)
- Training/fine-tuning
- A graph/DAG workflow engine
- A specific workflow engine (Temporal integration is an adapter)

---

## 8. Key dependencies

| Crate | Used by | Purpose |
|-------|---------|---------|
| `serde`, `serde_json` | neuron-types, all | Serialization |
| `thiserror` | neuron-types, all | Error derives |
| `schemars` | neuron-types, neuron-tool | JSON Schema for tool inputs |
| `futures` | neuron-types, all | Stream trait, combinators |
| `tokio` | providers, tool, loop | Async runtime |
| `reqwest` | providers | HTTP client |
| `rmcp` | neuron-mcp | Official MCP Rust SDK |
| `tracing` | all (optional) | Structured logging |
| `chrono` | neuron-runtime | Timestamps |
| `uuid` | neuron-runtime | Session IDs |
| `tokio-util` | neuron-tool | CancellationToken |
