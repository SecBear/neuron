# layer0

> Protocol traits for composable agentic AI systems

[![crates.io](https://img.shields.io/crates/v/layer0.svg)](https://crates.io/crates/layer0)
[![docs.rs](https://docs.rs/layer0/badge.svg)](https://docs.rs/layer0)
[![license](https://img.shields.io/crates/l/layer0.svg)](LICENSE-MIT)

## Overview

`layer0` defines the foundational protocol traits that the entire neuron workspace builds on. It
contains **no implementations** — only the contracts that every agentic component must satisfy.

The traits here form the 6-layer model:

| Layer | Trait(s) | Responsibility |
|-------|----------|----------------|
| 0 | `Operator`, `Effect` | Invoke operators, emit structured effects |
| 1 | `StateStore` | Persistent key-value memory |
| 2 | `Environment`, `CredentialRef` | Credential injection + env config |
| 3 | `Orchestrator`, `WorkflowHandle` | Coordinate multi-operator workflows |
| 4 | `Hook`, `HookRegistry` | Pre/post lifecycle middleware |
| 5 | `Observable` | Structured observability events |

## Usage

```toml
[dependencies]
layer0 = "0.4"
```

### Test utilities

```toml
[dev-dependencies]
layer0 = { version = "0.4", features = ["test-utils"] }
```

The `test-utils` feature exports stub implementations (`StubOperator`, `StubStateStore`, etc.)
useful for testing downstream crates without pulling in full implementations.

## Part of the neuron workspace

[neuron](https://github.com/secbear/neuron) is a composable async agentic AI framework for Rust.
See the [book](https://secbear.github.io/neuron) for architecture and guides.
