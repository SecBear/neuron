# Packaging, Versioning, and Umbrella Crate

## Purpose

The redesign must be consumable.

Old Neuron shipped an umbrella `neuron` crate with feature flags and a prelude.

The redesign needs an equivalent packaging story.

## Requirements

- Provide an umbrella crate (likely `neuron`) that re-exports protocol + key implementations behind feature flags.
- Provide a stable set of feature flags for:
  - providers
  - MCP
  - orchestration implementations
  - state backends
  - hooks
  - environment implementations
- Provide a prelude that covers the happy path.

## Current Status

On `redesign/v2`, there is no umbrella crate directory.

This is required for “core complete” because it determines whether anyone can actually adopt the redesign without importing 12 crates manually.

