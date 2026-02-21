//! Shared types and traits for rust-agent-blocks.
//!
//! This crate defines the lingua franca — messages, providers, tools, errors —
//! that all other agent-blocks crates depend on. Zero logic, pure types.

pub mod types;
pub mod traits;
pub mod error;
pub mod wasm;
pub mod stream;

pub use types::*;
pub use wasm::*;
