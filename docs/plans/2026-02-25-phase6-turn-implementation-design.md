# Phase 6: Turn Implementation Bridge — Design Document

## Goal

Build Layer 1 from scratch against layer0 protocol traits. Prove it works with real Anthropic Haiku calls. Four new crates, full modularity, zero drift from our architecture.

## Architecture

NeuronTurn is ONE implementation of the `layer0::Turn` trait. It provides the ReAct loop (call model, execute tools, repeat). Everything inside it — providers, tools, context strategies — is swappable behind trait boundaries. The protocol contract is `Turn::execute(TurnInput) -> TurnOutput`. How NeuronTurn fulfills that contract is internal.

## Key Design Decisions

1. **Design from scratch.** Only port bits from v0.3.0 that match exactly. New internal types aligned with layer0's Content model.
2. **Full modularity.** Providers, tools, context strategies, hooks — all swappable via trait objects or generics.
3. **Provider trait is internal, not object-safe.** Uses RPITIT. The object-safe boundary is `layer0::Turn`.
4. **Real Anthropic Haiku calls** as the proving ground.
5. **No streaming yet.** Can be added without changing core types.
6. **Hook mutations stay as-is.** `ModifyToolInput` is the only mutation. Message modification is ContextStrategy's job. Output redaction is post-processing. No `unsafe` hook pattern.
7. **Provider-native truncation is invisible to the loop.** Provider impls configure it internally. `ContextStrategy` handles client-side compaction. No phantom `StopReason::ContextCompaction`.
8. **The loop does NOT retry.** Provider errors become `TurnError::Retryable` or `TurnError::Model`. Orchestrator decides retry policy.
9. **Cost comes from the provider.** Provider knows its model and pricing. Returns `cost: Option<Decimal>` in `ProviderResponse`.

---

## Crate Structure

```
neuron-tool/                 # ToolDyn trait, ToolRegistry, middleware
                             # Depends on: layer0

neuron-turn/                 # ReAct loop implementing layer0::Turn
                             # Defines: Provider trait, ContextStrategy trait
                             # Accepts: Arc<dyn StateReader>, HookRegistry, ToolRegistry
                             # Depends on: layer0, neuron-tool, neuron-hooks

neuron-context/              # Concrete ContextStrategy implementations
                             # Depends on: neuron-turn (for ContextStrategy trait)

neuron-provider-anthropic/   # Anthropic API client implementing Provider
                             # Depends on: neuron-turn (for Provider trait)
```

### Dependency Graph

```
layer0                        (protocol traits — never changes)
  ↑
  ├── neuron-tool             (depends on: layer0)
  ├── neuron-hooks            (depends on: layer0)  [already exists]
  │
  └── neuron-turn             (depends on: layer0, neuron-tool, neuron-hooks)
        ↑
        ├── neuron-context              (depends on: neuron-turn)
        └── neuron-provider-anthropic   (depends on: neuron-turn)
```

### Swappability

- New provider → implement `Provider` in a new crate
- New context strategy → implement `ContextStrategy` in a new crate
- New tool source (MCP, gRPC, HTTP) → implement `ToolDyn`, register it
- Don't want the ReAct loop → implement `layer0::Turn` directly, skip neuron-turn entirely

---

## Scope Boundaries

### Protocol Contract (layer0 — every Turn must produce this)

```
TurnInput → Turn::execute() → TurnOutput
                                 ├─ message: Content
                                 ├─ exit_reason: ExitReason
                                 ├─ metadata: TurnMetadata (tokens, cost, duration)
                                 └─ effects: Vec<Effect>
```

Every Turn implementation fills in these fields. The protocol doesn't care how.

### NeuronTurn Implementation (our specific Turn)

Adds the ReAct loop, tool execution, context assembly, compaction, budget/turn/timeout enforcement. None of this is required by the protocol — it's NeuronTurn-specific.

### Hooks (external observation/control)

Hooks observe and veto. They cannot mutate the Turn's internal state (except `ModifyToolInput` for tool argument rewriting). Compaction is internal to the Turn. Budget enforcement CAN be a hook (BudgetHook reads cost, returns Halt).

---

## Internal Types (neuron-turn/src/types.rs)

These are the internal lingua franca. Not layer0 types, not Anthropic-specific.

### ProviderMessage

```rust
pub struct ProviderMessage {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

pub enum Role {
    System,
    User,
    Assistant,
}
```

### ContentPart

```rust
pub enum ContentPart {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    Image {
        source: ImageSource,
        media_type: String,
    },
}

pub enum ImageSource {
    Base64(String),
    Url(String),
}
```

Note: No `Custom` variant. If `TurnInput` contains `ContentBlock::Custom`, it is logged as a warning and converted to `ContentPart::Text` (JSON-stringified). This is a deliberate simplification for the internal model.

### ProviderRequest

```rust
pub struct ProviderRequest {
    pub model: Option<String>,
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<ToolSchema>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub system: Option<String>,
    pub extra: serde_json::Value, // provider-specific config passthrough
}

pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
```

- `model`: override from `TurnConfig.model`
- `system`: base system prompt + `TurnConfig.system_addendum` (appended with newline separator)
- `tools`: filtered by `TurnConfig.allowed_tools` before construction
- `extra`: provider-specific config (thinking blocks, reasoning effort, response format). Provider impls extract what they need.
- `TurnInput.metadata` passthrough: trace IDs and other metadata are forwarded via `extra`.

### ProviderResponse

```rust
pub struct ProviderResponse {
    pub content: Vec<ContentPart>,
    pub stop_reason: StopReason,
    pub usage: TokenUsage,
    pub model: String,              // actual model used
    pub cost: Option<Decimal>,      // provider calculates from model + tokens
    pub truncated: Option<bool>,    // telemetry: provider truncated input?
}

pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    ContentFilter,
}

pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: Option<u64>,
    pub cache_creation_tokens: Option<u64>,
}
```

- `StopReason::MaxTokens`: loop surfaces as `TurnError::Model("output truncated")`. Caller decides retry.
- `cost`: provider knows its pricing. `None` if unknown. Loop aggregates across iterations.
- `truncated`: telemetry only. Not control flow.

---

## Internal Traits

### Provider (defined in neuron-turn, NOT object-safe)

```rust
pub trait Provider: Send + Sync {
    fn complete(
        &self,
        request: ProviderRequest,
    ) -> impl Future<Output = Result<ProviderResponse, ProviderError>> + Send;
}
```

Each provider (Anthropic, OpenAI, Ollama) implements this. Provider-native features (truncation, caching, thinking blocks) are handled by the provider impl using `ProviderRequest.extra`.

### ContextStrategy (defined in neuron-turn)

```rust
pub trait ContextStrategy: Send + Sync {
    fn token_estimate(&self, messages: &[ProviderMessage]) -> usize;
    fn should_compact(&self, messages: &[ProviderMessage], limit: usize) -> bool;
    fn compact(&self, messages: Vec<ProviderMessage>) -> Vec<ProviderMessage>;
}
```

Implementations: `NoCompaction`, `SlidingWindow`, `Summarization` (future), `Composite` (chains strategies).

Provider-native truncation (e.g., OpenAI `truncation: auto`) is invisible to the strategy — the Provider impl configures it internally.

### ToolDyn (defined in neuron-tool)

```rust
pub trait ToolDyn: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn call(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>>;
}
```

Any tool source (local function, MCP server, HTTP endpoint) implements this. ToolRegistry holds `HashMap<String, Arc<dyn ToolDyn>>`.

---

## NeuronTurn

```rust
pub struct NeuronTurn<P: Provider> {
    provider: P,
    tools: ToolRegistry,
    context_strategy: Box<dyn ContextStrategy>,
    hooks: HookRegistry,
    state_reader: Arc<dyn StateReader>,
    config: NeuronTurnConfig,
}
```

- `provider`: the LLM provider (generic, not object-safe)
- `tools`: registered tools (all sources)
- `context_strategy`: how to manage the context window
- `hooks`: hook pipeline (can be empty)
- `state_reader`: read-only access to state (for context assembly)
- `config`: defaults (base system prompt, default model, etc.)

Per-request overrides come from `TurnInput.config` (layer0's `TurnConfig`).

```rust
#[async_trait]
impl<P: Provider + 'static> Turn for NeuronTurn<P> {
    async fn execute(&self, input: TurnInput) -> Result<TurnOutput, TurnError> {
        // The ReAct loop
    }
}
```

---

## ReAct Loop Data Flow

```
TurnInput arrives (layer0 type)
  │
  ├─ Read history from StateReader
  ├─ Read TurnConfig (max_turns, model, allowed_tools, etc.)
  ├─ Assemble context: system + history + tool schemas
  │   └─ Filter tools by TurnConfig.allowed_tools
  │   └─ Append TurnConfig.system_addendum to base system prompt
  │
  ▼
┌─────────────── ReAct Loop ───────────────┐
│                                           │
│  1. Hook: PreInference                    │
│  2. Build ProviderRequest                 │
│  3. Call provider.complete(request)       │
│  4. Hook: PostInference                   │
│  5. Aggregate tokens + cost              │
│  6. Check StopReason:                     │
│     - MaxTokens → TurnError::Model       │
│     - ContentFilter → TurnError::Model   │
│     - EndTurn (no tool calls) → exit loop │
│     - ToolUse → continue to step 7       │
│  7. For each tool call:                   │
│     a. Check if effect tool → add Effect  │
│     b. Hook: PreToolUse                   │
│        (may ModifyToolInput or SkipTool)  │
│     c. ToolRegistry.call(name, input)     │
│     d. Hook: PostToolUse                  │
│     e. Backfill result into messages      │
│     f. Record ToolCallRecord              │
│  8. Check max_turns, max_cost, timeout    │
│  9. Hook: ExitCheck                       │
│     (may Halt → ObserverHalt exit)        │
│  10. ContextStrategy: compact if needed   │
│  11. Repeat                               │
│                                           │
└───────────────────────────────────────────┘
  │
  ▼
Build TurnOutput (layer0 type)
  ├─ message: final Content (from last provider response)
  ├─ exit_reason: ExitReason
  ├─ metadata: TurnMetadata (aggregated tokens, cost, duration, tools_called)
  └─ effects: Vec<Effect> (accumulated from effect tools)
```

---

## Effect Generation

Effects are produced by "effect tools" — special tool names that the loop intercepts instead of executing locally.

| Tool Name | Effect Produced | Backfill to Context |
|---|---|---|
| `write_memory` | `Effect::WriteMemory { scope, key, value }` | "Memory written." |
| `delete_memory` | `Effect::DeleteMemory { scope, key }` | "Memory deleted." |
| `delegate` | `Effect::Delegate { agent, input }` | "Delegation requested." |
| `handoff` | `Effect::Handoff { agent, state }` | "Handoff initiated." |
| `signal` | `Effect::Signal { target, payload }` | "Signal sent." |

Regular tools (bash, read_file, etc.) execute normally via `ToolDyn::call()`. The loop checks the tool name against a known set of effect tool names before executing.

`Effect::Log` is generated by the loop itself during execution (not from tool calls). `Effect::Custom` can be generated by a tool that returns a specially-structured JSON response (convention-based).

---

## Mapping: layer0 ↔ Internal Types

### TurnInput → Context Assembly

| TurnInput field | Used for |
|---|---|
| `message` | Last user message, converted to `ProviderMessage` |
| `trigger` | Informs context assembly (not sent to provider) |
| `session` | Key for reading history from StateReader |
| `config.model` | `ProviderRequest.model` |
| `config.system_addendum` | Appended to base system prompt |
| `config.max_turns` | Loop iteration limit |
| `config.max_cost` | Budget enforcement in loop |
| `config.max_duration` | Timeout enforcement in loop |
| `config.allowed_tools` | Filters `ProviderRequest.tools` |
| `metadata` | Forwarded to `ProviderRequest.extra` for tracing |

### ProviderResponse → TurnOutput

| ProviderResponse field | TurnOutput field |
|---|---|
| `content` | `message` (ContentPart → ContentBlock mapping) |
| `stop_reason` | `exit_reason` (EndTurn→Complete, ToolUse→N/A continues loop) |
| `usage` | `metadata.tokens_in`, `metadata.tokens_out` (aggregated across all calls) |
| `cost` | `metadata.cost` (aggregated across all calls) |
| `model` | Not captured in TurnMetadata (logged, not protocol) |
| `truncated` | Not captured (telemetry for hooks/observability) |

### ContentPart ↔ ContentBlock

Bidirectional mapping. `ContentBlock::Custom` → `ContentPart::Text` (JSON-stringified, with warning log). All other variants map 1:1.

---

## Validated Against Source Documents

- **HANDOFF.md**: Aligned. Layer 1 internals correctly scoped. Turn trait signature matches. StateReader injection present.
- **composable-agentic-architecture.md**: Aligned. Turn owns context assembly, inference, tool execution, exit conditions. Does not own isolation, durability, or state writes.
- **agentic-decision-map-v3.md**: Aligned. All 23 decisions correctly addressed or deferred to appropriate layer.
- **validation-and-coordination.md**: Aligned. Protocol boundaries preserved. No layer0 changes needed.

No contradictions found. All gaps resolved in this design.
