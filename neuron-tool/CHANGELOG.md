# Changelog

All notable changes to `neuron-tool` are documented here.
Maintained by [release-please](https://github.com/googleapis/release-please)
from [Conventional Commits](https://www.conventionalcommits.org/).

## [0.3.0](https://github.com/SecBear/neuron/compare/neuron-tool/v0.2.0...neuron-tool-v0.3.0) (2026-02-24)


### Features

* add new modules, examples, and tests ([dae838c](https://github.com/SecBear/neuron/commit/dae838c1a4d5aa802d2ec11bd9007d5428ea378f))
* usage limits, tool timeout, structured output validation, and OTel instrumentation ([#40](https://github.com/SecBear/neuron/issues/40)) ([1af2182](https://github.com/SecBear/neuron/commit/1af2182b52fb1864fa61ef86e6a50216f4202939))


### Bug Fixes

* replace invalid crates.io category slug artificial-intelligence with science ([f330701](https://github.com/SecBear/neuron/commit/f3307010c7c97964c8d039ad10c6a0556dec3838))
* resolve 4 CI failures (format, dead code, links, cargo-deny) ([7adaf89](https://github.com/SecBear/neuron/commit/7adaf89473f9a69a12e6966dfa361016bd0d4d03))

## [0.2.0] - 2026-02-22

### Added
- `derive_tool` example demonstrating `#[neuron_tool]` macro with custom types
- `categories` and `documentation` fields in Cargo.toml
