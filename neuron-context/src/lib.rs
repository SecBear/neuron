#![doc = include_str!("../README.md")]

pub mod counter;
pub mod strategies;
pub mod persistent;
pub mod injector;

pub use counter::TokenCounter;
pub use strategies::{
    BoxedStrategy, CompositeStrategy, SlidingWindowStrategy, SummarizationStrategy,
    ToolResultClearingStrategy,
};
pub use persistent::{ContextSection, PersistentContext};
pub use injector::{InjectionTrigger, SystemInjector};
