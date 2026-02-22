# Changelog

All notable changes to `neuron-loop` are documented here.
Maintained by [release-please](https://github.com/googleapis/release-please)
from [Conventional Commits](https://www.conventionalcommits.org/).

## [0.2.0] - 2026-02-22

### Added
- Cancellation support via `CancellationToken` — checked at loop top and before tool execution, returns `LoopError::Cancelled`
- Parallel tool execution — `parallel_tool_execution` flag on `LoopConfig` uses `futures::future::join_all`
- `cancellation` and `parallel_tools` examples
- `categories` and `documentation` fields in Cargo.toml
