#![doc = include_str!("../README.md")]

pub mod client;
pub(crate) mod error;
pub mod mapping;
pub(crate) mod streaming;

pub use client::Anthropic;

// Re-export neuron-types for convenience
pub use neuron_types::{ProviderError, StreamEvent, StreamHandle};
