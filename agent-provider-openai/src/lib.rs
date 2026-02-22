#![doc = include_str!("../README.md")]

pub mod client;
pub mod error;
pub mod mapping;
pub mod streaming;

pub use client::OpenAi;

// Re-export agent-types for convenience
pub use agent_types::{ProviderError, StreamEvent, StreamHandle};
