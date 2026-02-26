# Secrets Architecture Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement three composable security traits (SecretResolver, AuthProvider, CryptoProvider) with type-level enforcement, per-backend micro-crates, and hook-based safety.

**Architecture:** Layer 0 gets data types only (SecretSource, SecretAccessEvent, SecretAccessOutcome). Error types live in their respective trait crates, not layer0. Three trait crates (neuron-secret, neuron-auth, neuron-crypto) define the interfaces and their own errors. Twelve backend stub crates provide the correct trait impl shape. neuron-hook-security provides RedactionHook and ExfilGuardHook. All follow existing patterns (HookRegistry, ToolRegistry).

**Tech Stack:** Rust, serde, async-trait, thiserror, zeroize

---

## Build Environment

All commands must be run inside the nix dev shell:

```bash
cd /Users/bear/dev/neuron-explore
nix develop --command bash -c '<command>'
```

Or if direnv is active, just run commands directly in `/Users/bear/dev/neuron-explore`.

Full check suite (run after every phase):

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

---

## Phase S1: Layer 0 Data Types

Add SecretSource, SecretAccessEvent, SecretAccessOutcome to layer0. Update CredentialRef to include a source field. Error types (SecretError, AuthError, CryptoError) live in their respective trait crates, NOT in layer0 — keeps the stability contract minimal.

### Task S1.1: Create secret.rs module

**Files:**
- Create: `layer0/src/secret.rs`
- Modify: `layer0/src/lib.rs`

**Step 1: Create `layer0/src/secret.rs` with all data types**

```rust
//! Secret management data types — the stability contract for credential resolution.
//!
//! These are data types only. The actual resolution traits (`SecretResolver`,
//! `AuthProvider`, `CryptoProvider`) live in separate crates (`neuron-secret`,
//! `neuron-auth`, `neuron-crypto`). Layer 0 defines the vocabulary; higher
//! layers define the behavior.

use serde::{Deserialize, Serialize};

/// Where a secret is stored. This describes the BACKEND, not the delivery mechanism.
///
/// Delivery is handled by [`crate::environment::CredentialInjection`] (env var, file, sidecar).
/// A secret can live in Vault (source) and be delivered as an env var (injection) —
/// these are orthogonal concerns.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SecretSource {
    /// HashiCorp Vault.
    Vault {
        /// The Vault mount point (e.g., "secret", "kv").
        mount: String,
        /// Path within the mount (e.g., "data/api-keys/anthropic").
        path: String,
    },
    /// AWS Secrets Manager.
    AwsSecretsManager {
        /// The secret ID or ARN.
        secret_id: String,
        /// AWS region (uses default if None).
        region: Option<String>,
    },
    /// GCP Secret Manager.
    GcpSecretManager {
        /// GCP project ID.
        project: String,
        /// Secret ID within the project.
        secret_id: String,
    },
    /// Azure Key Vault.
    AzureKeyVault {
        /// The vault URL (e.g., "https://myvault.vault.azure.net").
        vault_url: String,
        /// The secret name within the vault.
        secret_name: String,
    },
    /// OS keystore (macOS Keychain, Windows DPAPI, Linux Secret Service).
    OsKeystore {
        /// The service name used to store/retrieve the credential.
        service: String,
    },
    /// Kubernetes Secret.
    Kubernetes {
        /// The namespace containing the secret.
        namespace: String,
        /// The secret resource name.
        name: String,
        /// The key within the secret's data map.
        key: String,
    },
    /// Hardware token (YubiKey PIV, HSM via PKCS#11).
    Hardware {
        /// The slot identifier (e.g., "9a" for YubiKey PIV auth slot).
        slot: String,
    },
    /// Custom source for future backends.
    Custom {
        /// The backend provider identifier.
        provider: String,
        /// Backend-specific configuration.
        config: serde_json::Value,
    },
}

/// Outcome of a secret access attempt.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SecretAccessOutcome {
    /// Secret was successfully resolved.
    Resolved,
    /// Access was denied by policy.
    Denied,
    /// Resolution failed (backend error, timeout, etc.).
    Failed,
    /// A lease was renewed.
    Renewed,
    /// A lease was released/revoked.
    Released,
}

/// Lifecycle event emitted when a secret is accessed.
///
/// This is part of the observability vocabulary (like [`crate::lifecycle::BudgetEvent`]).
/// The orchestrator or a hook can subscribe to these events for audit logging,
/// compliance tracking, or anomaly detection.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretAccessEvent {
    /// The credential name (label, not the secret value).
    pub credential_name: String,
    /// Where resolution was attempted.
    pub source: SecretSource,
    /// What happened.
    pub outcome: SecretAccessOutcome,
    /// When it happened (Unix timestamp milliseconds).
    pub timestamp_ms: u64,
    /// Opaque lease identifier for renewal/revocation tracking.
    pub lease_id: Option<String>,
    /// Lease TTL in seconds, if applicable.
    pub lease_ttl_secs: Option<u64>,
    /// Sanitized failure reason (never contains secret material).
    pub reason: Option<String>,
    /// Workflow ID for correlation.
    pub workflow_id: Option<String>,
    /// Agent/operator ID for correlation.
    pub agent_id: Option<String>,
    /// Trace ID for distributed tracing.
    pub trace_id: Option<String>,
}

impl SecretSource {
    /// Returns a short, telemetry-safe kind tag for this source variant.
    ///
    /// Safe to log, include in error messages, and use in metrics —
    /// never contains secret material.
    pub fn kind(&self) -> &'static str {
        match self {
            SecretSource::Vault { .. } => "vault",
            SecretSource::AwsSecretsManager { .. } => "aws",
            SecretSource::GcpSecretManager { .. } => "gcp",
            SecretSource::AzureKeyVault { .. } => "azure",
            SecretSource::OsKeystore { .. } => "os_keystore",
            SecretSource::Kubernetes { .. } => "kubernetes",
            SecretSource::Hardware { .. } => "hardware",
            SecretSource::Custom { .. } => "custom",
            _ => "unknown",
        }
    }
}

impl SecretAccessEvent {
    /// Create a new secret access event with required fields.
    pub fn new(
        credential_name: impl Into<String>,
        source: SecretSource,
        outcome: SecretAccessOutcome,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            credential_name: credential_name.into(),
            source,
            outcome,
            timestamp_ms,
            lease_id: None,
            lease_ttl_secs: None,
            reason: None,
            workflow_id: None,
            agent_id: None,
            trace_id: None,
        }
    }
}
```

**Step 2: Register the module in `layer0/src/lib.rs`**

After line 63 (`pub mod state;`), add:
```rust
pub mod secret;
```

After line 76 (`pub use lifecycle::{BudgetEvent, CompactionEvent, ObservableEvent};`), add:
```rust
pub use secret::{SecretAccessEvent, SecretAccessOutcome, SecretSource};
```

**Step 3: Run `cargo build -p layer0`**

Expected: compiles cleanly.

### Task S1.2: Add HookAction::ModifyToolOutput to hook.rs + wire in hook executor

The existing `HookAction` has `ModifyToolInput` but no output mutation variant. RedactionHook needs to modify tool OUTPUT. Adding the variant alone is not sufficient — the hook executor in neuron-op-react has a `_ => {}` catch-all that would silently drop it.

**Files:**
- Modify: `layer0/src/hook.rs`
- Modify: `neuron-op-react/src/lib.rs` (hook executor)

**Step 1: Add ModifyToolOutput variant to HookAction**

After the `ModifyToolInput` variant in `layer0/src/hook.rs`, add:
```rust
    /// Replace the tool output with a modified version (e.g., redacted secrets).
    /// Only valid at PostToolUse. v0 scope: PostToolUse only.
    /// Future: PostInference for redacting final assistant text before return/logging.
    ModifyToolOutput {
        /// The replacement output.
        new_output: serde_json::Value,
    },
```

**Step 2: Wire ModifyToolOutput in the hook executor**

In `neuron-op-react/src/lib.rs`, the PostToolUse hook dispatch currently only checks for `HookAction::Halt`. Find the PostToolUse dispatch site and add handling for `ModifyToolOutput` — replace the tool result content with the new output. The exact wiring depends on the current PostToolUse dispatch code shape, but the semantics are: if a PostToolUse hook returns `ModifyToolOutput`, replace the tool result content before it flows into the model context.

**Step 3: Run `cargo build && cargo test`**

Expected: compiles cleanly. Existing tests still pass (HookAction is `#[non_exhaustive]`, but the explicit match arm is better than the catch-all).

### Task S1.3: Update CredentialRef to include source field

**Files:**
- Modify: `layer0/src/environment.rs`

**Step 1: Add the import at the top of environment.rs**

After line 5 (`use serde::{Deserialize, Serialize};`), add:
```rust
use crate::secret::SecretSource;
```

**Step 2: Update the CredentialRef struct (lines 104-112)**

Replace:
```rust
/// A reference to a credential that should be injected into the environment.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRef {
    /// Name of the credential (e.g., "anthropic-api-key").
    pub name: String,
    /// How to inject it.
    pub injection: CredentialInjection,
}
```

With:
```rust
/// A reference to a credential that should be injected into the environment.
///
/// Combines three orthogonal concerns:
/// - `name`: what the credential is called (label for humans and audit logs)
/// - `source`: where the secret lives (backend — Vault, AWS, OS keystore, etc.)
/// - `injection`: how it's delivered to the operator (env var, file, sidecar)
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRef {
    /// Name of the credential (e.g., "anthropic-api-key").
    pub name: String,
    /// Where the secret lives (the backend).
    pub source: SecretSource,
    /// How to inject it into the execution environment (the delivery mechanism).
    pub injection: CredentialInjection,
}
```

**Step 3: Update the CredentialRef::new constructor (lines 180-188)**

Replace:
```rust
impl CredentialRef {
    /// Create a new credential reference.
    pub fn new(name: impl Into<String>, injection: CredentialInjection) -> Self {
        Self {
            name: name.into(),
            injection,
        }
    }
}
```

With:
```rust
impl CredentialRef {
    /// Create a new credential reference.
    pub fn new(
        name: impl Into<String>,
        source: SecretSource,
        injection: CredentialInjection,
    ) -> Self {
        Self {
            name: name.into(),
            source,
            injection,
        }
    }
}
```

**Step 4: Fix any tests that construct CredentialRef**

Search for `CredentialRef::new` and `CredentialRef {` in `tests/phase1.rs` and `tests/phase2.rs`. Update all call sites to include a `source` field. Use `SecretSource::OsKeystore { service: "test".into() }` as a default for tests.

**Step 5: Run `cargo build -p layer0 && cargo test -p layer0 --features test-utils`**

Expected: all tests pass.

### Task S1.4: Add tests for new layer0 types

**Files:**
- Modify: `layer0/tests/phase1.rs`

**Step 1: Add serde roundtrip tests for SecretSource**

Append to the file:

```rust
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Secret types — serde roundtrips and Display tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

use layer0::secret::{SecretAccessEvent, SecretAccessOutcome, SecretSource};

#[test]
fn secret_source_all_variants_round_trip() {
    let sources = vec![
        SecretSource::Vault {
            mount: "secret".into(),
            path: "data/api-key".into(),
        },
        SecretSource::AwsSecretsManager {
            secret_id: "arn:aws:secretsmanager:us-east-1:123:secret:api-key".into(),
            region: Some("us-east-1".into()),
        },
        SecretSource::GcpSecretManager {
            project: "my-project".into(),
            secret_id: "api-key".into(),
        },
        SecretSource::AzureKeyVault {
            vault_url: "https://myvault.vault.azure.net".into(),
            secret_name: "api-key".into(),
        },
        SecretSource::OsKeystore {
            service: "neuron-test".into(),
        },
        SecretSource::Kubernetes {
            namespace: "default".into(),
            name: "api-secrets".into(),
            key: "anthropic-key".into(),
        },
        SecretSource::Hardware {
            slot: "9a".into(),
        },
        SecretSource::Custom {
            provider: "1password".into(),
            config: json!({"vault": "Engineering"}),
        },
    ];
    for source in sources {
        let json = serde_json::to_string(&source).unwrap();
        let back: SecretSource = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2);
    }
}

#[test]
fn secret_access_outcome_all_variants_round_trip() {
    let outcomes = vec![
        SecretAccessOutcome::Resolved,
        SecretAccessOutcome::Denied,
        SecretAccessOutcome::Failed,
        SecretAccessOutcome::Renewed,
        SecretAccessOutcome::Released,
    ];
    for outcome in &outcomes {
        let json = serde_json::to_string(outcome).unwrap();
        let back: SecretAccessOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(*outcome, back);
    }
}

#[test]
fn secret_access_event_round_trip() {
    let event = SecretAccessEvent::new(
        "anthropic-api-key",
        SecretSource::Vault {
            mount: "secret".into(),
            path: "data/api-key".into(),
        },
        SecretAccessOutcome::Resolved,
        1740000000000,
    );
    let json = serde_json::to_string(&event).unwrap();
    let back: SecretAccessEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back.credential_name, "anthropic-api-key");
    assert_eq!(back.outcome, SecretAccessOutcome::Resolved);
    assert_eq!(back.timestamp_ms, 1740000000000);
}

#[test]
fn secret_access_event_with_all_fields() {
    let mut event = SecretAccessEvent::new(
        "db-password",
        SecretSource::AwsSecretsManager {
            secret_id: "prod/db/password".into(),
            region: Some("us-west-2".into()),
        },
        SecretAccessOutcome::Denied,
        1740000000000,
    );
    event.lease_id = Some("lease-abc-123".into());
    event.lease_ttl_secs = Some(3600);
    event.reason = Some("policy: requires mfa".into());
    event.workflow_id = Some("wf-001".into());
    event.agent_id = Some("agent-research".into());
    event.trace_id = Some("trace-xyz".into());

    let json = serde_json::to_string(&event).unwrap();
    let back: SecretAccessEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back.lease_id.as_deref(), Some("lease-abc-123"));
    assert_eq!(back.lease_ttl_secs, Some(3600));
    assert_eq!(back.reason.as_deref(), Some("policy: requires mfa"));
    assert_eq!(back.workflow_id.as_deref(), Some("wf-001"));
    assert_eq!(back.agent_id.as_deref(), Some("agent-research"));
    assert_eq!(back.trace_id.as_deref(), Some("trace-xyz"));
}

// Note: SecretError, AuthError, CryptoError Display tests live in their
// respective trait crate tests (neuron-secret, neuron-auth, neuron-crypto).

#[test]
fn credential_ref_with_source_round_trip() {
    use layer0::environment::{CredentialInjection, CredentialRef};

    let cred = CredentialRef::new(
        "anthropic-api-key",
        SecretSource::Vault {
            mount: "secret".into(),
            path: "data/anthropic".into(),
        },
        CredentialInjection::EnvVar {
            var_name: "ANTHROPIC_API_KEY".into(),
        },
    );
    let json = serde_json::to_string(&cred).unwrap();
    let back: CredentialRef = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, "anthropic-api-key");
}
```

**Step 2: Run full layer0 test suite**

```bash
cargo test -p layer0 --features test-utils
```

Expected: all existing tests + ~10 new tests pass.

### Task S1.5: Commit Phase S1

```bash
git add layer0/src/secret.rs layer0/src/hook.rs layer0/src/environment.rs layer0/src/lib.rs layer0/tests/phase1.rs layer0/tests/phase2.rs neuron-op-react/src/lib.rs
git commit -m "feat(layer0): add secret data types, HookAction::ModifyToolOutput, CredentialRef.source"
```

---

## Phase S2: neuron-secret crate

The core trait crate: SecretValue (in-memory wrapper), SecretLease, SecretResolver trait, SecretRegistry.

### Task S2.1: Create neuron-secret crate scaffold

**Files:**
- Create: `neuron-secret/Cargo.toml`
- Create: `neuron-secret/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: Create `neuron-secret/Cargo.toml`**

```toml
[package]
name = "neuron-secret"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
description = "Secret resolution traits and types for neuron"

[dependencies]
layer0 = { path = "../layer0" }
async-trait = "0.1"
thiserror = "2"
zeroize = "1"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

**Step 2: Create `neuron-secret/src/lib.rs` with all types and traits**

```rust
#![deny(missing_docs)]
//! Secret resolution for neuron.
//!
//! This crate defines the [`SecretResolver`] trait, the [`SecretValue`] in-memory
//! wrapper (no Serialize, no Display, no Clone — memory zeroed on drop), and the
//! [`SecretRegistry`] for composing multiple resolvers.
//!
//! ## Design
//!
//! - Resolvers resolve a [`SecretSource`] (from layer0), not a string name.
//!   The name→source mapping lives in [`CredentialRef`].
//! - [`SecretValue`] uses scoped exposure (`with_bytes`) to prevent accidental leaks.
//! - [`SecretRegistry`] dispatches by [`SecretSource`] variant, following the same
//!   composition pattern as `ToolRegistry` and `HookRegistry`.

use async_trait::async_trait;
use layer0::secret::SecretSource;
use std::sync::Arc;
use std::time::SystemTime;
use thiserror::Error;
use zeroize::Zeroizing;

/// Errors from secret resolution (crate-local, not in layer0).
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum SecretError {
    /// The secret was not found in the backend.
    #[error("secret not found: {0}")]
    NotFound(String),

    /// Access denied by policy.
    #[error("access denied: {0}")]
    AccessDenied(String),

    /// Backend communication failure (network, timeout, etc.).
    #[error("backend error: {0}")]
    BackendError(String),

    /// The lease has expired and cannot be renewed.
    #[error("lease expired: {0}")]
    LeaseExpired(String),

    /// No resolver registered for this source type.
    /// The string is the source kind tag (from `SecretSource::kind()`).
    #[error("no resolver for source: {0}")]
    NoResolver(String),

    /// Catch-all.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// An opaque secret value. Cannot be logged, serialized, or cloned.
/// Memory is zeroed on drop via [`Zeroizing`].
///
/// The only way to access the bytes is through [`SecretValue::with_bytes`],
/// which enforces scoped exposure — the secret is only visible inside the closure.
pub struct SecretValue {
    inner: Zeroizing<Vec<u8>>,
}

impl SecretValue {
    /// Create a new secret value. The input vector is moved, not copied.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            inner: Zeroizing::new(bytes),
        }
    }

    /// Scoped exposure. The secret bytes are only accessible inside the closure.
    /// This is the ONLY way to read the value.
    pub fn with_bytes<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        f(&self.inner)
    }

    /// Returns the length of the secret in bytes.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the secret is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

// Intentionally: no Display, no Clone, no Serialize, no PartialEq.

/// A resolved secret with optional lease information.
///
/// Leases allow time-bounded access to secrets. When a lease expires,
/// the secret must be re-resolved from the backend. Renewable leases
/// can be extended without re-authentication.
pub struct SecretLease {
    /// The resolved secret value.
    pub value: SecretValue,
    /// When this lease expires (None = no expiry).
    pub expires_at: Option<SystemTime>,
    /// Whether this lease can be renewed.
    pub renewable: bool,
    /// Opaque lease ID for renewal/revocation.
    pub lease_id: Option<String>,
}

impl SecretLease {
    /// Create a new lease with no expiry.
    pub fn permanent(value: SecretValue) -> Self {
        Self {
            value,
            expires_at: None,
            renewable: false,
            lease_id: None,
        }
    }

    /// Create a new lease with a TTL.
    pub fn with_ttl(value: SecretValue, ttl: std::time::Duration) -> Self {
        Self {
            value,
            expires_at: Some(SystemTime::now() + ttl),
            renewable: false,
            lease_id: None,
        }
    }

    /// Check if this lease has expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| SystemTime::now() > exp)
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for SecretLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretLease")
            .field("value", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("renewable", &self.renewable)
            .field("lease_id", &self.lease_id)
            .finish()
    }
}

/// Resolve a secret from a specific backend.
///
/// Implementations are backend-specific: `VaultResolver` talks to Vault,
/// `AwsResolver` talks to AWS Secrets Manager, `KeystoreResolver` talks to
/// the OS keychain, etc.
///
/// Resolvers do NOT map names to sources. That mapping lives in
/// `CredentialRef.source`. The resolver receives the source directly
/// and knows how to fetch from that backend.
#[async_trait]
pub trait SecretResolver: Send + Sync {
    /// Resolve a secret from the given source.
    async fn resolve(&self, source: &SecretSource) -> Result<SecretLease, SecretError>;
}

/// How to match a [`SecretSource`] variant to a resolver.
#[derive(Debug, Clone)]
pub enum SourceMatcher {
    /// Match all `SecretSource::Vault` variants.
    Vault,
    /// Match all `SecretSource::AwsSecretsManager` variants.
    Aws,
    /// Match all `SecretSource::GcpSecretManager` variants.
    Gcp,
    /// Match all `SecretSource::AzureKeyVault` variants.
    Azure,
    /// Match all `SecretSource::OsKeystore` variants.
    OsKeystore,
    /// Match all `SecretSource::Kubernetes` variants.
    Kubernetes,
    /// Match all `SecretSource::Hardware` variants.
    Hardware,
    /// Match a specific `SecretSource::Custom` provider name.
    Custom(String),
}

impl SourceMatcher {
    /// Check if this matcher matches the given source.
    pub fn matches(&self, source: &SecretSource) -> bool {
        match (self, source) {
            (SourceMatcher::Vault, SecretSource::Vault { .. }) => true,
            (SourceMatcher::Aws, SecretSource::AwsSecretsManager { .. }) => true,
            (SourceMatcher::Gcp, SecretSource::GcpSecretManager { .. }) => true,
            (SourceMatcher::Azure, SecretSource::AzureKeyVault { .. }) => true,
            (SourceMatcher::OsKeystore, SecretSource::OsKeystore { .. }) => true,
            (SourceMatcher::Kubernetes, SecretSource::Kubernetes { .. }) => true,
            (SourceMatcher::Hardware, SecretSource::Hardware { .. }) => true,
            (SourceMatcher::Custom(name), SecretSource::Custom { provider, .. }) => {
                name == provider
            }
            _ => false,
        }
    }
}

/// Composes multiple resolvers, routing by [`SecretSource`] variant.
///
/// When `resolve()` is called, the registry matches the source to a registered
/// resolver and delegates. If no resolver matches, returns `SecretError::NoResolver`.
///
/// Optionally emits [`SecretAccessEvent`]s through a [`SecretEventSink`] for audit logging.
pub struct SecretRegistry {
    resolvers: Vec<(SourceMatcher, Arc<dyn SecretResolver>)>,
    event_sink: Option<Arc<dyn SecretEventSink>>,
}

impl SecretRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            resolvers: Vec::new(),
            event_sink: None,
        }
    }

    /// Register a resolver for sources matching the given pattern.
    pub fn with_resolver(
        mut self,
        matcher: SourceMatcher,
        resolver: Arc<dyn SecretResolver>,
    ) -> Self {
        self.resolvers.push((matcher, resolver));
        self
    }

    /// Set the event sink for audit logging.
    pub fn with_event_sink(mut self, sink: Arc<dyn SecretEventSink>) -> Self {
        self.event_sink = Some(sink);
        self
    }

    /// Add a resolver for sources matching the given pattern.
    pub fn add(&mut self, matcher: SourceMatcher, resolver: Arc<dyn SecretResolver>) {
        self.resolvers.push((matcher, resolver));
    }
}

impl Default for SecretRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Optional event sink for audit logging of secret access.
///
/// The SecretRegistry emits [`SecretAccessEvent`]s through this sink.
/// Implementations can forward to an event bus, write to audit logs,
/// or feed anomaly detection systems.
///
/// If no sink is provided to SecretRegistry, events are silently dropped.
pub trait SecretEventSink: Send + Sync {
    /// Emit a secret access event.
    fn emit(&self, event: layer0::secret::SecretAccessEvent);
}

#[async_trait]
impl SecretResolver for SecretRegistry {
    async fn resolve(&self, source: &SecretSource) -> Result<SecretLease, SecretError> {
        for (matcher, resolver) in &self.resolvers {
            if matcher.matches(source) {
                let result = resolver.resolve(source).await;
                // Emit audit event if sink is configured
                if let Some(sink) = &self.event_sink {
                    use layer0::secret::{SecretAccessEvent, SecretAccessOutcome};
                    let outcome = if result.is_ok() {
                        SecretAccessOutcome::Resolved
                    } else {
                        SecretAccessOutcome::Failed
                    };
                    let event = SecretAccessEvent::new(
                        source.kind(),
                        source.clone(),
                        outcome,
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    );
                    sink.emit(event);
                }
                return result;
            }
        }
        Err(SecretError::NoResolver(source.kind().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_value_debug_is_redacted() {
        let secret = SecretValue::new(b"super-secret-key".to_vec());
        let debug = format!("{:?}", secret);
        assert_eq!(debug, "[REDACTED]");
        assert!(!debug.contains("super-secret"));
    }

    #[test]
    fn secret_value_with_bytes_exposes_content() {
        let secret = SecretValue::new(b"my-api-key".to_vec());
        secret.with_bytes(|bytes| {
            assert_eq!(bytes, b"my-api-key");
        });
    }

    #[test]
    fn secret_value_len() {
        let secret = SecretValue::new(b"12345".to_vec());
        assert_eq!(secret.len(), 5);
        assert!(!secret.is_empty());

        let empty = SecretValue::new(vec![]);
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn secret_lease_permanent_never_expires() {
        let lease = SecretLease::permanent(SecretValue::new(b"key".to_vec()));
        assert!(!lease.is_expired());
        assert!(lease.expires_at.is_none());
        assert!(!lease.renewable);
    }

    #[test]
    fn secret_lease_debug_redacts_value() {
        let lease = SecretLease::permanent(SecretValue::new(b"secret".to_vec()));
        let debug = format!("{:?}", lease);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn source_matcher_vault() {
        let matcher = SourceMatcher::Vault;
        assert!(matcher.matches(&SecretSource::Vault {
            mount: "secret".into(),
            path: "data/key".into(),
        }));
        assert!(!matcher.matches(&SecretSource::OsKeystore {
            service: "test".into(),
        }));
    }

    #[test]
    fn source_matcher_custom() {
        let matcher = SourceMatcher::Custom("1password".into());
        assert!(matcher.matches(&SecretSource::Custom {
            provider: "1password".into(),
            config: serde_json::json!({}),
        }));
        assert!(!matcher.matches(&SecretSource::Custom {
            provider: "bitwarden".into(),
            config: serde_json::json!({}),
        }));
    }

    // Object safety
    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn secret_resolver_is_object_safe_send_sync() {
        _assert_send_sync::<Box<dyn SecretResolver>>();
        _assert_send_sync::<Arc<dyn SecretResolver>>();
    }

    #[tokio::test]
    async fn registry_no_resolver_returns_error() {
        let registry = SecretRegistry::new();
        let source = SecretSource::Vault {
            mount: "secret".into(),
            path: "data/key".into(),
        };
        let result = registry.resolve(&source).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SecretError::NoResolver(_)));
    }

    // Test registry dispatches to correct resolver
    struct StubResolver {
        value: &'static [u8],
    }

    #[async_trait]
    impl SecretResolver for StubResolver {
        async fn resolve(&self, _source: &SecretSource) -> Result<SecretLease, SecretError> {
            Ok(SecretLease::permanent(SecretValue::new(
                self.value.to_vec(),
            )))
        }
    }

    #[tokio::test]
    async fn registry_dispatches_to_matching_resolver() {
        let registry = SecretRegistry::new()
            .with_resolver(
                SourceMatcher::Vault,
                Arc::new(StubResolver { value: b"vault-secret" }),
            )
            .with_resolver(
                SourceMatcher::OsKeystore,
                Arc::new(StubResolver {
                    value: b"keystore-secret",
                }),
            );

        let vault_source = SecretSource::Vault {
            mount: "secret".into(),
            path: "data/key".into(),
        };
        let lease = registry.resolve(&vault_source).await.unwrap();
        lease.value.with_bytes(|b| assert_eq!(b, b"vault-secret"));

        let keystore_source = SecretSource::OsKeystore {
            service: "test".into(),
        };
        let lease = registry.resolve(&keystore_source).await.unwrap();
        lease
            .value
            .with_bytes(|b| assert_eq!(b, b"keystore-secret"));
    }
}
```

**Step 3: Add to workspace members in `Cargo.toml`**

Add `"neuron-secret"` to the workspace members list.

**Step 4: Run tests**

```bash
cargo build -p neuron-secret && cargo test -p neuron-secret && cargo clippy -p neuron-secret -- -D warnings
```

Expected: all tests pass, zero warnings.

### Task S2.2: Commit Phase S2

```bash
git add neuron-secret/ Cargo.toml Cargo.lock
git commit -m "feat: add neuron-secret — SecretResolver trait, SecretValue, SecretRegistry"
```

---

## Phase S3: neuron-auth crate

AuthProvider trait, AuthRequest, AuthToken, AuthProviderChain.

### Task S3.1: Create neuron-auth crate

**Files:**
- Create: `neuron-auth/Cargo.toml`
- Create: `neuron-auth/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: Create `neuron-auth/Cargo.toml`**

```toml
[package]
name = "neuron-auth"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
description = "Authentication provider traits for neuron"

[dependencies]
layer0 = { path = "../layer0" }
neuron-secret = { path = "../neuron-secret" }
async-trait = "0.1"
thiserror = "2"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

**Step 2: Create `neuron-auth/src/lib.rs`**

```rust
#![deny(missing_docs)]
//! Authentication providers for neuron.
//!
//! This crate defines the [`AuthProvider`] trait for obtaining authentication
//! credentials to access secret backends. It also provides [`AuthProviderChain`]
//! for composing multiple providers (try in order until one succeeds, like
//! AWS DefaultCredentialsChain).
//!
//! ## Separation of Concerns
//!
//! Auth providers produce credentials (tokens). Secret resolvers consume them.
//! A `VaultResolver` takes an `Arc<dyn AuthProvider>` and uses it to authenticate
//! before fetching secrets. This separation follows the pattern established by
//! AWS SDK (`ProvideCredentials` vs `SecretsManagerClient`), vaultrs
//! (`auth::*` vs `kv2::*`), and Google Cloud SDK.

use async_trait::async_trait;
use neuron_secret::SecretValue;
use std::sync::Arc;
use std::time::SystemTime;
use thiserror::Error;

/// Errors from authentication providers (crate-local, not in layer0).
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum AuthError {
    /// Authentication failed (bad credentials, expired token, etc.).
    #[error("auth failed: {0}")]
    AuthFailed(String),

    /// The requested scope or audience is not available.
    #[error("scope unavailable: {0}")]
    ScopeUnavailable(String),

    /// Backend communication failure.
    #[error("backend error: {0}")]
    BackendError(String),

    /// Catch-all.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Context for an authentication request.
///
/// Different backends need different context:
/// - OIDC needs audience and scope
/// - AWS needs region and role
/// - K8s SA token needs namespace and service account
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct AuthRequest {
    /// Target audience (OIDC audience, API identifier).
    pub audience: Option<String>,
    /// Requested scopes (OIDC scopes, OAuth2 scopes).
    pub scopes: Vec<String>,
    /// Target resource identifier (e.g., Vault path, AWS region).
    pub resource: Option<String>,
    /// Actor identity for audit (workflow ID, agent ID).
    pub actor: Option<String>,
}

impl AuthRequest {
    /// Create an empty auth request (no specific context).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the target audience.
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    /// Add a scope.
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scopes.push(scope.into());
        self
    }

    /// Set the target resource.
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    /// Set the actor identity.
    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }
}

/// An opaque authentication token with expiry.
///
/// Uses [`SecretValue`] internally for in-memory protection (zeroize on drop,
/// no Display, no Serialize). Access the token bytes via [`AuthToken::with_bytes`].
pub struct AuthToken {
    inner: SecretValue,
    expires_at: Option<SystemTime>,
}

impl AuthToken {
    /// Create a new auth token.
    pub fn new(bytes: Vec<u8>, expires_at: Option<SystemTime>) -> Self {
        Self {
            inner: SecretValue::new(bytes),
            expires_at,
        }
    }

    /// Create a token that never expires (for dev/test).
    pub fn permanent(bytes: Vec<u8>) -> Self {
        Self::new(bytes, None)
    }

    /// Scoped exposure of the token bytes.
    pub fn with_bytes<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        self.inner.with_bytes(f)
    }

    /// Check if this token has expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| SystemTime::now() > exp)
            .unwrap_or(false)
    }

    /// Returns when this token expires, if known.
    pub fn expires_at(&self) -> Option<SystemTime> {
        self.expires_at
    }
}

impl std::fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthToken")
            .field("value", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Provide authentication credentials for accessing a secret backend.
///
/// Implementations: `OidcAuthProvider` (token exchange), `K8sAuthProvider`
/// (service account token), `StaticAuthProvider` (hardcoded, dev only),
/// `FileTokenProvider` (reads from mounted file).
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Provide an authentication token for the given request context.
    async fn provide(&self, request: &AuthRequest) -> Result<AuthToken, AuthError>;
}

/// Tries providers in order until one succeeds.
///
/// Modeled after AWS `DefaultCredentialsChain`. If all providers fail,
/// returns the last error. If no providers are configured, returns
/// `AuthError::AuthFailed`.
pub struct AuthProviderChain {
    providers: Vec<Arc<dyn AuthProvider>>,
}

impl AuthProviderChain {
    /// Create a new empty chain.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Add a provider to the end of the chain.
    pub fn with_provider(mut self, provider: Arc<dyn AuthProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    /// Add a provider to the end of the chain (mutable).
    pub fn add(&mut self, provider: Arc<dyn AuthProvider>) {
        self.providers.push(provider);
    }
}

impl Default for AuthProviderChain {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthProvider for AuthProviderChain {
    async fn provide(&self, request: &AuthRequest) -> Result<AuthToken, AuthError> {
        let mut last_err = None;
        for provider in &self.providers {
            match provider.provide(request).await {
                Ok(token) => return Ok(token),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err
            .unwrap_or_else(|| AuthError::AuthFailed("no providers configured".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn auth_provider_is_object_safe_send_sync() {
        _assert_send_sync::<Box<dyn AuthProvider>>();
        _assert_send_sync::<Arc<dyn AuthProvider>>();
    }

    #[test]
    fn auth_token_debug_is_redacted() {
        let token = AuthToken::permanent(b"secret-token".to_vec());
        let debug = format!("{:?}", token);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret-token"));
    }

    #[test]
    fn auth_token_with_bytes_exposes_content() {
        let token = AuthToken::permanent(b"my-token".to_vec());
        token.with_bytes(|bytes| {
            assert_eq!(bytes, b"my-token");
        });
    }

    #[test]
    fn auth_token_permanent_never_expires() {
        let token = AuthToken::permanent(b"token".to_vec());
        assert!(!token.is_expired());
        assert!(token.expires_at().is_none());
    }

    #[test]
    fn auth_request_builder() {
        let req = AuthRequest::new()
            .with_audience("https://vault.internal")
            .with_scope("read:secrets")
            .with_scope("write:audit")
            .with_resource("secret/data/api-key")
            .with_actor("workflow-001");

        assert_eq!(req.audience.as_deref(), Some("https://vault.internal"));
        assert_eq!(req.scopes.len(), 2);
        assert_eq!(req.resource.as_deref(), Some("secret/data/api-key"));
        assert_eq!(req.actor.as_deref(), Some("workflow-001"));
    }

    struct AlwaysFailProvider;

    #[async_trait]
    impl AuthProvider for AlwaysFailProvider {
        async fn provide(&self, _request: &AuthRequest) -> Result<AuthToken, AuthError> {
            Err(AuthError::AuthFailed("always fails".into()))
        }
    }

    struct StaticTokenProvider {
        token: Vec<u8>,
    }

    #[async_trait]
    impl AuthProvider for StaticTokenProvider {
        async fn provide(&self, _request: &AuthRequest) -> Result<AuthToken, AuthError> {
            Ok(AuthToken::permanent(self.token.clone()))
        }
    }

    #[tokio::test]
    async fn chain_empty_returns_error() {
        let chain = AuthProviderChain::new();
        let result = chain.provide(&AuthRequest::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn chain_first_success_wins() {
        let chain = AuthProviderChain::new()
            .with_provider(Arc::new(StaticTokenProvider {
                token: b"first".to_vec(),
            }))
            .with_provider(Arc::new(StaticTokenProvider {
                token: b"second".to_vec(),
            }));

        let token = chain.provide(&AuthRequest::new()).await.unwrap();
        token.with_bytes(|b| assert_eq!(b, b"first"));
    }

    #[tokio::test]
    async fn chain_skips_failures() {
        let chain = AuthProviderChain::new()
            .with_provider(Arc::new(AlwaysFailProvider))
            .with_provider(Arc::new(StaticTokenProvider {
                token: b"fallback".to_vec(),
            }));

        let token = chain.provide(&AuthRequest::new()).await.unwrap();
        token.with_bytes(|b| assert_eq!(b, b"fallback"));
    }

    #[tokio::test]
    async fn chain_all_fail_returns_last_error() {
        let chain = AuthProviderChain::new()
            .with_provider(Arc::new(AlwaysFailProvider))
            .with_provider(Arc::new(AlwaysFailProvider));

        let result = chain.provide(&AuthRequest::new()).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "auth failed: always fails");
    }
}
```

**Step 3: Add to workspace members in `Cargo.toml`**

Add `"neuron-auth"` to the workspace members list.

**Step 4: Run tests**

```bash
cargo build -p neuron-auth && cargo test -p neuron-auth && cargo clippy -p neuron-auth -- -D warnings
```

### Task S3.2: Commit Phase S3

```bash
git add neuron-auth/ Cargo.toml Cargo.lock
git commit -m "feat: add neuron-auth — AuthProvider trait, AuthToken, AuthProviderChain"
```

---

## Phase S4: neuron-crypto crate

### Task S4.1: Create neuron-crypto crate

**Files:**
- Create: `neuron-crypto/Cargo.toml`
- Create: `neuron-crypto/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: Create `neuron-crypto/Cargo.toml`**

```toml
[package]
name = "neuron-crypto"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
description = "Cryptographic provider traits for neuron"

[dependencies]
layer0 = { path = "../layer0" }
async-trait = "0.1"
thiserror = "2"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

**Step 2: Create `neuron-crypto/src/lib.rs`**

```rust
#![deny(missing_docs)]
//! Cryptographic operations for neuron.
//!
//! This crate defines the [`CryptoProvider`] trait for cryptographic operations
//! where private keys never leave the provider boundary (Vault Transit, PKCS#11,
//! HSM, YubiKey, KMS).
//!
//! The consumer sends data in and gets results out. Private keys are never
//! exposed — this is the fundamental security property of hardware security
//! modules and transit encryption engines.

use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;

/// Errors from cryptographic operations (crate-local, not in layer0).
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum CryptoError {
    /// The referenced key was not found.
    #[error("key not found: {0}")]
    KeyNotFound(String),

    /// The operation is not supported for this key type or algorithm.
    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),

    /// The cryptographic operation failed.
    #[error("crypto operation failed: {0}")]
    OperationFailed(String),

    /// Catch-all.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Cryptographic operations where private keys never leave the provider boundary.
///
/// Implementations:
/// - `VaultTransitProvider`: Vault Transit engine (encrypt, decrypt, sign, verify)
/// - `HardwareProvider`: PKCS#11 / YubiKey PIV (sign, decrypt with on-device keys)
/// - `KmsProvider`: AWS KMS / GCP KMS / Azure Key Vault (envelope encryption)
///
/// The `key_ref` parameter is an opaque identifier meaningful to the implementation:
/// - Vault Transit: the key name in the transit engine
/// - PKCS#11: slot + key label
/// - YubiKey: PIV slot (e.g., "9a", "9c")
/// - KMS: key ARN or resource ID
#[async_trait]
pub trait CryptoProvider: Send + Sync {
    /// Sign data with the referenced key.
    async fn sign(
        &self,
        key_ref: &str,
        algorithm: &str,
        data: &[u8],
    ) -> Result<Vec<u8>, CryptoError>;

    /// Verify a signature against the referenced key.
    async fn verify(
        &self,
        key_ref: &str,
        algorithm: &str,
        data: &[u8],
        signature: &[u8],
    ) -> Result<bool, CryptoError>;

    /// Encrypt data with the referenced key.
    async fn encrypt(
        &self,
        key_ref: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError>;

    /// Decrypt data with the referenced key.
    async fn decrypt(
        &self,
        key_ref: &str,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn crypto_provider_is_object_safe_send_sync() {
        _assert_send_sync::<Box<dyn CryptoProvider>>();
        _assert_send_sync::<Arc<dyn CryptoProvider>>();
    }

    struct NoopCryptoProvider;

    #[async_trait]
    impl CryptoProvider for NoopCryptoProvider {
        async fn sign(
            &self,
            _key_ref: &str,
            _algorithm: &str,
            data: &[u8],
        ) -> Result<Vec<u8>, CryptoError> {
            // Stub: return data as "signature"
            Ok(data.to_vec())
        }

        async fn verify(
            &self,
            _key_ref: &str,
            _algorithm: &str,
            data: &[u8],
            signature: &[u8],
        ) -> Result<bool, CryptoError> {
            Ok(data == signature)
        }

        async fn encrypt(
            &self,
            _key_ref: &str,
            plaintext: &[u8],
        ) -> Result<Vec<u8>, CryptoError> {
            // Stub: return plaintext as "ciphertext"
            Ok(plaintext.to_vec())
        }

        async fn decrypt(
            &self,
            _key_ref: &str,
            ciphertext: &[u8],
        ) -> Result<Vec<u8>, CryptoError> {
            Ok(ciphertext.to_vec())
        }
    }

    #[tokio::test]
    async fn noop_provider_sign_verify_roundtrip() {
        let provider = NoopCryptoProvider;
        let data = b"hello world";
        let sig = provider.sign("key-1", "ed25519", data).await.unwrap();
        let valid = provider.verify("key-1", "ed25519", data, &sig).await.unwrap();
        assert!(valid);
    }

    #[tokio::test]
    async fn noop_provider_encrypt_decrypt_roundtrip() {
        let provider = NoopCryptoProvider;
        let plaintext = b"secret message";
        let ciphertext = provider.encrypt("key-1", plaintext).await.unwrap();
        let decrypted = provider.decrypt("key-1", &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
```

**Step 3: Add to workspace members. Run tests.**

```bash
cargo build -p neuron-crypto && cargo test -p neuron-crypto && cargo clippy -p neuron-crypto -- -D warnings
```

### Task S4.2: Commit Phase S4

```bash
git add neuron-crypto/ Cargo.toml Cargo.lock
git commit -m "feat: add neuron-crypto — CryptoProvider trait for hardware/transit crypto"
```

---

## Phase S5: Backend Stub Crates

Create all 12 backend stub crates. Each has the correct trait impl shape but returns stub data. Group into 3 commits (secret resolvers, auth providers, crypto providers).

### Task S5.1: Secret resolver stubs (6 crates)

Create these crates, each following the same pattern:

**neuron-secret-env** — `EnvResolver` reads from process env vars (functional, for dev/test):

Uses `SecretSource::Custom { provider: "env", config: { "var_name": "..." } }` — NOT `OsKeystore`,
which has different semantics (macOS Keychain, DPAPI, etc.).

**Config schema** (document in doc comment):
```json
{ "type": "custom", "provider": "env", "config": { "var_name": "ANTHROPIC_API_KEY" } }
```
Required field: `var_name` (string) — the environment variable name to read.

```rust
// Cargo.toml deps: neuron-secret, layer0, async-trait, serde_json
pub struct EnvResolver;

#[async_trait]
impl SecretResolver for EnvResolver {
    async fn resolve(&self, source: &SecretSource) -> Result<SecretLease, SecretError> {
        match source {
            SecretSource::Custom { provider, config } if provider == "env" => {
                let var_name = config.get("var_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SecretError::NotFound(
                        "env source requires config.var_name".into()
                    ))?;
                match std::env::var(var_name) {
                    Ok(val) => Ok(SecretLease::permanent(SecretValue::new(val.into_bytes()))),
                    Err(_) => Err(SecretError::NotFound(
                        format!("env var {} not set", var_name)
                    )),
                }
            }
            _ => Err(SecretError::NoResolver("env".into())),
        }
    }
}
```

Register with: `registry.with_resolver(SourceMatcher::Custom("env".into()), Arc::new(EnvResolver))`

**neuron-secret-vault**, **neuron-secret-aws**, **neuron-secret-gcp**, **neuron-secret-keystore**, **neuron-secret-k8s** — all stubs:

Each follows this pattern (using VaultResolver as example):

```rust
// Cargo.toml deps: neuron-secret, neuron-auth, layer0, async-trait
pub struct VaultResolver {
    _addr: String,
    _auth: Arc<dyn AuthProvider>,
}

impl VaultResolver {
    pub fn new(addr: impl Into<String>, auth: Arc<dyn AuthProvider>) -> Self {
        Self { _addr: addr.into(), _auth: auth }
    }
}

#[async_trait]
impl SecretResolver for VaultResolver {
    async fn resolve(&self, source: &SecretSource) -> Result<SecretLease, SecretError> {
        match source {
            SecretSource::Vault { mount, path } => {
                Err(SecretError::BackendError(format!(
                    "VaultResolver is a stub — would resolve {}/{}", mount, path
                )))
            }
            _ => Err(SecretError::NoResolver("not a Vault source".into())),
        }
    }
}
```

For each crate: Cargo.toml, src/lib.rs with struct + trait impl + tests (object safety, correct source matching, rejection of wrong sources).

Add all 6 to workspace members.

**Commit:**

```bash
git add neuron-secret-env/ neuron-secret-vault/ neuron-secret-aws/ neuron-secret-gcp/ neuron-secret-keystore/ neuron-secret-k8s/ Cargo.toml Cargo.lock
git commit -m "feat: add secret resolver stubs — env, vault, aws, gcp, keystore, k8s"
```

### Task S5.2: Auth provider stubs (4 crates)

**neuron-auth-static** — `StaticAuthProvider` (functional, for dev/test):

```rust
pub struct StaticAuthProvider { token: Vec<u8> }

impl StaticAuthProvider {
    pub fn new(token: impl Into<Vec<u8>>) -> Self { Self { token: token.into() } }
}

#[async_trait]
impl AuthProvider for StaticAuthProvider {
    async fn provide(&self, _request: &AuthRequest) -> Result<AuthToken, AuthError> {
        Ok(AuthToken::permanent(self.token.clone()))
    }
}
```

**neuron-auth-oidc**, **neuron-auth-k8s**, **neuron-auth-file** — all stubs following the same pattern.

`FileTokenProvider` reads a token from a file path (partially functional):

```rust
pub struct FileTokenProvider { path: PathBuf }

#[async_trait]
impl AuthProvider for FileTokenProvider {
    async fn provide(&self, _request: &AuthRequest) -> Result<AuthToken, AuthError> {
        let bytes = tokio::fs::read(&self.path).await
            .map_err(|e| AuthError::BackendError(format!("failed to read token file: {}", e)))?;
        Ok(AuthToken::permanent(bytes))
    }
}
```

Add all 4 to workspace members.

**Commit:**

```bash
git add neuron-auth-static/ neuron-auth-oidc/ neuron-auth-k8s/ neuron-auth-file/ Cargo.toml Cargo.lock
git commit -m "feat: add auth provider stubs — static, oidc, k8s, file"
```

### Task S5.3: Crypto provider stubs (2 crates)

**neuron-crypto-vault** — `VaultTransitProvider` (stub)
**neuron-crypto-hardware** — `HardwareProvider` (stub)

Both follow the same pattern: struct with constructor, trait impl that returns `CryptoError::UnsupportedOperation("stub")`, tests for object safety.

Add both to workspace members.

**Commit:**

```bash
git add neuron-crypto-vault/ neuron-crypto-hardware/ Cargo.toml Cargo.lock
git commit -m "feat: add crypto provider stubs — vault transit, hardware/pkcs11"
```

---

## Phase S6: neuron-hook-security

### Task S6.1: Create neuron-hook-security crate

**Files:**
- Create: `neuron-hook-security/Cargo.toml`
- Create: `neuron-hook-security/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: Create `neuron-hook-security/Cargo.toml`**

```toml
[package]
name = "neuron-hook-security"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
description = "Security hooks for neuron — redaction and exfiltration detection"

[dependencies]
layer0 = { path = "../layer0" }
async-trait = "0.1"
regex = "1"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

**Step 2: Create `neuron-hook-security/src/lib.rs`**

Two hooks:

**RedactionHook** — fires at `PostToolUse`. Scans tool output for patterns matching known
secret formats. Returns `HookAction::ModifyToolOutput` (the NEW variant added in S1.2)
with matches replaced by `[REDACTED]`. If no patterns match, returns `HookAction::Continue`.

**v0 scope:** `PostToolUse` only — redacts tool output before the model sees it.
**Future:** Add `PostInference` hook point to redact final assistant text before it's
returned to the user or logged. Not blocking for v0 — tool output is the primary leak vector.

**Intentionally narrow patterns for v0** (avoid regex false positive arms race):
- AWS access keys: `AKIA[A-Z0-9]{16}`
- Vault tokens: `hvs\.[a-zA-Z0-9_-]+`
- GitHub tokens: `gh[ps]_[a-zA-Z0-9]{36}`
- User-supplied custom patterns via `RedactionHook::with_pattern(regex)`

NOT included in v0: "generic API key" detection. Future: fingerprint-based redaction
(HMAC of resolved secret values for exact-match without regex).

**ExfilGuardHook** — fires at `PreToolUse`. Checks if tool input looks like it's trying
to exfiltrate data (e.g., base64 blobs being sent to external URLs, shell commands
piping env vars to curl). Returns `HookAction::Halt` on match.

Both implement `Hook` from layer0. Tests cover: pattern matching, no false positives
on normal text content, halting on exfil attempts, custom pattern registration.

**Commit:**

```bash
git add neuron-hook-security/ Cargo.toml Cargo.lock
git commit -m "feat: add neuron-hook-security — RedactionHook, ExfilGuardHook"
```

---

## Phase S7: Full Check + Workspace Integration

### Task S7.1: Run full workspace check suite

Each crate tests itself — no need to add all new crates to workspace root dev-dependencies
(avoids monolith creep). Only add to root dev-deps if cross-crate integration tests are
needed later.

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

Expected: All existing tests + all new tests pass. Zero warnings. Clean docs.

### Task S7.3: Final commit

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add secrets crates to workspace dev-dependencies"
```

---

## Verification

After all phases, run the full check suite:

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

Expected:
- All existing 352+ tests pass
- ~50+ new tests from secrets crates
- Zero clippy warnings
- Clean rustdoc

## Crate Count Summary

| Phase | Crates Created | Type |
|-------|---------------|------|
| S1 | 0 (layer0 changes) | Data types |
| S2 | neuron-secret | Trait crate |
| S3 | neuron-auth | Trait crate |
| S4 | neuron-crypto | Trait crate |
| S5 | 12 backend stubs | Implementation stubs |
| S6 | neuron-hook-security | Hook implementations |
| **Total** | **16 new crates** | |
