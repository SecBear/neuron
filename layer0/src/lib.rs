//! # layer0 — Protocol traits for composable agentic AI systems
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
//!
//! ## Dependency Notes
//!
//! This crate depends on `serde_json::Value` for extension data fields
//! (metadata, tool inputs, custom payloads). This is an intentional choice:
//! JSON is the universal interchange format for agentic systems, and
//! `serde_json::Value` is the de facto standard in the Rust ecosystem.
//! The alternative (generic `T: Serialize`) would complicate trait object
//! safety without practical benefit.
//!
//! ## Future: Native Async Traits
//!
//! Protocol traits currently use `async-trait` (heap-allocated futures).
//! When Rust stabilizes `async fn in dyn Trait` with `Send` bounds,
//! these traits will migrate to native async. This will be a breaking
//! change in a minor version bump before v1.0.

#![deny(missing_docs)]

pub mod content;
pub mod duration;
pub mod effect;
pub mod environment;
pub mod error;
pub mod hook;
pub mod id;
pub mod lifecycle;
pub mod orchestrator;
pub mod state;
pub mod turn;

#[cfg(feature = "test-utils")]
pub mod test_utils;

// Re-exports for convenience
pub use content::{Content, ContentBlock};
pub use duration::DurationMs;
pub use effect::{Effect, Scope, SignalPayload};
pub use environment::{Environment, EnvironmentSpec};
pub use error::{EnvError, HookError, OrchError, StateError, TurnError};
pub use hook::{Hook, HookAction, HookContext, HookPoint};
pub use id::{AgentId, ScopeId, SessionId, WorkflowId};
pub use lifecycle::{BudgetEvent, CompactionEvent, ObservableEvent};
pub use orchestrator::{Orchestrator, QueryPayload};
pub use state::{SearchResult, StateReader, StateStore};
pub use turn::{
    ExitReason, ToolCallRecord, Turn, TurnConfig, TurnInput, TurnMetadata, TurnOutput,
};
