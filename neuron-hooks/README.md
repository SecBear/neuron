# neuron-hooks

> Hook registry and lifecycle middleware for neuron operators

[![crates.io](https://img.shields.io/crates/v/neuron-hooks.svg)](https://crates.io/crates/neuron-hooks)
[![docs.rs](https://docs.rs/neuron-hooks/badge.svg)](https://docs.rs/neuron-hooks)
[![license](https://img.shields.io/crates/l/neuron-hooks.svg)](LICENSE-MIT)

## Overview

`neuron-hooks` provides the `Hook` trait and `HookRegistry` for attaching pre- and post-execution
middleware to operators. Hooks receive lifecycle events (before a turn, after a turn, on error) and
can observe, mutate, or short-circuit execution.

Hooks compose cleanly: register multiple hooks in a registry and they execute in order.

Built into the neuron ecosystem: [`neuron-hook-security`](../neuron-hook-security) provides
ready-made security hooks for redaction and exfiltration detection.

## Usage

```toml
[dependencies]
neuron-hooks = "0.4"
```

### Implementing a custom hook

```rust
use neuron_hooks::{Hook, HookContext, HookError};
use async_trait::async_trait;

pub struct LoggingHook;

#[async_trait]
impl Hook for LoggingHook {
    async fn before_turn(&self, ctx: &HookContext) -> Result<(), HookError> {
        println!("Starting turn for operator: {}", ctx.operator_id());
        Ok(())
    }

    async fn after_turn(&self, ctx: &HookContext) -> Result<(), HookError> {
        println!("Turn completed");
        Ok(())
    }
}
```

### Registering hooks

```rust
use neuron_hooks::HookRegistry;

let mut registry = HookRegistry::new();
registry.register(LoggingHook);
```

## Part of the neuron workspace

[neuron](https://github.com/secbear/neuron) is a composable async agentic AI framework for Rust.
See the [book](https://secbear.github.io/neuron) for architecture and guides.
