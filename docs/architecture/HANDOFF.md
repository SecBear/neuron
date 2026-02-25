# Composable Agentic Architecture — Implementation Handoff

## What This Document Is

An implementation spec for Claude Code. It defines the trait crate (Layer 0) that forms the foundation of a composable agentic architecture, plus the phased plan to integrate it with the existing neuron crate ecosystem.

Read these companion documents for the research and rationale behind every decision here:

- `agentic-decision-map-v3.md` — The 23 architectural decisions every agentic system makes, with the full design space at each decision point. This is the "why these traits exist."
- `composable-agentic-architecture.md` — The 4-protocol + 2-interface architecture, gap analysis showing why three protocols was insufficient, and the coverage map proving all 23 decisions are handled. This is the "why these boundaries."

This document is the "what to build and how."

---

## Naming

The crate name for Layer 0 has not been chosen. Throughout this document, `CORE` is used as a placeholder for whatever the Layer 0 crate is named. Similarly, `core` appears in module paths as `CORE::`. When implementing, replace all occurrences with the chosen name.

The name must be:
- Available on crates.io
- Short (one word or hyphenated pair)
- Descriptive of "protocol definitions for agentic AI systems"
- Not "agentic" (taken)

---

## Architecture Overview

```
LAYER 0 — CORE (this spec)
  Protocol traits + message types. Zero deps beyond serde + async.
  Changes: almost never. This is the stability contract.

LAYER 1 — TURN IMPLEMENTATIONS (neuron today)
  neuron-loop implements CORE::Turn
  neuron-provider-*, neuron-context, neuron-tool, neuron-mcp
  stay unchanged internally — they're Protocol ① internals.

LAYER 2 — ORCHESTRATION IMPLEMENTATIONS (new crates)
  In-process, Temporal, Restate, etc. Each implements CORE::Orchestrator.

LAYER 3 — STATE IMPLEMENTATIONS (new crates)
  HashMap, filesystem, git, SQLite, Postgres. Each implements CORE::StateStore.

LAYER 4 — ENVIRONMENT IMPLEMENTATIONS (new crates)
  Local (passthrough), Docker, k8s, Wasm. Each implements CORE::Environment.

LAYER 5 — CROSS-CUTTING (new crates)
  Hook registry, lifecycle coordination, OpenTelemetry.
```

Layer 0 is its own repo. Everything else can live in the neuron workspace or separate repos — that's an organizational decision, not an architectural one.

---

## Layer 0: The Trait Crate

### Crate Setup

```toml
[package]
name = "CORE"
version = "0.1.0"
edition = "2024"
description = "Protocol traits for composable agentic AI systems"
license = "MIT OR Apache-2.0"
# Dual license matches neuron and the Rust ecosystem convention

[dependencies]
serde = { version = "1", features = ["derive"] }
async-trait = "0.1"          # Until Rust stabilizes async fn in traits everywhere we need them
thiserror = "2"              # Error types
rust_decimal = { version = "1", features = ["serde-str"] }  # Precise cost tracking

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"

[features]
default = []
```

**Dependency policy**: Layer 0 has near-zero dependencies. `serde` for serialization (message types must cross process boundaries). `async-trait` for the protocol traits (async is non-negotiable — every protocol operation may be a network call). `thiserror` for ergonomic errors. `rust_decimal` for precise cost tracking (floating point accumulation errors are real when tracking spend across thousands of LLM calls). Nothing else. No runtime (`tokio` is dev-only). No HTTP. No provider-specific types. If you're considering adding a dependency, the answer is almost certainly no.

### Module Structure

```
src/
  lib.rs          — re-exports, crate-level docs
  content.rs      — Content, ContentBlock, message types
  turn.rs         — Turn trait, TurnInput, TurnOutput, TurnConfig
  effect.rs       — Effect enum, the turn's output side-effects
  orchestrator.rs — Orchestrator trait, topology types
  state.rs        — StateStore trait, Scope, search types
  environment.rs  — Environment trait, isolation/credential specs
  hook.rs         — Hook trait, HookPoint, HookAction
  lifecycle.rs    — LifecycleEvent, budget/compaction coordination types
  error.rs        — Error types for each protocol
  id.rs           — AgentId, SessionId, WorkflowId, typed ID wrappers
```

### The Message Types

These cross every protocol boundary. They must be serializable, cloneable, and self-describing. Every field must justify its existence — anything optional should be `Option`, anything extensible should use `Value` or a `Custom` variant.

```rust
// ── src/content.rs ──

use serde::{Deserialize, Serialize};

/// The universal content type. Crosses every boundary.
/// Intentionally simple — complex structured content uses
/// ContentBlock variants, not nested Content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "image")]
    Image {
        #[serde(with = "serde_bytes_or_url")]
        source: ImageSource,
        media_type: String,
    },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },

    /// Escape hatch for future content types.
    /// If a new modality is invented, it goes here first.
    /// When it stabilizes, it graduates to a named variant.
    #[serde(rename = "custom")]
    Custom {
        content_type: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ImageSource {
    Base64(String),
    Url(String),
}

impl Content {
    pub fn text(s: impl Into<String>) -> Self {
        Content::Text(s.into())
    }

    /// Extract plain text content, ignoring non-text blocks.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Content::Text(s) => Some(s),
            Content::Blocks(blocks) => {
                // Return first text block's content
                blocks.iter().find_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
            }
        }
    }
}
```

```rust
// ── src/id.rs ──

use serde::{Deserialize, Serialize};
use std::fmt;

/// Typed ID wrappers prevent mixing up agent IDs, session IDs, etc.
/// These are just strings underneath — no UUID enforcement, no format
/// requirement. The protocol doesn't care what your IDs look like.

macro_rules! typed_id {
    ($name:ident, $display:expr) => {
        #[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }
    };
}

typed_id!(AgentId, "agent");
typed_id!(SessionId, "session");
typed_id!(WorkflowId, "workflow");
typed_id!(ScopeId, "scope");
```

### The Turn Protocol (①)

```rust
// ── src/turn.rs ──

use crate::{content::Content, effect::Effect, error::TurnError, id::*};
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// What triggers a turn. Informs context assembly — a scheduled trigger
/// means you need to reconstruct everything from state, while a user
/// message carries conversation context naturally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    /// Human sent a message
    User,
    /// Another agent assigned a task
    Task,
    /// Signal from another workflow/agent
    Signal,
    /// Cron/schedule triggered
    Schedule,
    /// System event (file change, webhook, etc.)
    SystemEvent,
    /// Future trigger types
    Custom(String),
}

/// Input to a turn. Everything the turn needs to execute.
///
/// Design decision: TurnInput does NOT include conversation history
/// or memory contents. The turn runtime reads those from a StateStore
/// during context assembly. TurnInput carries the *new* information
/// that triggered this turn — not the accumulated state.
///
/// This keeps the protocol boundary clean: the caller provides what's
/// new, the turn runtime decides how to assemble context from what's
/// new + what's stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnInput {
    /// The new message/task/signal that triggered this turn
    pub message: Content,

    /// What caused this turn to start
    pub trigger: TriggerType,

    /// Session for conversation continuity. If None, the turn is stateless.
    /// The turn runtime uses this to read history from the StateStore.
    pub session: Option<SessionId>,

    /// Configuration for this specific turn execution.
    /// None means "use the turn runtime's defaults."
    pub config: Option<TurnConfig>,

    /// Opaque metadata that passes through the turn unchanged.
    /// Useful for tracing (trace_id), routing (priority), or
    /// domain-specific context that the protocol doesn't need
    /// to understand.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Per-turn configuration overrides. Every field is optional —
/// None means "use the implementation's default."
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnConfig {
    /// Maximum iterations of the inner ReAct loop
    pub max_turns: Option<u32>,

    /// Maximum cost for this turn in USD
    pub max_cost: Option<Decimal>,

    /// Maximum wall-clock time for this turn
    pub max_duration: Option<Duration>,

    /// Model override (implementation-specific string)
    pub model: Option<String>,

    /// Tool restrictions for this turn.
    /// None = use defaults. Some(list) = only these tools.
    pub allowed_tools: Option<Vec<String>>,

    /// Additional system prompt content to prepend/append.
    /// Does not replace the turn runtime's base identity —
    /// it augments it. Use for per-task instructions.
    pub system_addendum: Option<String>,
}

/// Why a turn ended. The caller needs to know this to decide
/// what happens next (retry? continue? escalate?).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    /// Model produced a final text response (natural completion)
    Complete,
    /// Hit the max_turns limit
    MaxTurns,
    /// Hit the cost budget
    BudgetExhausted,
    /// Circuit breaker tripped (consecutive failures)
    CircuitBreaker,
    /// Wall-clock timeout
    Timeout,
    /// Observer/guardrail halted execution
    ObserverHalt { reason: String },
    /// Unrecoverable error during execution
    Error,
    /// Future exit reasons
    Custom(String),
}

/// Output from a turn. Contains the response, metadata about
/// execution, and any side-effects the turn wants executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnOutput {
    /// The turn's response content
    pub message: Content,

    /// Why the turn ended
    pub exit_reason: ExitReason,

    /// Execution metadata (cost, tokens, timing)
    pub metadata: TurnMetadata,

    /// Side-effects the turn wants executed.
    ///
    /// CRITICAL DESIGN DECISION: The turn declares effects but does
    /// not execute them. The calling layer (orchestrator, lifecycle
    /// coordinator) decides when and how to execute them. This is
    /// what makes the turn runtime independent of the layers around it.
    ///
    /// A turn running in-process has its effects executed immediately.
    /// A turn running in a Temporal activity has its effects serialized
    /// and executed by the workflow. Same turn code, different execution.
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// Execution metadata. Every field is concrete (not optional) because
/// every turn produces this data. Implementations that can't track
/// a field (e.g., cost for a local model) use zero/default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnMetadata {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost: Decimal,
    pub turns_used: u32,
    pub tools_called: Vec<ToolCallRecord>,
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    pub duration: Duration,
    pub success: bool,
}

impl Default for TurnMetadata {
    fn default() -> Self {
        Self {
            tokens_in: 0,
            tokens_out: 0,
            cost: Decimal::ZERO,
            turns_used: 0,
            tools_called: vec![],
            duration: Duration::ZERO,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// THE TRAIT
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Protocol ① — The Turn
///
/// What one agent does per cycle. Receives input, assembles context,
/// reasons (model call), acts (tool execution), produces output.
///
/// The ReAct while-loop, the agentic loop, the augmented LLM —
/// whatever you call it, this trait is its boundary.
///
/// Implementations:
/// - neuron's AgentLoop (full-featured turn with tools + context mgmt)
/// - A raw API call wrapper (minimal, no tools)
/// - A human-in-the-loop adapter (waits for human input)
/// - A mock (for testing)
///
/// The trait is intentionally one method. The turn is atomic from the
/// outside — you send input, you get output. Everything that happens
/// inside (how many model calls, how many tool uses, what context
/// strategy) is the implementation's concern.
#[async_trait]
pub trait Turn: Send + Sync {
    /// Execute a single turn.
    ///
    /// The turn runtime:
    /// 1. Assembles context (identity + history + memory + tools)
    /// 2. Runs the ReAct loop (reason → act → observe → repeat)
    /// 3. Returns the output + effects
    ///
    /// The turn MAY read from a StateStore during context assembly.
    /// The turn MUST NOT write to external state directly — it
    /// declares writes as Effects in the output.
    async fn execute(&self, input: TurnInput) -> Result<TurnOutput, TurnError>;
}
```

### The Effect System

```rust
// ── src/effect.rs ──

use crate::{content::Content, id::*};
use serde::{Deserialize, Serialize};

/// A side-effect declared by a turn. NOT executed by the turn —
/// the calling layer decides when and how to execute it.
///
/// This is the key composability mechanism. A turn running in-process
/// has its effects executed by a simple loop. A turn running in Temporal
/// has its effects serialized into the workflow history. A turn running
/// in a test harness has its effects captured for assertions.
///
/// The Custom variant ensures future effect types can be represented
/// without changing the enum. When a new effect type stabilizes
/// (used by 3+ implementations), it graduates to a named variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// Write a value to persistent state.
    WriteMemory {
        scope: Scope,
        key: String,
        value: serde_json::Value,
    },

    /// Delete a value from persistent state.
    DeleteMemory {
        scope: Scope,
        key: String,
    },

    /// Send a fire-and-forget signal to another agent or workflow.
    Signal {
        target: WorkflowId,
        payload: SignalPayload,
    },

    /// Request that the orchestrator dispatch another agent.
    /// This is how delegation works — the turn doesn't call the
    /// other agent directly, it asks the orchestrator to do it.
    Delegate {
        agent: AgentId,
        input: Box<TurnInput>,
    },

    /// Hand off the conversation to another agent. Unlike Delegate,
    /// the current turn is done — the next agent takes over.
    Handoff {
        agent: AgentId,
        /// State to pass to the next agent. This is NOT the full
        /// conversation — it's whatever the current agent thinks
        /// the next agent needs to continue.
        state: serde_json::Value,
    },

    /// Emit a log/trace event. Observers and telemetry consume these.
    Log {
        level: LogLevel,
        message: String,
        data: Option<serde_json::Value>,
    },

    /// Future effect types. Named string + arbitrary payload.
    /// Use this for domain-specific effects that aren't general
    /// enough for a named variant.
    Custom {
        effect_type: String,
        data: serde_json::Value,
    },
}

/// Where state lives. Scopes are hierarchical — a session scope
/// is narrower than a workflow scope, which is narrower than global.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Per-conversation
    Session(SessionId),
    /// Per-workflow-execution
    Workflow(WorkflowId),
    /// Per-agent within a workflow
    Agent {
        workflow: WorkflowId,
        agent: AgentId,
    },
    /// Shared across all workflows
    Global,
    /// Future scopes
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalPayload {
    pub signal_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
```

### The Orchestrator Protocol (②)

```rust
// ── src/orchestrator.rs ──

use crate::{error::OrchError, id::*, turn::TurnInput, turn::TurnOutput};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Protocol ② — Orchestration
///
/// How turns from different agents compose, and how execution
/// survives failures. Durability and composition are inseparable —
/// Temporal replay IS orchestration IS crash recovery. They're the
/// same system.
///
/// Implementations:
/// - LocalOrchestrator: in-process, tokio tasks, no durability
/// - TemporalOrchestrator: Temporal workflows, full durability
/// - RestateOrchestrator: Restate, durable execution
/// - HttpOrchestrator: dispatch over HTTP (microservice pattern)
///
/// The key property: calling code doesn't know which implementation
/// is behind the trait. `dispatch()` might be a function call or a
/// network hop to another continent. The trait is transport-agnostic.
#[async_trait]
pub trait Orchestrator: Send + Sync {
    /// Dispatch a single turn to an agent. May execute locally or
    /// remotely. May be durable or fire-and-forget. The trait doesn't
    /// specify — the implementation decides.
    async fn dispatch(
        &self,
        agent: &AgentId,
        input: TurnInput,
    ) -> Result<TurnOutput, OrchError>;

    /// Dispatch multiple turns in parallel. The implementation decides
    /// whether this is tokio::join!, Temporal child workflows, parallel
    /// HTTP requests, or something else.
    ///
    /// Returns results in the same order as the input tasks.
    /// Individual tasks may fail independently.
    async fn dispatch_many(
        &self,
        tasks: Vec<(AgentId, TurnInput)>,
    ) -> Vec<Result<TurnOutput, OrchError>>;

    /// Fire-and-forget signal to a running workflow.
    /// Used for: inter-agent messaging, user feedback injection,
    /// budget adjustments, cancellation.
    ///
    /// Returns Ok(()) when the signal is accepted (not when it's
    /// processed — that's async by nature).
    ///
    /// Uses `effect::SignalPayload` — the same type turns use to
    /// declare signals as effects. One type, two sides of the boundary.
    async fn signal(
        &self,
        target: &WorkflowId,
        signal: crate::effect::SignalPayload,
    ) -> Result<(), OrchError>;

    /// Read-only query of a running workflow's state.
    /// Used for: dashboards, status checks, budget queries.
    ///
    /// Returns a JSON value — the schema depends on the workflow.
    async fn query(
        &self,
        target: &WorkflowId,
        query: QueryPayload,
    ) -> Result<serde_json::Value, OrchError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPayload {
    pub query_type: String,
    pub params: serde_json::Value,
}
```

### The State Protocol (③)

```rust
// ── src/state.rs ──

use crate::{effect::Scope, error::StateError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Protocol ③ — State
///
/// How data persists and is retrieved across turns and sessions.
///
/// Implementations:
/// - InMemoryStore: HashMap (testing, ephemeral)
/// - FsStore: filesystem (CLAUDE.md, plain files)
/// - GitStore: git-backed (versioned, auditable, mergeable)
/// - SqliteStore: embedded database
/// - PgStore: PostgreSQL (queryable, transactional)
///
/// The trait is deliberately minimal — CRUD + search + list.
/// Compaction is NOT part of this trait because compaction requires
/// coordination across protocols (the Lifecycle Interface).
/// Versioning is NOT part of this trait because not all backends
/// support it — implementations that do can expose it via
/// additional traits or methods.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Read a value by key within a scope.
    /// Returns None if the key doesn't exist.
    async fn read(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<Option<serde_json::Value>, StateError>;

    /// Write a value. Creates or overwrites.
    async fn write(
        &self,
        scope: &Scope,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), StateError>;

    /// Delete a value. No-op if key doesn't exist.
    async fn delete(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<(), StateError>;

    /// List keys under a prefix within a scope.
    async fn list(
        &self,
        scope: &Scope,
        prefix: &str,
    ) -> Result<Vec<String>, StateError>;

    /// Semantic search within a scope. Returns matching keys
    /// with relevance scores. Implementations that don't support
    /// search return an empty vec (not an error).
    async fn search(
        &self,
        scope: &Scope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StateError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub key: String,
    pub score: f64,
    /// Preview/snippet of the matched content.
    /// Implementations decide what to include.
    pub snippet: Option<String>,
}

/// Read-only view of state, given to the turn runtime during
/// context assembly. The turn can read but cannot write — writes
/// go through Effects in TurnOutput.
///
/// This trait exists to enforce the read/write asymmetry at the
/// type level. A Turn receives `&dyn StateReader`, not `&dyn StateStore`.
#[async_trait]
pub trait StateReader: Send + Sync {
    async fn read(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<Option<serde_json::Value>, StateError>;

    async fn list(
        &self,
        scope: &Scope,
        prefix: &str,
    ) -> Result<Vec<String>, StateError>;

    async fn search(
        &self,
        scope: &Scope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StateError>;
}

/// Blanket implementation: every StateStore is a StateReader.
#[async_trait]
impl<T: StateStore> StateReader for T {
    async fn read(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<Option<serde_json::Value>, StateError> {
        StateStore::read(self, scope, key).await
    }

    async fn list(
        &self,
        scope: &Scope,
        prefix: &str,
    ) -> Result<Vec<String>, StateError> {
        StateStore::list(self, scope, prefix).await
    }

    async fn search(
        &self,
        scope: &Scope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StateError> {
        StateStore::search(self, scope, query, limit).await
    }
}
```

### The Environment Protocol (④)

```rust
// ── src/environment.rs ──

use crate::{error::EnvError, turn::TurnInput, turn::TurnOutput};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Protocol ④ — Environment
///
/// What wraps the turn's execution context. Handles isolation,
/// credentials, and resource constraints. The environment mediates
/// between the agent's actions and the host system.
///
/// Implementations:
/// - LocalEnvironment: no isolation, direct execution (dev mode)
/// - DockerEnvironment: spin up container, execute, tear down
/// - K8sEnvironment: create pod with network policy, execute, delete
/// - WasmEnvironment: sandboxed Wasm runtime
///
/// The critical insight: Environment wraps Turn, it doesn't replace it.
/// `run()` takes a `&dyn Turn` and runs it within an isolated context.
/// The Turn implementation doesn't know it's being wrapped.
///
/// For the `LocalEnvironment`, `run()` literally just calls
/// `turn.execute(input)`. For `DockerEnvironment`, it provisions a
/// container, serializes the TurnInput, runs the turn inside the
/// container, deserializes the TurnOutput, and tears down. Same trait,
/// radically different isolation.
#[async_trait]
pub trait Environment: Send + Sync {
    /// Execute a turn within this environment's isolation boundary.
    ///
    /// The implementation:
    /// 1. Provisions any required isolation (container, sandbox, etc.)
    /// 2. Injects credentials according to the spec
    /// 3. Applies resource limits
    /// 4. Executes the turn
    /// 5. Captures the output
    /// 6. Tears down the isolation context
    ///
    /// For local/dev: just calls turn.execute(input) directly.
    async fn run(
        &self,
        turn: &dyn crate::turn::Turn,
        input: TurnInput,
        spec: &EnvironmentSpec,
    ) -> Result<TurnOutput, EnvError>;
}

/// Declarative specification for an execution environment.
/// This is serializable so it can live in config files (YAML, TOML).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentSpec {
    /// Isolation boundaries to apply, outermost first.
    #[serde(default)]
    pub isolation: Vec<IsolationBoundary>,

    /// Credentials to make available inside the environment.
    #[serde(default)]
    pub credentials: Vec<CredentialRef>,

    /// Resource limits.
    pub resources: Option<ResourceLimits>,

    /// Network policy.
    pub network: Option<NetworkPolicy>,
}

/// A single isolation boundary. Multiple boundaries compose
/// (e.g., container + gVisor + network policy = defense in depth).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IsolationBoundary {
    /// OS process boundary
    Process,
    /// Container (Docker, containerd, etc.)
    Container { image: Option<String> },
    /// Syscall interception (gVisor runsc)
    Gvisor,
    /// Hardware-enforced VM (Kata Containers)
    MicroVm,
    /// WebAssembly sandbox
    Wasm { runtime: Option<String> },
    /// Network-level isolation
    NetworkPolicy { rules: Vec<NetworkRule> },
    /// Future isolation types
    Custom {
        boundary_type: String,
        config: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRef {
    /// Name of the credential (e.g., "anthropic-api-key")
    pub name: String,
    /// How to inject it
    pub injection: CredentialInjection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialInjection {
    /// Set as environment variable
    EnvVar { var_name: String },
    /// Mount as file
    File { path: String },
    /// Inject via sidecar/proxy (agent never sees the secret)
    Sidecar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu: Option<String>,        // e.g. "1.0", "500m"
    pub memory: Option<String>,     // e.g. "2Gi", "512Mi"
    pub disk: Option<String>,       // e.g. "10Gi"
    pub gpu: Option<String>,        // e.g. "1" or "nvidia.com/gpu: 1"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Default action for traffic not matching any rule
    pub default: NetworkAction,
    /// Explicit rules
    pub rules: Vec<NetworkRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRule {
    /// Domain or CIDR to match
    pub destination: String,
    /// Port (optional, None = all ports)
    pub port: Option<u16>,
    /// Allow or deny
    pub action: NetworkAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkAction {
    Allow,
    Deny,
}
```

### The Hook Interface (⑤)

```rust
// ── src/hook.rs ──

use crate::{content::Content, error::HookError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Where in the turn's inner loop a hook fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    /// Before each model inference call
    PreInference,
    /// After model responds, before tool execution
    PostInference,
    /// Before each tool is executed
    PreToolUse,
    /// After each tool completes, before result enters context
    PostToolUse,
    /// At each exit-condition check
    ExitCheck,
}

/// What context is available to a hook at its firing point.
/// Read-only — hooks observe and decide, they don't mutate directly.
/// (Mutation happens via HookAction::Modify.)
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Current hook point
    pub point: HookPoint,
    /// Current tool being called (only at Pre/PostToolUse)
    pub tool_name: Option<String>,
    /// Tool input (only at PreToolUse)
    pub tool_input: Option<serde_json::Value>,
    /// Tool result (only at PostToolUse)
    pub tool_result: Option<String>,
    /// Model response (only at PostInference)
    pub model_output: Option<Content>,
    /// Running execution metadata
    pub tokens_used: u64,
    pub cost: rust_decimal::Decimal,
    pub turns_completed: u32,
    pub elapsed: std::time::Duration,
}

/// What a hook decides to do.
#[derive(Debug, Clone)]
pub enum HookAction {
    /// Continue normally
    Continue,
    /// Halt the turn (observer tripwire). The turn exits with
    /// ExitReason::ObserverHalt.
    Halt { reason: String },
    /// Skip this tool call (only valid at PreToolUse).
    /// The tool is not executed and a synthetic "skipped by policy"
    /// result is backfilled.
    SkipTool { reason: String },
    /// Modify the tool input before execution (only at PreToolUse).
    /// Used for: parameter sanitization, injection of defaults.
    ModifyToolInput { new_input: serde_json::Value },
}

/// A hook that can observe and intervene in the turn's inner loop.
///
/// Hooks are registered externally (by the orchestrator, environment,
/// or lifecycle coordinator) and the turn runtime calls them at the
/// defined points. The turn doesn't know who's watching.
///
/// Implementations:
/// - BudgetHook: track cost, halt if over budget
/// - GuardrailHook: validate tool calls against policy
/// - TelemetryHook: emit OpenTelemetry spans
/// - HeartbeatHook: signal liveness to orchestrator (Temporal)
/// - MemorySyncHook: trigger memory writes after state-changing tools
///
/// Hook handlers SHOULD complete quickly. An LLM-based guardrail that
/// calls a model on every tool use adds latency to every tool call.
/// The performance cost is the hook author's responsibility.
#[async_trait]
pub trait Hook: Send + Sync {
    /// Which points this hook fires at.
    fn points(&self) -> &[HookPoint];

    /// Called at each registered hook point.
    /// Returning an error does NOT halt the turn — it logs the error
    /// and continues. Use HookAction::Halt to halt.
    async fn on_event(&self, ctx: &HookContext) -> Result<HookAction, HookError>;
}
```

### The Lifecycle Interface (⑥)

```rust
// ── src/lifecycle.rs ──

use crate::{effect::Scope, id::*};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Events that flow between protocols for lifecycle coordination.
/// These are NOT a trait — they're a shared vocabulary. Each protocol
/// emits and/or consumes these events through whatever mechanism
/// is appropriate (channels, callbacks, event bus, direct calls).
///
/// The Lifecycle Interface is deliberately not a trait because
/// lifecycle coordination is the orchestrator's job. The orchestrator
/// listens for events, applies policies, and takes action. There's
/// no separate "lifecycle service" — it's a responsibility of
/// the orchestration layer.

/// Budget-related events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BudgetEvent {
    /// Emitted by turn after each model call
    CostIncurred {
        agent: AgentId,
        cost: Decimal,
        cumulative: Decimal,
    },
    /// Emitted by orchestrator when nearing limit
    BudgetWarning {
        workflow: WorkflowId,
        spent: Decimal,
        limit: Decimal,
    },
    /// Decision by orchestrator
    BudgetAction {
        workflow: WorkflowId,
        action: BudgetDecision,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetDecision {
    Continue,
    DowngradeModel { from: String, to: String },
    HaltWorkflow,
    RequestIncrease { amount: Decimal },
}

/// Context pressure events — for compaction coordination.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompactionEvent {
    /// Emitted by turn when context window is filling
    ContextPressure {
        agent: AgentId,
        fill_percent: f64,
        tokens_used: u64,
        tokens_available: u64,
    },
    /// Emitted before compaction to trigger memory flush
    PreCompactionFlush {
        agent: AgentId,
        scope: Scope,
    },
    /// Emitted after compaction completes
    CompactionComplete {
        agent: AgentId,
        strategy: String,
        tokens_freed: u64,
    },
}

/// Observability events — the common vocabulary all layers emit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservableEvent {
    /// Which protocol emitted this
    pub source: EventSource,
    /// Event type (free-form, namespaced by convention)
    pub event_type: String,
    /// When it happened
    pub timestamp: Duration, // since workflow start, not wall clock
    /// Event payload
    pub data: serde_json::Value,
    /// Correlation ID across protocols
    pub trace_id: Option<String>,
    /// Workflow context
    pub workflow_id: Option<WorkflowId>,
    /// Agent context
    pub agent_id: Option<AgentId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Turn,
    Orchestration,
    State,
    Environment,
    Hook,
}
```

### Error Types

```rust
// ── src/error.rs ──

use thiserror::Error;

/// Turn execution errors.
#[derive(Debug, Error)]
pub enum TurnError {
    #[error("model error: {0}")]
    Model(String),

    #[error("tool error in {tool}: {message}")]
    Tool { tool: String, message: String },

    #[error("context assembly failed: {0}")]
    ContextAssembly(String),

    /// The turn failed but retrying might succeed.
    /// The orchestrator's retry policy decides.
    #[error("retryable: {0}")]
    Retryable(String),

    /// The turn failed and retrying won't help.
    /// Budget exceeded, invalid input, safety refusal.
    #[error("non-retryable: {0}")]
    NonRetryable(String),

    /// Catch-all. Include context.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Orchestration errors.
#[derive(Debug, Error)]
pub enum OrchError {
    #[error("agent not found: {0}")]
    AgentNotFound(String),

    #[error("workflow not found: {0}")]
    WorkflowNotFound(String),

    #[error("dispatch failed: {0}")]
    DispatchFailed(String),

    #[error("signal delivery failed: {0}")]
    SignalFailed(String),

    #[error("turn error: {0}")]
    TurnError(#[from] TurnError),

    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// State errors.
#[derive(Debug, Error)]
pub enum StateError {
    #[error("not found: {scope:?}/{key}")]
    NotFound { scope: String, key: String },

    #[error("write failed: {0}")]
    WriteFailed(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Environment errors.
#[derive(Debug, Error)]
pub enum EnvError {
    #[error("provisioning failed: {0}")]
    ProvisionFailed(String),

    #[error("isolation violation: {0}")]
    IsolationViolation(String),

    #[error("credential injection failed: {0}")]
    CredentialFailed(String),

    #[error("resource limit exceeded: {0}")]
    ResourceExceeded(String),

    #[error("turn error: {0}")]
    TurnError(#[from] TurnError),

    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Hook errors. These are logged but do NOT halt the turn
/// (use HookAction::Halt to halt).
#[derive(Debug, Error)]
pub enum HookError {
    #[error("hook failed: {0}")]
    Failed(String),

    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}
```

### lib.rs

```rust
// ── src/lib.rs ──

//! # CORE — Protocol traits for composable agentic AI systems
//!
//! This crate defines the four protocol boundaries and two cross-cutting
//! interfaces that compose to form any agentic AI system.
//!
//! ## The Protocols
//!
//! | Protocol | Trait | What it does |
//! |----------|-------|-------------|
//! | ① Turn | [`Turn`] | What one agent does per cycle |
//! | ② Orchestration | [`Orchestrator`] | How agents compose + durability |
//! | ③ State | [`StateStore`] | How data persists across turns |
//! | ④ Environment | [`Environment`] | Isolation, credentials, resources |
//!
//! ## The Interfaces
//!
//! | Interface | Types | What it does |
//! |-----------|-------|-------------|
//! | ⑤ Hooks | [`Hook`], [`HookPoint`], [`HookAction`] | Observation + intervention |
//! | ⑥ Lifecycle | [`BudgetEvent`], [`CompactionEvent`] | Cross-layer coordination |
//!
//! ## Design Principle
//!
//! Every protocol trait is operation-defined, not mechanism-defined.
//! [`Turn::execute`] means "cause this agent to process one cycle" —
//! not "make an API call" or "run a subprocess." This is what makes
//! implementations swappable: a Temporal workflow, a function call,
//! and a future system that doesn't exist yet all implement the same trait.
//!
//! ## Companion Documents
//!
//! - Agentic Decision Map: enumerates all 23 architectural decisions
//! - Composable Agentic Architecture: the 4+2 protocol boundary design

pub mod content;
pub mod effect;
pub mod environment;
pub mod error;
pub mod hook;
pub mod id;
pub mod lifecycle;
pub mod orchestrator;
pub mod state;
pub mod turn;

// Re-exports for convenience
pub use content::{Content, ContentBlock};
pub use effect::{Effect, Scope, SignalPayload};
pub use environment::{Environment, EnvironmentSpec};
pub use error::{EnvError, HookError, OrchError, StateError, TurnError};
pub use hook::{Hook, HookAction, HookContext, HookPoint};
pub use id::{AgentId, ScopeId, SessionId, WorkflowId};
pub use lifecycle::{BudgetEvent, CompactionEvent, ObservableEvent};
pub use orchestrator::{Orchestrator, QueryPayload};
pub use state::{StateReader, StateStore};
pub use turn::{ExitReason, Turn, TurnConfig, TurnInput, TurnMetadata, TurnOutput};
```

---

## Phased Implementation Plan

### Phase 1: Layer 0 crate (the contract)

**What**: Implement everything in this document as a published crate.

**Acceptance criteria**:
- All types compile with `#[deny(missing_docs)]`
- All message types round-trip through `serde_json` (serialize → deserialize → equals original)
- All trait objects work: `Box<dyn Turn>`, `Box<dyn Orchestrator>`, `Box<dyn StateStore>`, `Box<dyn Environment>`, `Box<dyn Hook>` all compile and are Send + Sync
- The `Custom` / `Custom(String)` variants on Content, Effect, IsolationBoundary, TriggerType, ExitReason, and Scope all round-trip through serde correctly
- Zero non-dev dependencies beyond `serde`, `async-trait`, `thiserror`, `rust_decimal`
- `cargo doc` generates clean documentation with the architecture overview
- Tests cover: message type serialization, blanket `StateReader` impl, typed ID conversions, `Content` helper methods

**Not in scope**: No implementations of any trait. This crate is pure interfaces.

### Phase 2: In-memory implementations (prove the traits work)

**What**: Minimal implementations of each trait to validate the API surface is usable.

These can live in the Layer 0 crate behind a `test-utils` feature flag, or in a separate `CORE-test` crate.

```rust
// Prove Turn trait works
pub struct EchoTurn;  // returns the input as output

// Prove Orchestrator works
pub struct LocalOrchestrator {
    agents: HashMap<AgentId, Box<dyn Turn>>,
}

// Prove StateStore works
pub struct InMemoryStore {
    data: DashMap<(Scope, String), serde_json::Value>,
}

// Prove Environment works
pub struct LocalEnvironment;  // no isolation, just passthrough

// Prove Hook works
pub struct LoggingHook;  // logs every event, always returns Continue
```

**Acceptance criteria**:
- `LocalOrchestrator` can dispatch to `EchoTurn`, get a result back
- `LocalOrchestrator::dispatch_many` runs turns concurrently (tokio)
- `InMemoryStore` round-trips read/write/delete/list
- `LocalEnvironment::run` just calls `turn.execute(input)` and returns the result
- `LoggingHook` fires at all hook points without errors
- An integration test composes ALL of these: orchestrator dispatches to two echo agents, results are written to in-memory state, hooks log each step

### Phase 3: neuron integration (prove it works with real agents)

**What**: Make neuron's `AgentLoop` implement `CORE::Turn`.

**Approach**: A thin adapter crate (or feature flag in neuron) that wraps `AgentLoop`:

```rust
use CORE::{Turn, TurnInput, TurnOutput, TurnError};
use neuron_loop::AgentLoop;

pub struct NeuronTurn {
    agent: AgentLoop,
}

#[async_trait]
impl Turn for NeuronTurn {
    async fn execute(&self, input: TurnInput) -> Result<TurnOutput, TurnError> {
        // Convert TurnInput → neuron's input format
        // Call agent.run_text() or agent.run()
        // Convert neuron's output → TurnOutput
        // Collect tool call records into TurnMetadata
        // Map exit reasons
    }
}
```

**Acceptance criteria**:
- `NeuronTurn` can be used anywhere `dyn Turn` is expected
- A real Anthropic API call works through the adapter (requires API key)
- Tool calls are recorded in `TurnMetadata.tools_called`
- Cost tracking in `TurnMetadata.cost` is accurate
- Exit reasons map correctly (model done, max turns, budget)
- Effects are populated from the agent's actions (any file writes become `Effect::WriteMemory`, etc.)

**What this proves**: The Turn trait is practical, not just theoretical. If `AgentLoop` can't implement it cleanly, the trait is wrong — go back to Phase 1.

### Phase 4: Filesystem state + composition test

**What**: Build `FsStore` (implements `StateStore`) and run a composed system.

**Acceptance criteria**:
- `FsStore` maps scopes to directory hierarchies
- Read/write/delete/list work against the filesystem
- Search does basic text matching (grep-like, not vector)
- Integration test: `LocalOrchestrator` dispatches `NeuronTurn` agents, results are written to `FsStore`, hooks track budget — all composed through the trait interfaces

**What this proves**: The full loop works — orchestration dispatches turns, turns produce effects, effects are executed against state, hooks observe everything. All through trait objects. All swappable.

### Phase 5+: Additional implementations (as needed)

Each of these is an independent crate implementing one protocol trait:

- `CORE-orch-temporal` — Temporal orchestration (for Sortie integration)
- `CORE-state-git` — Git-backed state (versioned, auditable)
- `CORE-state-sqlite` — SQLite state (embedded, queryable)
- `CORE-env-docker` — Docker container isolation
- `CORE-env-k8s` — Kubernetes pod isolation
- `CORE-hooks-otel` — OpenTelemetry hook implementation

Each one has a clear acceptance criterion: implement the protocol trait, pass the integration test suite with the existing composed system, swap in for the simpler implementation without changing any other code.

---

## Design Decisions Log

Decisions made in this spec that future implementers should understand:

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Turn declares effects, doesn't execute them | Keeps turn stateless from its own perspective. Enables different execution strategies (immediate, batched, durable) without changing turn code. |
| 2 | `StateReader` as separate read-only trait | Enforces at the type level that turns can read but not write. Prevents accidental state mutation. Blanket impl means any `StateStore` is a `StateReader`. |
| 3 | `Custom` variants on all enums | Future-proofing. New content types, effect types, isolation types, trigger types, exit reasons, and scopes can be represented without changing the enum. When a custom variant stabilizes (3+ implementations), it graduates to a named variant. |
| 4 | `rust_decimal` for cost, not `f64` | Floating point accumulation errors are real over thousands of LLM calls. `$0.001 × 10000` must equal `$10.00`, not `$9.999999...`. |
| 5 | `metadata: serde_json::Value` on TurnInput | Opaque passthrough for tracing, routing, priority, or domain-specific context. The protocol doesn't need to understand it. |
| 6 | Environment takes `&dyn Turn`, not generic `T: Turn` | Must be object-safe. The environment doesn't know what turn implementation it's wrapping at compile time — it might receive different implementations for different agents. |
| 7 | Hook errors don't halt turns | A failing telemetry hook shouldn't kill agent execution. Use `HookAction::Halt` for intentional halts. Errors are logged. |
| 8 | Lifecycle is events, not a trait | No single "lifecycle service" — it's the orchestrator's responsibility to listen for events and apply policies. Making it a trait would force an unnecessary abstraction. |
| 9 | `TurnInput` doesn't carry history | The turn runtime reads history from `StateReader` during context assembly. This keeps the protocol boundary thin — the caller provides what's new, the turn decides how to assemble context. |
| 10 | `SignalPayload` defined once in `effect.rs` | The same type used from both sides: turns declare signals as `Effect::Signal { payload }`, orchestrators deliver them via `Orchestrator::signal()`. One definition, referenced from both modules. |

---

## What This Enables

When Phases 1-4 are complete, you have a working system where:

- Any turn implementation (neuron, raw API wrapper, human, mock) plugs in via `dyn Turn`
- Any orchestration strategy (direct calls, tokio tasks, Temporal) plugs in via `dyn Orchestrator`
- Any persistence backend (memory, filesystem, git, database) plugs in via `dyn StateStore`
- Any isolation level (none, Docker, k8s, gVisor) plugs in via `dyn Environment`
- Any observer (logging, guardrails, budget tracker, telemetry) plugs in via `dyn Hook`
- All of these compose through the protocol boundaries without knowing about each other
- Swapping any single component doesn't require changing any other component

The system works as a single process (everything in-process) or as a distributed system (orchestration over network, state in external database, turns in isolated containers) — determined at deployment time, not at code time. The same trait interfaces, the same calling code, radically different runtime topology.

And if someone invents a new kind of orchestration, state backend, isolation mechanism, or observation pattern at any point in the future — they implement one trait, and it works with everything that already exists.
