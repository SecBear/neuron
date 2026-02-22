#![doc = include_str!("../README.md")]

pub mod client;
pub mod error;
pub mod mapping;
pub mod streaming;

pub use client::Ollama;

// Re-export neuron-types for convenience
pub use neuron_types::{ProviderError, StreamEvent, StreamHandle};
