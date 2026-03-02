# Layer0 Stability Contract

Layer 0 (`layer0` crate) is the protocol boundary. It is treated as a stability contract.

## Rules

1. Do not change `layer0` trait signatures without updating the relevant spec(s) and providing a
   migration story.
2. Keep `layer0` dependencies minimal. Do not add new dependencies unless a spec explicitly
   requires it.
3. All public types and trait methods in `layer0` must be documented (`#![deny(missing_docs)]`).
4. All cross-boundary types must round-trip through `serde_json`.
5. All protocol traits must remain object-safe (`Arc<dyn Trait>` must compile and be `Send + Sync`).

## Where To Look

1. Specs: `specs/02-layer0-protocol-contract.md`
2. Deep details: `docs/architecture/HANDOFF.md`

