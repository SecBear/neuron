# State

The state system provides scoped key-value persistence through the `StateStore` and `StateReader` traits. neuron ships two implementations: `MemoryStore` (in-memory, ephemeral) and `FsStore` (filesystem-backed, durable).

## StateStore and StateReader

`StateStore` provides full read-write access:

```rust
#[async_trait]
pub trait StateStore: Send + Sync {
    async fn read(&self, scope: &Scope, key: &str)
        -> Result<Option<serde_json::Value>, StateError>;
    async fn write(&self, scope: &Scope, key: &str, value: serde_json::Value)
        -> Result<(), StateError>;
    async fn delete(&self, scope: &Scope, key: &str) -> Result<(), StateError>;
    async fn list(&self, scope: &Scope, prefix: &str) -> Result<Vec<String>, StateError>;
    async fn search(&self, scope: &Scope, query: &str, limit: usize)
        -> Result<Vec<SearchResult>, StateError>;
}
```

`StateReader` is a read-only projection (read, list, search only). Every `StateStore` automatically implements `StateReader` via a blanket impl. Operators receive `&dyn StateReader` during context assembly -- they can read state but must declare writes as effects.

## Scopes

State is partitioned by `Scope`. A scope is a structured identifier that determines where data lives:

```rust
pub enum Scope {
    Agent(AgentId),
    Session(SessionId),
    Workflow(WorkflowId),
    Global,
    Custom { namespace: String, id: String },
}
```

Scopes provide isolation: an agent's state does not collide with another agent's state, and session-scoped data is separate from workflow-scoped data.

## MemoryStore (`neuron-state-memory`)

In-memory storage using a `HashMap`. Data is lost when the process exits.

```rust
use neuron_state_memory::MemoryStore;

let store = MemoryStore::new();
```

Best for:
- Unit and integration tests
- Short-lived processes
- Prototyping

The memory store supports concurrent access through internal locking.

### Example usage

```rust,no_run
use layer0::state::StateStore;
use layer0::effect::Scope;
use layer0::id::SessionId;
use neuron_state_memory::MemoryStore;
use serde_json::json;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let store = MemoryStore::new();
let scope = Scope::Session(SessionId("sess-001".into()));

// Write
store.write(&scope, "user_preference", json!({"theme": "dark"})).await?;

// Read
let value = store.read(&scope, "user_preference").await?;
assert_eq!(value, Some(json!({"theme": "dark"})));

// List keys with prefix
store.write(&scope, "history/turn-1", json!({"msg": "hello"})).await?;
store.write(&scope, "history/turn-2", json!({"msg": "world"})).await?;
let keys = store.list(&scope, "history/").await?;
assert_eq!(keys.len(), 2);

// Delete
store.delete(&scope, "user_preference").await?;
let value = store.read(&scope, "user_preference").await?;
assert_eq!(value, None);
# Ok(())
# }
```

## FsStore (`neuron-state-fs`)

Filesystem-backed storage. Each scope/key pair maps to a file on disk. Data persists across process restarts.

```rust,no_run
use neuron_state_fs::FsStore;

let store = FsStore::new("/tmp/neuron-state");
```

The directory structure mirrors the scope hierarchy:

```
/tmp/neuron-state/
  session/
    sess-001/
      user_preference.json
      history/
        turn-1.json
        turn-2.json
  agent/
    coder/
      config.json
```

Best for:
- CLI tools that need persistent state
- Local development
- Single-machine deployments

## Search

The `search` method supports semantic search within a scope. Implementations that do not support search return an empty `Vec` (not an error):

```rust,no_run
use layer0::state::StateStore;

# async fn example(store: &dyn StateStore) -> Result<(), Box<dyn std::error::Error>> {
let scope = Scope::Global;
let results = store.search(&scope, "user authentication", 5).await?;
for result in results {
    println!("{}: score={}", result.key, result.score);
}
# Ok(())
# }
```

`MemoryStore` and `FsStore` return empty results for search. A future store backed by a vector database or full-text search engine could provide real semantic search.

## Using state with operators

Operators do not write to state directly. Instead:

1. The operator runtime provides a `&dyn StateReader` during context assembly.
2. The operator reads whatever state it needs to build context.
3. If the operator wants to persist something, it includes a state-write `Effect` in its `OperatorOutput`.
4. The calling layer (orchestrator, environment) executes the effect.

This design keeps operators pure: input in, output + effects out. The same operator works whether state is in-memory, on disk, or in a remote database.

## Error handling

```rust
pub enum StateError {
    NotFound { scope, key },   // Key does not exist
    WriteFailed(String),       // Write operation failed
    Serialization(String),     // Serde error
    Other(Box<dyn Error>),     // Catch-all
}
```

Note that `read` returns `Ok(None)` for missing keys, not `Err(NotFound)`. The `NotFound` variant is for cases where a key was expected to exist (e.g., in a higher-level API that wraps the store).
