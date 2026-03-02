# State Core

## Purpose

State provides continuity across operator cycles.

## Protocol

Layer 0 defines:

- `StateStore` for read/write/delete/list/search
- `StateReader` as read-only capability (blanket-implemented for all StateStores)

## Required Semantics

- `Scope` must be treated as part of the keyspace.
- `list(prefix)` must be deterministic.
- `search` may be unimplemented by some backends (returns empty), but this must be documented.

Compaction is coordinated via lifecycle vocabulary (not inside the StateStore trait).

## Current Implementation Status

- `neuron-state-memory` exists.
- `neuron-state-fs` exists.

Still required for “core complete”:

- explicit examples and tests demonstrating scope isolation and persistence semantics

