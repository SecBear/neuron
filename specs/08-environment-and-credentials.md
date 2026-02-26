# Environment and Credentials

## Purpose

Environment defines how operator work runs under isolation and credential constraints.

This is where “local vs docker vs k8s” lives. The protocol should not change across these.

## Protocol

`layer0::Environment` defines:

- `run(input, spec) -> output`

`EnvironmentSpec` defines:

- isolation boundaries
- credential references + injection strategy
- resource limits
- network policy

## Credentials Integration

Credential *delivery* is an environment concern (env var, mounted file, sidecar).

Credential *source backend* is a secret/auth/crypto concern.

## Current Implementation Status

- `neuron-env-local` exists.

Stubs are acceptable for docker/k8s implementations right now.

Still required for “core complete”:

- documentation and tests proving that credentials are represented consistently end-to-end (source + injection)
- a reference “credential resolution + injection” pipeline for local mode (even if backend sources are stubbed)

