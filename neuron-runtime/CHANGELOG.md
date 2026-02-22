# Changelog

All notable changes to `neuron-runtime` are documented here.
Maintained by [release-please](https://github.com/googleapis/release-please)
from [Conventional Commits](https://www.conventionalcommits.org/).

## [0.2.0] - 2026-02-22

### Added
- `TracingHook` — concrete `ObservabilityHook` using the `tracing` crate, maps all 8 hook events to structured spans
- `GuardrailHook` — `ObservabilityHook` adapter that wires input/output guardrails into the hook system (builder pattern)
- `tracing_hook`, `local_durable`, and `full_production` examples
- `categories` and `documentation` fields in Cargo.toml
