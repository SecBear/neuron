# Changelog

All notable changes to `neuron-types` are documented here.
Maintained by [release-please](https://github.com/googleapis/release-please)
from [Conventional Commits](https://www.conventionalcommits.org/).

## [0.2.0] - 2026-02-22

### Added
- `EmbeddingProvider` trait for embedding models, separate from `Provider`
- `EmbeddingRequest`, `EmbeddingResponse`, `EmbeddingUsage`, `EmbeddingError` types
- `LoopError::Cancelled` variant for cooperative cancellation
- `categories` and `documentation` fields in Cargo.toml for crates.io discoverability
