# CozoDB Backend Activation Plan

**Date:** 2026-03-04
**Status:** Proposed
**Crate:** `neuron-state-cozo`

## Problem

The `neuron-state-cozo` crate cannot declare `cozo 0.7.6` as a Cargo dependency
because of a native library linking conflict:

- `neuron-state-sqlite` depends on `rusqlite 0.34`, which pulls in
  `libsqlite3-sys 0.32` (declares `links = "sqlite3"`).
- `cozo 0.7.6` with its default `storage-sqlite` feature also links
  `libsqlite3-sys` (declares `links = "sqlite3"`).
- Cargo's resolver rejects two crates in the same build graph that declare the
  same `links` value, even when one is behind an optional feature flag. This is
  a hard constraint of the build system, not a fixable configuration issue.

As a result, the crate currently ships a HashMap-based in-memory stub and the
`cozo` feature flag is a no-op marker.

## Solution

CozoDB supports multiple storage backends selected at compile time via feature
flags:

| Feature            | Backend   | Native linking | Persistence |
|--------------------|-----------|----------------|-------------|
| `storage-sqlite`   | SQLite    | Yes (`sqlite3`)| Yes         |
| `storage-rocksdb`  | RocksDB   | Yes (`rocksdb`) | Yes         |
| `storage-sled`     | Sled      | No             | Yes         |
| `storage-mem`      | In-memory | No             | No          |

**Use `storage-mem`:**

```toml
cozo = { version = "0.7.6", default-features = false, features = ["storage-mem"] }
```

This eliminates the conflict entirely — no native `sqlite3` linking, no
vendoring, no workspace `[patch]` hacks.

### Why storage-mem is the right choice

1. **Agent memory is session-scoped.** The current HashMap implementation is
   already ephemeral. Replacing it with CozoDB's in-memory backend preserves
   this property while unlocking Datalog graph traversal, HNSW vector search,
   and FTS — the actual value proposition of the crate.

2. **Persistence is StateStore's responsibility.** The `StateStore` trait
   abstracts persistence. If durable graph state is needed, the store can
   serialize CozoDB relations to the backing `StateStore` (e.g., SQLite via
   `neuron-state-sqlite`) on checkpoint/shutdown. The graph engine itself does
   not need to own persistence.

3. **Pure Rust, zero native deps.** `storage-mem` has no C/C++ build
   dependencies, simplifying CI and cross-compilation.

### If persistence is needed later

Use `storage-rocksdb` instead:

```toml
cozo = { version = "0.7.6", default-features = false, features = ["storage-rocksdb"] }
```

RocksDB has its own `links` value (`rocksdb`, not `sqlite3`) so there is no
conflict with `rusqlite`. This trades pure-Rust simplicity for on-disk
durability. Only pursue this if session-scoped memory proves insufficient.

## Activation Steps

1. **Update `Cargo.toml`**: Add `cozo` as an optional dependency gated on the
   `cozo` feature:
   ```toml
   [dependencies]
   cozo = { version = "0.7.6", default-features = false, features = ["storage-mem"], optional = true }

   [features]
   cozo = ["dep:cozo"]
   ```

2. **Gate `CozoEngine`**: Wrap the real CozoDB engine impl with
   `#[cfg(feature = "cozo")]` and keep the HashMap fallback for the default
   (no-feature) build.

3. **Update tests**: Add integration tests under `#[cfg(feature = "cozo")]`
   that exercise Datalog queries, vector search, and FTS through the
   `StateStore` interface. All tests remain in-process, no real HTTP calls.

4. **Verify conflict-free build**:
   ```sh
   nix develop --command cargo check -p neuron-state-cozo --features cozo
   nix develop --command cargo check -p neuron-state-sqlite
   nix develop --command cargo check --workspace
   ```

5. **CI**: Add a matrix entry that builds with `--features cozo` to catch
   regressions.

## Risks

| Risk | Mitigation |
|------|------------|
| `cozo 0.7.6` is pre-1.0, single maintainer | Pin exact version (`=0.7.6`). Vendor source into workspace if upstream goes unmaintained. |
| `storage-mem` data lost on crash | Acceptable — agent memory is session-scoped. If durability needed, switch to `storage-rocksdb`. |
| CozoDB query language learning curve | Datalog is well-documented; wrap queries behind typed methods on `CozoStore`. |
| Memory pressure from large graphs | Monitor; CozoDB mem backend is bounded by process heap. Add configurable limits if needed. |

## References

- [CozoDB documentation](https://docs.cozodb.org/)
- `neuron/docs/plans/2026-02-28-agent-memory-graphrag-design.md` — original design
- `neuron/state/neuron-state-cozo/` — current stub implementation
