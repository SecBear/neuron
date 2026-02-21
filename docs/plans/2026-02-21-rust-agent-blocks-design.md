# rust-agent-blocks: Design Document

**Date:** 2026-02-21
**Status:** Draft — pending verification phase

---

## 1. Vision

`rust-agent-blocks` is a set of independent, composable Rust crates that provide the building blocks for constructing AI agent systems. Not a framework. Not an SDK. Building blocks.

Anyone can pick the blocks they need and compose them into an agent loop, an SDK, a CLI, a TUI, a GUI, or a workflow orchestration engine. Each block is its own repository, its own crate, publishable independently.

### Design principles

1. **Composable, not opinionated.** Blocks compose via traits. No block dictates how you build your agent.
2. **The loop is boring.** The agentic while loop is ~300 lines. All value lives in the blocks around it: context, tools, runtime.
3. **Rust type safety.** Invalid states are unrepresentable. Compile-time verification of tool schemas, message structure, provider contracts.
4. **Built for agents to work on.** Flat files, obvious names, inline docs, no hidden control flow. AI agents can reason about, modify, and extend every block.
5. **Provider-agnostic from the ground up.** The `Provider` trait lives in the types crate. Any LLM — API, local, remote — just implements the trait.
6. **Durability-ready.** Hooks for Temporal or any durable execution engine, without modifying the loop.

---

## 2. Prior art and research

This design is informed by deep research into existing agent frameworks. None of them ship independent, composable blocks. All ship monoliths.

### Frameworks studied

| Framework | Language | Loop | Decomposition |
|-----------|----------|------|---------------|
| Claude Code (Anthropic) | TypeScript | Flat while loop, ~300 lines (`nO`) | Monolith, internal modules |
| Pydantic AI | Python | FSM via `pydantic-graph` | Two layers: graph + agent. Everything else inline |
| OpenAI Agents SDK | Python | Runner with 3-step cycle | Flat, single package |
| Claude Agent SDK | Python/TS | Opaque (wraps Claude Code) | Single entry point (`query()`) |
| Rig | Rust | `Op` trait pipeline | One fat core, satellite vector store adapters |
| OpenHands | Python | Observe-Think-Act cycle | Best internal decomposition (events, condensers, runtime, agenthub) but still one package |
| Goose (Block) | Rust | Agent loop + MCP | Monolithic Rust binary |

### Key architectural patterns (consensus across all sources)

**The agent loop.** Every framework converges on the same pattern:

```
while true:
    response = provider.complete(messages, tools)
    if response.has_tool_calls():
        results = execute_tools(response.tool_calls)
        messages.append(response)
        messages.append(tool_results)
    else:
        return response.text
```

The loop terminates when the model produces text without tool calls. No DAGs, no state machines. The model decides when to stop.

**Context is the bottleneck.** The context window is finite RAM. Compaction is mandatory for any non-trivial agent. Claude Code triggers auto-compact at ~92-95% capacity. OpenHands provides 4 pluggable condenser strategies. LangChain identifies 4 strategies: write (persist outside window), select (pull into window), compress (reduce tokens), isolate (partition across sub-agents).

**Tool descriptions are billboards.** Tools receive prominent placement in context windows and directly influence model decision-making. Descriptions must be unambiguous, specific, and token-efficient.

**Sub-agents for isolation.** Claude Code limits sub-agent depth to 1. Each sub-agent gets its own context window. Only condensed results return to the parent. Multi-agent approaches can use 15x more tokens than single-agent.

**Message history is the state.** A flat `Vec<Message>` is the source of truth. The model has no memory between API calls. Continuity comes from the client maintaining and growing this array.

### Sources

- Anthropic Engineering: [Effective Context Engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents), [Effective Harnesses for Long-Running Agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents), [Building Agents with the Claude Agent SDK](https://www.anthropic.com/engineering/building-agents-with-the-claude-agent-sdk), [Sandboxing](https://www.anthropic.com/engineering/claude-code-sandboxing)
- Ghuntley: [Tradecraft](https://ghuntley.com/tradecraft/), [Everything is a Ralph Loop](https://ghuntley.com/loop/), [How to Build a Coding Agent](https://ghuntley.com/agent/)
- Kotrotsos: [Claude Code Internals series](https://kotrotsos.medium.com/claude-code-internals-lessons-learned-and-whats-next-551092abeb5d) (14 parts covering loop, messages, tools, state, permissions, sub-agents, context, hooks, streaming)
- LangChain: [Context Engineering for Agents](https://blog.langchain.com/context-engineering-for-agents/)
- OpenHands: [arXiv:2511.03690](https://arxiv.org/abs/2511.03690) — composable SDK for software agents
- Rig: [GitHub](https://github.com/0xPlaygrounds/rig), [docs.rs](https://docs.rs/rig-core) — Rust LLM library
- Temporal: [Rust SDK](https://github.com/temporalio/sdk-core), [AI Cookbook](https://docs.temporal.io/ai-cookbook)

---

## 3. Block decomposition

### 3.1 Overview

```
rust-agent-blocks/                    # Meta-repo (this directory)
│
├── agent-types/                      # Shared types, traits, serde
├── agent-provider-anthropic/         # Anthropic Messages API
├── agent-provider-openai/            # OpenAI Chat/Responses API
├── agent-provider-ollama/            # Ollama (local/remote)
├── agent-tool/                       # Tool registry + execution pipeline
├── agent-mcp/                        # MCP client + server
├── agent-context/                    # Context engine (compaction, memory)
├── agent-loop/                       # The agentic while loop
└── agent-runtime/                    # Sub-agents, sessions, durability
```

### 3.2 Dependency graph

```
agent-types                     (zero deps, the foundation)
    ↑
    ├── agent-provider-*        (each implements Provider trait)
    ├── agent-tool              (implements Tool trait, registry, pipeline)
    ├── agent-mcp               (implements Tool trait via MCP protocol)
    └── agent-context           (+ optional Provider for summarization)
            ↑
        agent-loop              (composes provider + tool + context)
            ↑
        agent-runtime           (sub-agents, sessions, durability)
            ↑
        YOUR PROJECTS           (sdk, cli, tui, gui, gh-aw)
```

Rule: arrows only point up. No circular dependencies. Each block knows only about `agent-types` and the blocks directly below it.

### 3.3 Size estimates

| Block | Purpose | Est. lines |
|-------|---------|------------|
| `agent-types` | Shared types, traits, serde | ~500 |
| `agent-provider-anthropic` | Anthropic Messages API impl | ~800 |
| `agent-provider-openai` | OpenAI Chat/Responses API impl | ~800 |
| `agent-provider-ollama` | Ollama API impl | ~500 |
| `agent-tool` | Tool registry, pipeline, middleware | ~1500 |
| `agent-mcp` | MCP client + server | ~1500 |
| `agent-context` | Compaction, memory, token mgmt | ~1200 |
| `agent-loop` | The agentic while loop | ~300 |
| `agent-runtime` | Sub-agents, sessions, sandboxing, durability, guardrails, tracing | ~2500 |

---

## 4. Block specifications

### 4.1 `agent-types` — The lingua franca

Zero logic. Pure types + serde. Every other block depends on this. This crate defines the language everything speaks.

#### Messages

```rust
pub enum Role { User, Assistant, System }

pub enum ContentBlock {
    Text(String),
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String, is_error: bool },
    Image { source: ImageSource },
}

pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}
```

The conversation state is `Vec<Message>`. Flat. No threading.

#### Provider trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    async fn complete_stream(&self, request: CompletionRequest) -> Result<StreamHandle, ProviderError>;
}

pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub system: Option<String>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub stop_sequences: Vec<String>,
}

pub struct CompletionResponse {
    pub message: Message,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
}

pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cache_read_tokens: Option<usize>,
    pub cache_creation_tokens: Option<usize>,
}
```

#### Tool trait

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
}

pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

pub struct ToolContext {
    pub cwd: PathBuf,
    pub session_id: String,
    pub environment: HashMap<String, String>,
}
```

#### Stream events

```rust
pub enum StreamEvent {
    TextDelta(String),
    ToolUseStart { id: String, name: String },
    ToolUseInputDelta(String),
    ToolUseEnd,
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
pub trait ContextStrategy: Send + Sync {
    fn should_compact(&self, messages: &[Message], token_count: usize) -> bool;
    fn compact(&self, messages: Vec<Message>) -> impl Future<Output = Result<Vec<Message>, ContextError>> + Send;
    fn token_estimate(&self, messages: &[Message]) -> usize;
}
```

#### Durability hooks

```rust
#[async_trait]
pub trait DurabilityHook: Send + Sync {
    async fn on_loop_start(&self, state: &SessionState) -> Result<(), HookError> { Ok(()) }
    async fn on_before_llm_call(&self, request: &CompletionRequest) -> Result<(), HookError> { Ok(()) }
    async fn on_after_llm_call(&self, response: &CompletionResponse) -> Result<(), HookError> { Ok(()) }
    async fn on_before_tool_exec(&self, tool: &str, input: &serde_json::Value) -> Result<(), HookError> { Ok(()) }
    async fn on_after_tool_exec(&self, tool: &str, output: &ToolOutput) -> Result<(), HookError> { Ok(()) }
    async fn on_checkpoint(&self, state: &SessionState) -> Result<(), HookError> { Ok(()) }
}
```

#### Lifecycle hooks

```rust
pub enum HookEvent<'a> {
    LoopIteration { turn: usize },
    PreToolExecution { tool_name: &'a str, input: &'a serde_json::Value },
    PostToolExecution { tool_name: &'a str, output: &'a ToolOutput },
    ContextCompaction { old_tokens: usize, new_tokens: usize },
    SessionStart { session_id: &'a str },
    SessionEnd { session_id: &'a str },
}

#[async_trait]
pub trait Hook: Send + Sync {
    async fn on_event(&self, event: HookEvent<'_>) -> Result<HookAction, HookError>;
}

pub enum HookAction {
    Continue,
    Block(String),   // prevent the action with reason
    Modify(serde_json::Value),  // modify tool input
}
```

### 4.2 `agent-provider-*` — Provider implementations

Each provider is its own crate implementing the `Provider` trait from `agent-types`. Follows the serde pattern: trait in core, impls in satellites.

**Anthropic** (`agent-provider-anthropic`):
- Messages API (`/v1/messages`)
- Streaming via SSE
- Prompt caching (cache read/creation tokens)
- Extended thinking support
- Model constants (claude-sonnet-4, claude-opus-4, etc.)

**OpenAI** (`agent-provider-openai`):
- Chat Completions API (`/v1/chat/completions`)
- Responses API (`/v1/responses`)
- Streaming via SSE
- Structured output (json_schema response format)
- Model constants (gpt-4o, o3, etc.)

**Ollama** (`agent-provider-ollama`):
- Chat API (`/api/chat`)
- Streaming via newline-delimited JSON
- Local model management
- Works with any GGUF model

Each provider crate depends only on `agent-types` + HTTP client (`reqwest`).

### 4.3 `agent-tool` — Tool system

Registry, execution pipeline, middleware chain, derive macro.

#### Tool registry

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    middleware: Vec<Arc<dyn ToolMiddleware>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: impl Tool + 'static);
    pub fn register_arc(&mut self, tool: Arc<dyn Tool>);
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>>;
    pub fn definitions(&self) -> Vec<ToolDefinition>;
    pub async fn execute(&self, name: &str, input: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
}
```

#### Middleware chain

The 7-step pipeline from Claude Code, expressed as composable middleware:

```rust
#[async_trait]
pub trait ToolMiddleware: Send + Sync {
    async fn process(
        &self,
        call: &ToolCall,
        ctx: &ToolContext,
        next: Next<'_>,
    ) -> Result<ToolOutput, ToolError>;
}
```

Built-in middleware:
- `SchemaValidator` — validate input against JSON Schema
- `PermissionChecker` — allow/deny/ask
- `HookRunner` — pre/post lifecycle hooks
- `OutputFormatter` — truncate/format output for model consumption

#### Permission policy

```rust
pub trait PermissionPolicy: Send + Sync {
    fn check(&self, tool_name: &str, input: &serde_json::Value) -> PermissionDecision;
}

pub enum PermissionDecision {
    Allow,
    Deny(String),
    Ask(String),
}
```

#### Derive macro

```rust
#[derive(Tool)]
#[tool(name = "read_file", description = "Read contents of a file at the given path")]
struct ReadFile {
    #[tool(description = "Absolute path to the file to read")]
    path: String,
    #[tool(description = "Maximum number of lines to read", default = 2000)]
    max_lines: Option<usize>,
}
```

### 4.4 `agent-mcp` — MCP integration

Implements the Model Context Protocol for both consuming and exposing tools.

#### Client (consume external tools)

```rust
pub struct McpClient {
    transport: Box<dyn McpTransport>,
}

impl McpClient {
    pub async fn connect_stdio(command: &str, args: &[&str]) -> Result<Self, McpError>;
    pub async fn connect_sse(url: &str) -> Result<Self, McpError>;
    pub async fn discover_tools(&self) -> Result<Vec<Arc<dyn Tool>>, McpError>;
    pub async fn discover_resources(&self) -> Result<Vec<McpResource>, McpError>;
}
```

MCP tools implement the `Tool` trait from `agent-types`, so they plug into `ToolRegistry` without special handling:

```rust
let github = McpClient::connect_stdio("npx", &["@mcp/server-github"]).await?;
for tool in github.discover_tools().await? {
    registry.register_arc(tool);
}
```

#### Server (expose your tools)

```rust
pub struct McpServer {
    registry: ToolRegistry,
}

impl McpServer {
    pub async fn serve_stdio(&self) -> Result<(), McpError>;
    pub async fn serve_sse(&self, addr: SocketAddr) -> Result<(), McpError>;
}
```

### 4.5 `agent-context` — Context engine

Token management, compaction strategies, persistent memory, system injection.

#### Token counting

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
/// Drop oldest messages, keep system prompt + recent N
pub struct SlidingWindowStrategy {
    pub window_size: usize,
}

/// Summarize old messages using an LLM (can use a cheap model)
pub struct SummarizationStrategy<P: Provider> {
    pub provider: P,
    pub preserve_recent: usize,
    pub summary_prompt: String,
}

/// Clear tool results deep in history (safest form per Anthropic)
pub struct ToolResultClearingStrategy {
    pub keep_recent_n: usize,
}

/// Chain multiple strategies: try each in order until under threshold
pub struct CompositeStrategy {
    pub strategies: Vec<Box<dyn ContextStrategy>>,
}
```

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

#### System message injection

Reminders injected into the conversation to prevent drift:

```rust
pub struct SystemInjector {
    rules: Vec<InjectionRule>,
}

pub struct InjectionRule {
    pub trigger: InjectionTrigger,
    pub content: Box<dyn Fn(&SessionState) -> String + Send + Sync>,
}

pub enum InjectionTrigger {
    EveryNToolCalls(usize),
    EveryNTurns(usize),
    OnTokenThreshold(usize),
    Custom(Box<dyn Fn(&SessionState) -> bool + Send + Sync>),
}
```

### 4.6 `agent-loop` — The loop

Deliberately tiny. Composes Provider + Tools + Context via traits.

```rust
pub struct AgentLoop<P: Provider, C: ContextStrategy> {
    provider: P,
    tools: ToolRegistry,
    context: C,
    hooks: Vec<Box<dyn Hook>>,
    durability: Option<Box<dyn DurabilityHook>>,
    config: LoopConfig,
    messages: Vec<Message>,
}

pub struct LoopConfig {
    pub system_prompt: String,
    pub max_turns: Option<usize>,
    pub parallel_tool_execution: bool,
    pub stop_on_first_tool: bool,
}

pub struct AgentResult {
    pub response: String,
    pub messages: Vec<Message>,
    pub usage: TokenUsage,
    pub turns: usize,
}
```

Three execution modes:

```rust
impl<P: Provider, C: ContextStrategy> AgentLoop<P, C> {
    /// Run to completion, return final response
    pub async fn run(&mut self, prompt: &str) -> Result<AgentResult, LoopError>;

    /// Stream events as they occur
    pub fn run_stream(&mut self, prompt: &str) -> impl Stream<Item = StreamEvent>;

    /// Yield control after each turn for inspection/modification
    pub fn run_step(&mut self, prompt: &str) -> StepIterator<'_, P, C>;
}
```

The step-by-step mode is what makes this a building block rather than a black box:

```rust
pub struct StepIterator<'a, P: Provider, C: ContextStrategy> {
    loop_ref: &'a mut AgentLoop<P, C>,
}

impl<P: Provider, C: ContextStrategy> StepIterator<'_, P, C> {
    /// Execute the next turn and return what happened
    pub async fn next(&mut self) -> Option<TurnResult>;

    /// Access current message history
    pub fn messages(&self) -> &[Message];

    /// Inject a message before the next turn
    pub fn inject_message(&mut self, message: Message);

    /// Modify tool registry between turns
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

### 4.7 `agent-runtime` — Production layer

Sub-agents, sessions, sandboxing, guardrails, tracing, durability.

#### Sub-agents

```rust
pub struct SubAgentConfig {
    pub system_prompt: String,
    pub tools: Vec<String>,           // tool whitelist
    pub model: Option<String>,        // can use cheaper model
    pub max_depth: usize,             // default 1
    pub max_turns: Option<usize>,
}

pub struct SubAgentManager {
    configs: HashMap<String, SubAgentConfig>,
}

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
        tasks: Vec<(&str, &str)>,  // (agent_name, prompt)
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct SessionState {
    pub cwd: PathBuf,
    pub token_usage: TokenUsage,
    pub custom: HashMap<String, serde_json::Value>,
}

#[async_trait]
pub trait SessionStorage: Send + Sync {
    async fn save(&self, session: &Session) -> Result<(), StorageError>;
    async fn load(&self, id: &str) -> Result<Session, StorageError>;
    async fn list(&self) -> Result<Vec<SessionSummary>, StorageError>;
    async fn delete(&self, id: &str) -> Result<(), StorageError>;
}

// Built-in: FileSessionStorage, SqliteSessionStorage
```

#### Guardrails

```rust
#[async_trait]
pub trait InputGuardrail: Send + Sync {
    async fn check(&self, input: &str) -> GuardrailResult;
}

#[async_trait]
pub trait OutputGuardrail: Send + Sync {
    async fn check(&self, output: &str) -> GuardrailResult;
}

pub enum GuardrailResult {
    Pass,
    Tripwire(String),
    Warn(String),
}
```

#### Sandboxing

```rust
#[async_trait]
pub trait Sandbox: Send + Sync {
    async fn execute_tool(
        &self,
        tool: &dyn Tool,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, SandboxError>;
}

// Built-in implementations:
// - ProcessSandbox (fork + exec with restricted permissions)
// - WasmSandbox (wasmtime-based isolation)
// - BubblewrapSandbox (Linux, OS-level via bwrap)
// - SeatbeltSandbox (macOS, OS-level via sandbox-exec)
```

#### Durability (Temporal-ready)

```rust
pub struct TemporalDurability {
    client: TemporalClient,
    task_queue: String,
}

#[async_trait]
impl DurabilityHook for TemporalDurability {
    // Every LLM call becomes a Temporal Activity
    // Every tool execution becomes a Temporal Activity
    // The loop itself maps to a Temporal Workflow
    // Human-in-the-loop via Temporal Signals
    // Long-running agents via Continue-As-New
}
```

#### Tracing

Built on Rust's `tracing` crate. Compatible with any OpenTelemetry collector.

```rust
// Span types emitted:
// - agent_loop (entire run)
//   - llm_call (each provider call)
//   - tool_execution (each tool invocation)
//   - context_compaction (each compaction event)
//   - sub_agent (each sub-agent spawn)
//   - guardrail_check (each guardrail evaluation)
```

---

## 5. Composition examples

### Minimal agent (3 blocks)

```rust
use agent_types::*;
use agent_provider_anthropic::Anthropic;
use agent_loop::AgentLoop;

let provider = Anthropic::new(api_key).model("claude-sonnet-4-20250514");
let context = agent_context::SlidingWindowStrategy::new(100_000);

let mut agent = AgentLoop::new(provider, agent_tool::ToolRegistry::new(), context)
    .system_prompt("You are a helpful assistant.");

let result = agent.run("What is the capital of France?").await?;
println!("{}", result.response);
```

### Coding agent (5 blocks)

```rust
use agent_types::*;
use agent_provider_anthropic::Anthropic;
use agent_tool::ToolRegistry;
use agent_context::CompositeStrategy;
use agent_loop::AgentLoop;

let provider = Anthropic::new(api_key).model("claude-sonnet-4-20250514");

let mut tools = ToolRegistry::new();
tools.register(agent_tool::builtins::read_file());
tools.register(agent_tool::builtins::edit_file());
tools.register(agent_tool::builtins::bash());
tools.register(agent_tool::builtins::glob());
tools.register(agent_tool::builtins::grep());

let context = CompositeStrategy::new(vec![
    Box::new(agent_context::ToolResultClearingStrategy::new(10)),
    Box::new(agent_context::SummarizationStrategy::new(
        Anthropic::new(api_key).model("claude-haiku-4-5-20251001"),
        20, // preserve 20 recent messages
    )),
]);

let mut agent = AgentLoop::new(provider, tools, context)
    .system_prompt("You are a coding assistant. Use tools to read, search, and edit code.")
    .max_turns(50);

let result = agent.run("Fix the bug in src/main.rs").await?;
```

### Production agent with MCP + sub-agents + durability (all blocks)

```rust
use agent_types::*;
use agent_provider_anthropic::Anthropic;
use agent_tool::ToolRegistry;
use agent_mcp::McpClient;
use agent_context::CompositeStrategy;
use agent_loop::AgentLoop;
use agent_runtime::*;

// Provider
let provider = Anthropic::new(api_key).model("claude-sonnet-4-20250514");

// Tools: built-in + MCP
let mut tools = ToolRegistry::new();
tools.register(agent_tool::builtins::read_file());
tools.register(agent_tool::builtins::bash());

let github = McpClient::connect_stdio("npx", &["@mcp/server-github"]).await?;
for tool in github.discover_tools().await? {
    tools.register_arc(tool);
}

// Context
let context = CompositeStrategy::new(vec![
    Box::new(agent_context::ToolResultClearingStrategy::new(10)),
    Box::new(agent_context::SummarizationStrategy::new(
        Anthropic::new(api_key).model("claude-haiku-4-5-20251001"), 20,
    )),
]);

// Sub-agents
let mut sub_agents = SubAgentManager::new();
sub_agents.register("explorer", SubAgentConfig {
    system_prompt: "Search and analyze the codebase.".into(),
    tools: vec!["read_file".into(), "glob".into(), "grep".into()],
    model: Some("claude-haiku-4-5-20251001".into()),
    max_depth: 1,
    max_turns: Some(20),
});

// Sessions
let sessions = SessionManager::new(FileSessionStorage::new("./sessions"));

// Durability
let durability = TemporalDurability::connect("localhost:7233", "agent-tasks").await?;

// Compose
let mut agent = AgentLoop::new(provider, tools, context)
    .system_prompt("You are a production coding agent.")
    .max_turns(100)
    .with_durability(durability)
    .with_guardrail(NoSecretsGuardrail::new())
    .with_sub_agents(sub_agents)
    .with_session_manager(sessions);

// Run with streaming
let mut stream = agent.run_stream("Deploy the new feature to staging");
while let Some(event) = stream.next().await {
    match event {
        StreamEvent::TextDelta(text) => print!("{}", text),
        StreamEvent::ToolUseStart { name, .. } => println!("\n[using {}]", name),
        _ => {}
    }
}
```

---

## 6. Per-repo structure

Every block follows the same layout, optimized for agent readability:

```
agent-{block}/
├── CLAUDE.md              # Agent instructions for this crate
├── Cargo.toml
├── src/
│   ├── lib.rs             # Public API, re-exports, module docs
│   ├── types.rs           # All types in one place
│   ├── traits.rs          # All traits in one place
│   ├── {feature}.rs       # One file per feature, not nested dirs
│   └── error.rs           # Error types
├── tests/
│   └── integration.rs     # Integration tests
├── examples/
│   └── basic.rs           # Minimal working example
└── docs/
    └── plans/             # Design docs for this block
```

Agent-friendliness rules enforced in each CLAUDE.md:
- Flat file structure, no deep nesting
- One concept per file, named obviously
- All public types re-exported from `lib.rs`
- Inline doc comments on every public item
- Every trait has a doc example
- Error types are enums with descriptive variants
- No macro magic that hides control flow

---

## 7. What's NOT in scope

- A CLI, TUI, or GUI (build those on `agent-runtime`)
- Opinionated agent behaviors (compose those from blocks)
- Embedding/RAG (separate concern — can be a tool or context strategy)
- Training/fine-tuning
- A specific workflow engine (Temporal integration is an adapter, not a dependency)

---

## 8. Differentiators

1. **Actually decomposed.** No existing framework ships independent, composable blocks. Everyone ships monoliths.
2. **Rust type safety.** Invalid states are unrepresentable. Compile-time verification of tool schemas, message structure, provider contracts.
3. **The loop is 300 lines.** The value is in the blocks around it — context, tools, runtime — not in the loop itself.
4. **Provider-agnostic from the types layer.** The `Provider` trait lives in `agent-types`. Any LLM just implements the trait.
5. **Built for agents to work on.** Flat files, obvious names, inline docs, no hidden control flow.
6. **Durability-ready.** `DurabilityHook` trait means Temporal or any durable execution engine integrates without modifying the loop.
7. **MCP as a first-class peer.** MCP tools implement the same `Tool` trait as local tools.

---

## 9. Next phase: Research and verification

Before transitioning to implementation, a comprehensive research and verification phase is required to validate every aspect of this design. This includes:

- **API shape validation.** Verify every trait design against real API shapes: Anthropic Messages API, OpenAI Responses API, Ollama API, MCP specification.
- **Dependency graph verification.** Confirm no hidden circular dependencies exist. Verify that `agent-context`'s optional dependency on `Provider` (for summarization) doesn't create coupling problems.
- **Streaming abstraction validation.** Confirm the `StreamEvent` enum and `StreamHandle` type work across all three provider APIs (SSE for Anthropic/OpenAI, newline-delimited JSON for Ollama).
- **Tool pipeline edge cases.** Test that the middleware chain handles parallel tool calls, mixed text + tool responses, streaming tool input, and nested tool calls.
- **Context strategy interfaces.** Validate compaction strategies against real conversation traces. Verify token estimation accuracy across models.
- **Durability hook mapping.** Confirm the `DurabilityHook` trait maps cleanly to Temporal's Workflow/Activity model. Verify that the hook points are sufficient for checkpointing.
- **Rig source review.** Deep review of Rig's source code for Rust-idiomatic patterns to adopt or avoid. Particular attention to their `Op` trait, WASM compatibility layer, and provider abstractions.
- **MCP spec coverage.** Verify the `agent-mcp` design covers the full MCP specification (transports, auth, resources, prompts, sampling).
- **Real-world composition test.** Mentally trace through building a complete coding agent from these blocks to identify missing abstractions or awkward boundaries.

This phase should produce a verification report that confirms or revises each design decision before any code is written.
