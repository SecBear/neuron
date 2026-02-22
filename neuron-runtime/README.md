# neuron-runtime

Production runtime layer for the neuron agent blocks ecosystem. Provides
session management, sub-agent orchestration, input/output guardrails, durable
execution contexts, and tool sandboxing.

## Key Types

- `Session` -- a conversation session with messages, state, and timestamps.
  Created with `Session::new(id, cwd)`.
- `SessionState` -- mutable runtime state within a session (working directory,
  token usage, event count, custom metadata).
- `SessionStorage` trait -- async trait for persisting and loading sessions.
- `InMemorySessionStorage` -- `Arc<RwLock<HashMap>>` storage for testing.
- `FileSessionStorage` -- one JSON file per session on disk.
- `InputGuardrail` / `OutputGuardrail` -- traits for safety checks that run
  before input reaches the LLM or before output reaches the user.
- `GuardrailResult` -- `Pass`, `Tripwire(reason)`, or `Warn(reason)`. A
  tripwire halts execution immediately.
- `GuardrailHook` -- an `ObservabilityHook` that runs input/output guardrails
  during the agent loop. Tripwires terminate; warnings log and continue.
- `SubAgentConfig` -- configuration for spawning sub-agents (system prompt,
  tool filter, max depth, max turns).
- `SubAgentManager` -- registers and spawns sub-agents.
- `LocalDurableContext` -- passthrough `DurableContext` for local development
  (no journaling or replay).
- `Sandbox` trait / `NoOpSandbox` -- tool execution isolation boundary.

## Usage

```rust,no_run
use std::path::PathBuf;
use neuron_runtime::{
    FileSessionStorage, GuardrailResult, InputGuardrail,
    Session, SessionStorage,
};

// Create a session and persist it to disk
async fn create_session() -> Result<(), Box<dyn std::error::Error>> {
    let storage = FileSessionStorage::new(PathBuf::from("/tmp/sessions"));
    let session = Session::new("session-001", PathBuf::from("/home/user"));
    storage.save(&session).await?;

    let loaded = storage.load("session-001").await?;
    assert_eq!(loaded.id, "session-001");
    Ok(())
}

// Define an input guardrail that blocks secrets
struct NoSecrets;
impl InputGuardrail for NoSecrets {
    fn check(
        &self,
        input: &str,
    ) -> impl std::future::Future<Output = GuardrailResult> + Send {
        async move {
            if input.contains("API_KEY") || input.contains("sk-") {
                GuardrailResult::Tripwire(
                    "Input contains a secret".to_string(),
                )
            } else {
                GuardrailResult::Pass
            }
        }
    }
}
```

## GuardrailHook

`GuardrailHook` wraps input and output guardrails as an `ObservabilityHook`,
so they integrate directly into the agent loop via the hook system.

```rust,no_run
use neuron_runtime::{
    GuardrailHook, GuardrailResult, InputGuardrail, OutputGuardrail,
};

struct BlockSecrets;
impl InputGuardrail for BlockSecrets {
    fn check(
        &self,
        input: &str,
    ) -> impl std::future::Future<Output = GuardrailResult> + Send {
        async move {
            if input.contains("API_KEY") {
                GuardrailResult::Tripwire("secret in input".to_string())
            } else {
                GuardrailResult::Pass
            }
        }
    }
}

struct NoLeaks;
impl OutputGuardrail for NoLeaks {
    fn check(
        &self,
        output: &str,
    ) -> impl std::future::Future<Output = GuardrailResult> + Send {
        async move {
            if output.contains("sk-") {
                GuardrailResult::Tripwire("secret in output".to_string())
            } else {
                GuardrailResult::Pass
            }
        }
    }
}

let hook = GuardrailHook::new()
    .input_guardrail(BlockSecrets)
    .output_guardrail(NoLeaks);

// Pass `hook` as the ObservabilityHook when building the agent loop.
```

- On `PreLlmCall`, the hook extracts the last user message text and runs all
  input guardrails. A `Tripwire` returns `HookAction::Terminate`; a `Warn`
  logs via `tracing::warn!` and continues.
- On `PostLlmCall`, the hook extracts the assistant response text and runs all
  output guardrails with the same mapping.

## Part of neuron

This crate is one block in the [neuron](https://github.com/secbear/neuron)
composable agent toolkit. It depends on `neuron-types`, `neuron-tool`, and
`neuron-loop`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
