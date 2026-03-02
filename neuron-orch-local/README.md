# neuron-orch-local

> In-process orchestrator for neuron — no network, no external services

[![crates.io](https://img.shields.io/crates/v/neuron-orch-local.svg)](https://crates.io/crates/neuron-orch-local)
[![docs.rs](https://docs.rs/neuron-orch-local/badge.svg)](https://docs.rs/neuron-orch-local)
[![license](https://img.shields.io/crates/l/neuron-orch-local.svg)](LICENSE-MIT)

## Overview

`neuron-orch-local` is a fully in-process implementation of the `Orchestrator` trait from
[`layer0`](../layer0). It runs workflows by executing operators sequentially or via delegation in
the same process, maintaining a signal journal for observability and a workflow registry for
status queries.

Use it for:
- Single-machine agentic pipelines
- Testing multi-operator workflows without infrastructure
- Development and CI environments

## Usage

```toml
[dependencies]
neuron-orch-local = "0.4"
neuron-orch-kit = "0.4"
layer0 = "0.4"
```

```rust
use neuron_orch_local::LocalOrchestrator;
use neuron_orch_kit::SystemBuilder;

let system = SystemBuilder::new()
    .operator("worker", Arc::new(my_op))
    .state(Arc::new(mem_store))
    .build()?;

let orch = LocalOrchestrator::new(system);
let handle = orch.start_workflow("worker", input).await?;
let output = handle.await_result().await?;
```

## Part of the neuron workspace

[neuron](https://github.com/secbear/neuron) is a composable async agentic AI framework for Rust.
See the [book](https://secbear.github.io/neuron) for architecture and guides.
