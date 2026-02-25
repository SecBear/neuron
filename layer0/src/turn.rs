//! The Turn protocol — what one agent does per cycle.

use crate::{content::Content, duration::DurationMs, effect::Effect, error::TurnError, id::*};
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// What triggers a turn. Informs context assembly — a scheduled trigger
/// means you need to reconstruct everything from state, while a user
/// message carries conversation context naturally.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    /// Human sent a message.
    User,
    /// Another agent assigned a task.
    Task,
    /// Signal from another workflow/agent.
    Signal,
    /// Cron/schedule triggered.
    Schedule,
    /// System event (file change, webhook, etc.).
    SystemEvent,
    /// Future trigger types.
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
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnInput {
    /// The new message/task/signal that triggered this turn.
    pub message: Content,

    /// What caused this turn to start.
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
#[non_exhaustive]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnConfig {
    /// Maximum iterations of the inner ReAct loop.
    pub max_turns: Option<u32>,

    /// Maximum cost for this turn in USD.
    pub max_cost: Option<Decimal>,

    /// Maximum wall-clock time for this turn.
    pub max_duration: Option<DurationMs>,

    /// Model override (implementation-specific string).
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
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    /// Model produced a final text response (natural completion).
    Complete,
    /// Hit the max_turns limit.
    MaxTurns,
    /// Hit the cost budget.
    BudgetExhausted,
    /// Circuit breaker tripped (consecutive failures).
    CircuitBreaker,
    /// Wall-clock timeout.
    Timeout,
    /// Observer/guardrail halted execution.
    ObserverHalt {
        /// The reason the observer halted execution.
        reason: String,
    },
    /// Unrecoverable error during execution.
    Error,
    /// Future exit reasons.
    Custom(String),
}

/// Output from a turn. Contains the response, metadata about
/// execution, and any side-effects the turn wants executed.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnOutput {
    /// The turn's response content.
    pub message: Content,

    /// Why the turn ended.
    pub exit_reason: ExitReason,

    /// Execution metadata (cost, tokens, timing).
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
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnMetadata {
    /// Input tokens consumed.
    pub tokens_in: u64,
    /// Output tokens generated.
    pub tokens_out: u64,
    /// Cost in USD.
    pub cost: Decimal,
    /// Number of ReAct loop iterations used.
    pub turns_used: u32,
    /// Record of each tool call made.
    pub tools_called: Vec<ToolCallRecord>,
    /// Wall-clock duration of the turn.
    pub duration: DurationMs,
}

/// Record of a single tool invocation within a turn.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// Name of the tool that was called.
    pub name: String,
    /// How long the tool call took.
    pub duration: DurationMs,
    /// Whether the call succeeded.
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
            duration: DurationMs::ZERO,
        }
    }
}

impl TurnInput {
    /// Create a new TurnInput with required fields.
    pub fn new(message: Content, trigger: TriggerType) -> Self {
        Self {
            message,
            trigger,
            session: None,
            config: None,
            metadata: serde_json::Value::Null,
        }
    }
}

impl TurnOutput {
    /// Create a new TurnOutput with required fields.
    pub fn new(message: Content, exit_reason: ExitReason) -> Self {
        Self {
            message,
            exit_reason,
            metadata: TurnMetadata::default(),
            effects: vec![],
        }
    }
}

impl ToolCallRecord {
    /// Create a new ToolCallRecord.
    pub fn new(name: impl Into<String>, duration: DurationMs, success: bool) -> Self {
        Self {
            name: name.into(),
            duration,
            success,
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
