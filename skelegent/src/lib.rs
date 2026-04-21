#![deny(missing_docs)]
//! # Skelegent — composable agentic AI runtime
//!
//! One crate to depend on for building agents. Re-exports every skelegent
//! workspace crate, provides a [`prelude`] for ergonomic imports, and ships
//! an [`AgentBuilder`] that assembles the common pieces.
//!
//! ## Quick start
//!
//! ```no_run
//! use skelegent::prelude::*;
//! use skelegent::anthropic::AnthropicProvider;
//! use skelegent::builder::agent;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), ProtocolError> {
//! # struct MyBehaviour;
//! # #[async_trait]
//! # impl AgentBehaviour for MyBehaviour {
//! #     async fn init_context(&self, _: &OperatorInput, _: &DispatchContext) -> Context {
//! #         Context::new()
//! #     }
//! #     fn capabilities(&self, _: &Context) -> Vec<CapabilityDescriptor> { vec![] }
//! #     async fn handle_response(&self, _: &InferResponse, _: &mut Context) -> LoopDecision {
//! #         LoopDecision::Complete(OperatorOutput::new(
//! #             Content::text("done"),
//! #             Outcome::Terminal { terminal: TerminalOutcome::Completed },
//! #         ))
//! #     }
//! #     async fn handle_action_result(&self, _: &OperatorId, _: &OperatorOutput, _: &mut Context) -> LoopDecision {
//! #         LoopDecision::Continue
//! #     }
//! # }
//! let provider = AnthropicProvider::from_env_var("ANTHROPIC_API_KEY");
//! let my_agent = agent(provider, MyBehaviour)
//!     .name("my-agent")
//!     .description("Does things")
//!     .build();
//!
//! let input = OperatorInput::new(Content::text("hello"), TriggerType::User);
//! let ctx = DispatchContext::new(
//!     DispatchId::new("d-1"),
//!     OperatorId::new("my-agent"),
//! );
//! let output = my_agent.handle(input, &ctx).await?.collect().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Crate layout
//!
//! Every workspace crate is re-exported as a module under `skelegent::`:
//!
//! | Module | Purpose |
//! |---|---|
//! | [`layer0`] | Protocol traits, wire types, IDs |
//! | [`context_engine`] | Context, ContextOp, AgentLoop, Router, SyncOperator |
//! | [`turn`] | Provider trait, InferRequest/Response |
//! | [`anthropic`], [`openai`], [`ollama`] | Provider implementations |
//! | [`state_memory`], [`state_fs`] | State store backends |
//! | [`compute`] | Python execution runtime |
//! | [`env_local`] | Local (pass-through) environment provider |
//! | [`secret`] | SecretResolver, zeroization |
//! | [`auth`] | Auth middleware |

pub mod builder;
pub mod prelude;

// ── Core protocol layer ─────────────────────────────────────────────────────

/// Protocol traits, wire types, and IDs. Re-export of the `layer0` crate.
pub use layer0;

/// Context, ContextOp, ReactivePipeline, AgentLoop, Router, SyncOperator.
/// Re-export of the `skg-context-engine` crate.
pub use skg_context_engine as context_engine;

/// Provider trait and inference request/response types.
/// Re-export of the `skg-turn` crate.
pub use skg_turn as turn;

// ── Providers ───────────────────────────────────────────────────────────────

/// Anthropic provider implementation. Re-export of `skg-provider-anthropic`.
pub use skg_provider_anthropic as anthropic;

/// OpenAI provider implementation. Re-export of `skg-provider-openai`.
pub use skg_provider_openai as openai;

/// Ollama provider implementation. Re-export of `skg-provider-ollama`.
pub use skg_provider_ollama as ollama;

// ── State stores ────────────────────────────────────────────────────────────

/// In-memory state store. Re-export of `skg-state-memory`.
pub use skg_state_memory as state_memory;

/// Filesystem state store. Re-export of `skg-state-fs`.
pub use skg_state_fs as state_fs;

// ── Compute ─────────────────────────────────────────────────────────────────

/// Python execution runtime and tool. Re-export of `skg-op-compute-runtime`.
pub use skg_op_compute_runtime as compute;

// ── Environment ─────────────────────────────────────────────────────────────

/// Local (pass-through) environment provider. Re-export of `skg-env-local`.
pub use skg_env_local as env_local;

// ── Secrets & auth ──────────────────────────────────────────────────────────

/// Secret resolution and zeroization. Re-export of `skg-secret`.
pub use skg_secret as secret;

/// Authentication middleware. Re-export of `skg-auth`.
pub use skg_auth as auth;

// ── Top-level builder ───────────────────────────────────────────────────────

pub use builder::{agent, AgentBuilder};
