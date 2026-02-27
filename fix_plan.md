# fix_plan.md

This file is the single "what next" queue used by the Ralph loop (`PROMPT.md`).

Rules:

1. Keep it short.
2. Each item must link to the governing spec(s).
3. Each item must have a concrete "done when" and a verification command.

## Queue

1. Implement `brain` v1 (controller + worker tools + MCP config)
   - Spec: `specs/14-brain-agentic-research-assistant.md`
   - Done when:
     - A new `brain` crate exists with an offline integration test proving:
       - controller selects a worker tool
       - worker uses mock provider and returns structured JSON
       - controller synthesizes into final answer
     - `brain` loads a Claude/Codex-compatible `.mcp.json` (with optional `x-brain`) and assembles tools
   - Verify: `nix develop -c cargo test --workspace --all-targets`

2. Make orchestration “core complete” for composed systems
   - Specs: `specs/03-effects-and-execution-semantics.md`, `specs/05-orchestration-core.md`, `specs/06-composition-factory-and-glue.md`, `specs/11-testing-examples-and-backpressure.md`
   - Done when:
     - `neuron-orch-kit` has an integration-style test proving an effect pipeline end-to-end:
       - `WriteMemory`/`DeleteMemory` executed against a StateStore
       - `Delegate`/`Handoff` become follow-up dispatches
       - `Signal` calls `Orchestrator::signal` and is observable
     - `neuron-orch-local` defines and tests non-trivial `signal`/`query` semantics (not noop/null)
   - Verify: `nix develop -c cargo test --workspace --all-targets`

3. Implement credential resolution + injection + audit story in local mode
   - Specs: `specs/08-environment-and-credentials.md`, `specs/10-secrets-auth-crypto.md`, `specs/09-hooks-lifecycle-and-governance.md`
   - Done when:
     - Local execution demonstrates a coherent pipeline: `SecretSource` resolution → environment injection → lifecycle/audit event emission
     - Tests prove secret material does not leak into logs/errors by default (redaction + sanitized errors)
   - Verify: `nix develop -c cargo test --workspace --all-targets`

4. Add CI (hard enforcement) for formatting, tests, and clippy
   - Specs: `specs/13-documentation-and-dx-parity.md`
   - Done when:
     - GitHub Actions runs `pre-commit` (treefmt), `cargo test`, `cargo clippy` in the Nix dev shell
   - Verify: `nix develop -c cargo test --workspace --all-targets`

5. Add root README + crate map + quickstart
   - Spec: `specs/13-documentation-and-dx-parity.md`
   - Done when:
     - A new `README.md` exists with a minimal quickstart for local composition
     - Includes a crate map and points to `SPECS.md` for requirements
   - Verify: `nix develop -c cargo test --workspace --all-targets`

6. Add umbrella `neuron` crate with feature flags + prelude
   - Spec: `specs/12-packaging-versioning-and-umbrella-crate.md`
   - Done when:
     - `neuron/` crate exists and re-exports the happy-path set behind features
   - Verify: `nix develop -c cargo test --workspace --all-targets`
