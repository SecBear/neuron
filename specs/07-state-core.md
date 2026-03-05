# State Core

## Purpose

State provides continuity across operator cycles. The `StateStore` protocol is the
persistence boundary: operators read through a `StateReader` (read-only), declare writes
as `Effect::WriteMemory`, and the calling layer executes them.

## Protocol

Layer 0 defines:

- `StateStore` — CRUD + search + list + hinted variants
- `StateReader` — read-only capability; blanket-implemented for all `StateStore` implementations
- `StoreOptions` — advisory metadata carried on reads and writes

## Required Semantics

- `Scope` must be treated as part of the keyspace. Keys in different scopes are distinct.
- `list(prefix)` must be deterministic.
- `search` may be unimplemented by some backends (returns empty vec, not an error), but
  implementations must document whether they support it.

Compaction is coordinated via lifecycle vocabulary, not inside the `StateStore` trait.
Versioning is not part of this trait; implementations that support it expose additional
traits or methods.

## API Surface

### Basic CRUD (StateStore)

```rust
async fn read(&self, scope: &Scope, key: &str)
    -> Result<Option<serde_json::Value>, StateError>;

async fn write(&self, scope: &Scope, key: &str, value: serde_json::Value)
    -> Result<(), StateError>;

async fn delete(&self, scope: &Scope, key: &str) -> Result<(), StateError>;

async fn list(&self, scope: &Scope, prefix: &str) -> Result<Vec<String>, StateError>;

async fn search(&self, scope: &Scope, query: &str, limit: usize)
    -> Result<Vec<SearchResult>, StateError>;
```

### Hinted Variants

`write_hinted` and `read_hinted` accept a `&StoreOptions` alongside the normal
parameters. The defaults delegate to the unhinted variants — backends that wish to act on
hints override these methods:

```rust
async fn write_hinted(
    &self,
    scope: &Scope,
    key: &str,
    value: serde_json::Value,
    _options: &StoreOptions,
) -> Result<(), StateError> {
    self.write(scope, key, value).await  // default: ignore hints
}

async fn read_hinted(
    &self,
    scope: &Scope,
    key: &str,
    _options: &StoreOptions,
) -> Result<Option<serde_json::Value>, StateError> {
    self.read(scope, key).await  // default: ignore hints
}
```

`Effect::WriteMemory` carries the same five advisory fields so that the effect executor
can forward them to `write_hinted` without information loss.

### Transient Flush

```rust
fn clear_transient(&self) {}  // default: no-op
```

Called by operators at turn boundaries to discard scratchpad data written with
`Lifetime::Transient`. Backends that do not track lifetime semantics leave this as the
default no-op.

## StoreOptions

`StoreOptions` bundles five advisory fields passed to `write_hinted` / `read_hinted`.
All fields default to `None` (all hints absent). All fields are **advisory**: backends
**MAY** ignore any or all of them; callers **MUST NOT** assume any specific latency,
durability, or routing behavior as a result of setting them.

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoreOptions {
    pub tier: Option<MemoryTier>,
    pub lifetime: Option<Lifetime>,
    pub content_kind: Option<ContentKind>,
    pub salience: Option<f64>,
    pub ttl: Option<DurationMs>,
}
```

### Field Reference

| Field | Type | Meaning | When to set |
|---|---|---|---|
| `tier` | `Option<MemoryTier>` | Preferred storage speed tier | When access latency matters (e.g., hot path vs. archive) |
| `lifetime` | `Option<Lifetime>` | How long the data should persist | For scratchpad data (`Transient`) or cross-session facts (`Durable`) |
| `content_kind` | `Option<ContentKind>` | Cognitive category of the content | When the backend routes or indexes by memory type |
| `salience` | `Option<f64>` | Write-time importance (0.0–1.0, higher = more important) | When compaction or eviction priority matters |
| `ttl` | `Option<DurationMs>` | Auto-expiry hint in milliseconds | For data with a known validity window |

### Backend Guidance

| Hint | If backend supports it | If backend ignores it |
|---|---|---|
| `tier` | Route to appropriate storage layer | Treat as `Hot` (serve from whatever is available) |
| `lifetime` | Enforce persistence policy; `Transient` entries deleted on `clear_transient()` | Data persists until explicitly deleted |
| `content_kind` | Index or route by category (e.g., vector DB namespace) | Store uniformly |
| `salience` | Prefer to retain high-salience entries during eviction | No eviction preference |
| `ttl` | Schedule expiry; remove after the duration elapses | Data persists until explicitly deleted |

Backends that partially support hints (e.g., only `lifetime`) **MUST** document which
fields they honor.

## Advisory Enums

### MemoryTier

Hint for storage speed. Default is `Hot`.

```rust
pub enum MemoryTier {
    Hot,   // default
    Warm,
    Cold,
}
```

| Variant | Semantic intent | Backend guidance |
|---|---|---|
| `Hot` | Frequently accessed; latency-sensitive | Prefer in-process or near-cache storage (HashMap, Redis) |
| `Warm` | Moderately accessed | In-process or near cache; slightly higher latency acceptable |
| `Cold` | Rarely accessed; latency-tolerant | May use slower, cheaper storage (disk, object store) |

Callers set `Hot` for data that will be read in the same or next turn, `Warm` for session
summaries, and `Cold` for archival facts unlikely to be needed soon.

### Lifetime

Hint for how long data should survive.

```rust
pub enum Lifetime {
    Transient,
    Session,
    Durable,
}
```

| Variant | Semantic intent | Backend guidance |
|---|---|---|
| `Transient` | Within the current turn only; intermediate reasoning scratchpad | Eligible for removal on `clear_transient()`; never written to durable storage |
| `Session` | Survives turns; discarded when the session ends | Persist in session-scoped storage; discard on session teardown |
| `Durable` | Survives sessions indefinitely | Write to persistent storage; do not expire |

Use `Transient` for chain-of-thought notes and tool intermediate results that are
meaningless after the turn completes. Use `Durable` for facts the agent needs across
independent invocations (e.g., user preferences, repository conventions).

### ContentKind

Cognitive category based on memory taxonomy.

```rust
pub enum ContentKind {
    Episodic,
    Semantic,
    Procedural,
    Structural,
    Custom(String),
}
```

| Variant | Semantic intent | Example |
|---|---|---|
| `Episodic` | Specific past events | "User approved PR #42 on Jan 15" |
| `Semantic` | Generalized facts | "The API uses OAuth2" |
| `Procedural` | How-to knowledge | "To deploy, run `make release`" |
| `Structural` | Environment or file-system state | "File X exists at path Y" |
| `Custom(String)` | Domain-specific escape hatch | Any category not covered above |

Backends with category-aware storage (e.g., separate vector namespaces per kind) can use
`ContentKind` for routing. Backends without category support ignore it.

## AnnotatedMessage

`AnnotatedMessage` (defined in `turn/neuron-turn/src/context.rs`) wraps a
`ProviderMessage` with optional per-message metadata that governs how the turn layer
treats the message during context compaction.

```rust
pub struct AnnotatedMessage {
    pub message: ProviderMessage,
    pub policy: Option<CompactionPolicy>,  // default: Normal
    pub source: Option<String>,            // e.g. "mcp:github", "user", "tool:shell"
    pub salience: Option<f64>,             // 0.0–1.0 write-time importance hint
}
```

### Fields

| Field | Type | Meaning |
|---|---|---|
| `message` | `ProviderMessage` | The underlying message |
| `policy` | `Option<CompactionPolicy>` | Compaction treatment; `None` behaves as `Normal` |
| `source` | `Option<String>` | Human-readable origin tag (for logging and filtering) |
| `salience` | `Option<f64>` | Importance hint; does not replace `SearchResult.score` |

### Construction

An unannotated message via `AnnotatedMessage::from(msg)` has all metadata fields `None`
and behaves as `policy = Normal`. Two convenience constructors cover common cases:

```rust
// Survives all compaction cycles:
AnnotatedMessage::pinned(msg)

// policy = DiscardWhenDone, source = "mcp:<name>":
AnnotatedMessage::from_mcp(msg, server_name)
```

### CompactionPolicy

`CompactionPolicy` (defined in `layer0/src/lifecycle.rs`, re-exported from `layer0`)
controls how a `ContextStrategy` treats a message:

| Variant | Semantics |
|---|---|
| `Pinned` | Never compacted. For architectural decisions, constraints, user instructions. |
| `Normal` | Default. Subject to standard compaction. |
| `CompressFirst` | Compress preferentially. For verbose output, build logs. |
| `DiscardWhenDone` | Discard when the originating tool or MCP session ends. |

### How Metadata Flows Through Context Assembly

1. Every message entering the context window is wrapped as `AnnotatedMessage`.
2. The `ContextStrategy` (`TieredStrategy` or `NoCompaction`) receives
   `&[AnnotatedMessage]` and reads `policy` and `salience` when deciding what to retain,
   compress, or discard.
3. `source` is carried for observability — hooks and sinks can inspect it to filter log
   output or emit metrics per origin.
4. When `Effect::WriteMemory` accompanies a turn, the `salience` on the effect and the
   `salience` on `AnnotatedMessage` are independent: the former is a persistence hint to
   the backend, the latter is a context-window retention hint to the compaction strategy.

For the full turn-layer context assembly flow including `TieredStrategy` zone partitioning
and the pre-compaction flush pattern, see `specs/04-operator-turn-runtime.md §Context
Assembly`.

## Current Implementation Status

Implemented:

- `neuron-state-memory` — in-memory `StateStore`.
- `neuron-state-fs` — filesystem `StateStore`.
- `StoreOptions`, `MemoryTier`, `Lifetime`, `ContentKind` — defined in `layer0/src/state.rs`.
- `write_hinted` / `read_hinted` on `StateStore` and `StateReader` — defaults delegate to
  unhinted variants; backends override to act on hints.
- `clear_transient` — default no-op; backends that track `Lifetime::Transient` override.
- `AnnotatedMessage`, `CompactionPolicy` — defined in `turn/neuron-turn/src/context.rs`
  and `layer0/src/lifecycle.rs` respectively.
- `Effect::WriteMemory` carries the same five advisory fields as `StoreOptions`.

Still required for "core complete":

- Explicit examples and tests demonstrating scope isolation and persistence semantics.
- At least one backend that honors `Lifetime` hints (beyond the default no-op).
