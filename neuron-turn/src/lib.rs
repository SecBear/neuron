#![deny(missing_docs)]
//! ReAct loop implementing `layer0::Turn`.
//!
//! This crate provides [`NeuronTurn`], a full-featured implementation of
//! the [`layer0::Turn`] trait. It runs a ReAct loop: call the model,
//! execute tools, repeat until done.
//!
//! Key traits defined here:
//! - [`Provider`] — LLM provider interface (not object-safe, uses RPITIT)
//! - [`ContextStrategy`] — context window management

pub mod config;
pub mod context;
pub mod provider;
pub mod types;

// Re-exports
pub use config::NeuronTurnConfig;
pub use context::{ContextStrategy, NoCompaction};
pub use provider::{Provider, ProviderError};
pub use types::*;
