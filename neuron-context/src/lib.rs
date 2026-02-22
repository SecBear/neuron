#![doc = include_str!("../README.md")]

pub mod counter;
pub mod injector;
pub mod persistent;
pub mod strategies;

pub use counter::TokenCounter;
pub use injector::{InjectionTrigger, SystemInjector};
pub use persistent::{ContextSection, PersistentContext};
pub use strategies::{
    BoxedStrategy, CompositeStrategy, SlidingWindowStrategy, SummarizationStrategy,
    ToolResultClearingStrategy,
};
