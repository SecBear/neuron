#![deny(missing_docs)]
//! # skg-context-engine
//!
//! Mutable context substrate and reactive runtime for skelegent agents.
//!
//! ## Primitives
//!
//! - [`Context`] — mutable message buffer with extensions, metrics, intents.
//! - [`ContextOp`] / [`Trigger`] — composable, event-driven context transformations.
//! - [`ReactivePipeline`] — event-driven middleware engine: emit events, matching ops fire.
//! - [`Context::compile()`] → [`CompiledContext`] — snapshot to inference request.
//!
//! ## Behaviours
//!
//! - [`SyncOperator`] — convenience for tools and simple operators.
//! - [`Router`] — name-based dispatch to child operators.
//! - [`AgentLoop`] / [`AgentBehaviour`] — the agentic loop (infer → act → observe).

// Primitives
pub mod compile;
pub mod context;
pub mod error;

// Reactive context operations
pub mod agent_event;
pub mod context_op;
pub mod reactive_pipeline;

// Behaviours
pub mod agent_behaviour;
pub mod agent_loop;
pub mod router;
pub mod sync_operator;

// Re-exports: primitives
pub use compile::{CompileConfig, CompiledContext, InferResult};
pub use context::{Context, Extensions, TurnMetrics};
pub use error::EngineError;

// Re-exports: reactive context operations
pub use agent_event::{AgentEvent, EventKind, TimeoutKind};
pub use context_op::{ContextOp, ErasedContextOp, OpResult, Trigger, on};
pub use reactive_pipeline::ReactivePipeline;

// Re-exports: behaviours
pub use agent_behaviour::{AgentBehaviour, LoopDecision};
pub use agent_loop::{AgentLoop, PipelineFactory};
pub use router::Router;
pub use sync_operator::{SyncOperator, SyncOperatorAdapter};
