//! Convenient re-imports for building agents.
//!
//! ```
//! use skelegent::prelude::*;
//! ```

// ── From layer0 ─────────────────────────────────────────────────────────────

pub use layer0::{
    ApprovalFacts,
    Artifact,
    AuthFacts,
    // Capabilities
    CapabilityDescriptor,
    CapabilityFilter,
    CapabilityId,
    CapabilityKind,
    CapabilityModality,
    CapabilitySource,
    CollectedDispatch,

    // Content and messages
    Content,
    ContentBlock,
    DispatchContext,
    DispatchEvent,
    // Dispatch
    DispatchHandle,
    DispatchId,
    // Misc
    DurationMs,
    EnvError,

    // Environment
    EnvironmentProvider,
    EnvironmentSpec,
    ErrorCode,
    EventMeta,
    EventSource,

    ExecutionClass,
    // Events
    ExecutionEvent,
    // Intents
    Intent,
    IntentKind,
    InterceptionKind,
    LimitReason,

    MemoryScope,

    Message,
    MessageMeta,
    // Operator protocol
    Operator,
    OperatorConfig,

    // Identity
    OperatorId,
    OperatorInput,
    OperatorMetadata,
    OperatorOutput,
    // Outcome family
    Outcome,
    // Errors
    ProtocolError,
    ProvisionedEnv,

    ResumeInput,

    Role,

    SchedulingFacts,
    Scope,
    SessionId,
    StateError,
    StateReader,
    // State
    StateStore,
    StoreOptions,

    StreamingSupport,

    TerminalOutcome,
    TransferOutcome,
    // Wait / resume
    WaitReason,
    WaitState,
    WorkflowId,
};

// TriggerType is defined in layer0::operator but not re-exported at layer0 root.
pub use layer0::operator::TriggerType;

// ── From skg-context-engine ─────────────────────────────────────────────────

pub use skg_context_engine::{
    AgentBehaviour,
    // Reactive ops
    AgentEvent,
    // Behaviours
    AgentLoop,
    // Compile
    CompileConfig,
    CompiledContext,
    // Context
    Context,

    ContextOp,
    ErasedContextOp,
    EventKind,
    InferResult,

    LoopDecision,
    OpResult,
    PipelineFactory,
    ReactivePipeline,
    Router,
    SyncOperator,
    SyncOperatorAdapter,

    TimeoutKind,
    Trigger,
    on,

    // Macro (behind feature in the underlying crate, but we enable it)
    skg_tool,
};

// ── From skg-turn ───────────────────────────────────────────────────────────

pub use skg_turn::{InferRequest, InferResponse, Provider, ProviderError, ToolCall};

// ── Operator helpers ────────────────────────────────────────────────────────

pub use layer0::operator::{completed_handle, failed_handle};

// ── Common external macros ──────────────────────────────────────────────────

pub use async_trait::async_trait;
