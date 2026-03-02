# Hooks

> **Note:** The hook system's patterns are still evolving. This page provides a summary of the current design. For the full specification, see `docs/architecture/HANDOFF.md` in the repository.

Hooks provide observation and intervention at defined points inside the operator's inner loop. They fire before and after model inference, before and after tool execution, and at exit-condition checks.

## Overview

The `Hook` trait (defined in `layer0::hook`) declares which hook points an implementation listens to and what action to take when an event fires:

```rust
#[async_trait]
pub trait Hook: Send + Sync {
    fn points(&self) -> &[HookPoint];
    async fn on_event(&self, ctx: &HookContext) -> Result<HookAction, HookError>;
}
```

The five hook points are: `PreInference`, `PostInference`, `PreToolUse`, `PostToolUse`, and `ExitCheck`.

A hook can:
- **Observe** -- Log, emit telemetry, track metrics (return `HookAction::Continue`).
- **Halt** -- Stop execution with a reason (return `HookAction::Halt`).
- **Skip a tool** -- Prevent a tool call (return `HookAction::SkipTool` at `PreToolUse`).
- **Modify input/output** -- Sanitize tool input or redact tool output (return `ModifyToolInput` or `ModifyToolOutput`).

Hook errors are logged but do not halt execution. Use `HookAction::Halt` to halt.

## HookRegistry (`neuron-hooks`)

The `HookRegistry` collects hooks into an ordered pipeline. At each hook point, hooks fire in registration order. The pipeline short-circuits on any action other than `Continue`:

```rust,no_run
use neuron_hooks::HookRegistry;
use std::sync::Arc;

let mut registry = HookRegistry::new();
registry.add(Arc::new(budget_hook));
registry.add(Arc::new(logging_hook));
registry.add(Arc::new(guardrail_hook));
```

Both `ReactOperator` and `SingleShotOperator` accept a `HookRegistry` at construction time and dispatch events through it during execution.

## Use cases

- **Budget enforcement** -- Track accumulated cost at `PostInference`, halt if over budget.
- **Guardrails** -- Validate tool calls at `PreToolUse`, skip dangerous operations.
- **Telemetry** -- Emit OpenTelemetry spans at each hook point.
- **Heartbeat** -- Signal liveness to an orchestrator (e.g., Temporal heartbeat) at `PreInference`.
- **Secret redaction** -- Redact sensitive data from tool output at `PostToolUse`.

For security-focused hooks, see the `neuron-hook-security` crate.
