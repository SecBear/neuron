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
pub mod convert;
pub mod provider;
pub mod types;

// Re-exports
pub use config::NeuronTurnConfig;
pub use context::{ContextStrategy, NoCompaction};
pub use convert::{
    content_block_to_part, content_part_to_block, content_to_parts, content_to_user_message,
    parts_to_content,
};
pub use provider::{Provider, ProviderError};
pub use types::*;
