# Secrets Architecture Design

> Three traits (SecretResolver, AuthProvider, CryptoProvider), composable registries,
> per-backend micro-crates, and type-level enforcement against accidental secret exposure.

## Context

Layer 0 already defines the credential protocol boundary: `CredentialRef` declares WHAT
credentials an operator needs, `CredentialInjection` declares HOW they're delivered, and the
`Environment` trait implementation handles the actual injection. But no infrastructure exists
to resolve, authenticate, or manage credentials.

Research across 10+ technology domains (cloud secret managers, HashiCorp Vault, OIDC,
SPIFFE/SPIRE, OS keystores, hardware tokens, Kubernetes, CI/CD, encrypted-at-rest stores,
and the Rust crate ecosystem) reveals three distinct capability clusters that map to separate
trait boundaries. The mature Rust crates (AWS SDK, vaultrs, google-cloud-*, spiffe, secrecy)
consistently separate authentication from secret resolution, and all use some form of
in-memory wrapper to prevent accidental exposure.

## Design Principles

1. **Layer 0 = stability contract** — only data types in layer0. Traits live in separate crates.
2. **SecretSource = backend, CredentialInjection = delivery** — orthogonal concerns, no overlap.
3. **Resolvers resolve sources, not names** — `CredentialRef` maps name to source; resolver
   operates on `SecretSource` directly. No hidden global mapping.
4. **Type-level enforcement** — `SecretValue` has no `Serialize`, no `Display`, no `Clone`.
   Memory zeroed on drop. Scoped exposure only via `with_bytes` closure.
5. **Composition via registries** — same pattern as `ToolRegistry` and `HookRegistry`.
6. **Hooks for cross-cutting concerns** — redaction and exfil detection are hooks, not
   protocol methods.

## Three Capability Clusters

| Cluster | Trait | Operations | Examples |
|---------|-------|-----------|----------|
| Secret Storage & Retrieval | `SecretResolver` | resolve | Vault KV, AWS SM, GCP SM, OS keystore, K8s Secrets |
| Credential Issuance & Auth | `AuthProvider` | provide (with audience/scope/context) | OIDC, k8s SA token, static token, file token |
| Cryptographic Operations | `CryptoProvider` | sign, verify, encrypt, decrypt | Vault Transit, PKCS#11, HSM, YubiKey, KMS |

## Crate Layout

```
layer0/src/secret.rs           Data types ONLY: SecretSource, SecretAccessEvent,
                               SecretAccessOutcome.
                               Add `source: SecretSource` to CredentialRef (required, not Option).

neuron-secret/                 SecretResolver trait, SecretValue (zeroize wrapper),
                               SecretLease (TTL), SecretRegistry (routes by SecretSource variant),
                               SecretError (crate-local, not in layer0).

neuron-auth/                   AuthProvider trait, AuthRequest, AuthToken (opaque, time-bounded),
                               AuthProviderChain (tries providers in order),
                               AuthError (crate-local, not in layer0).

neuron-crypto/                 CryptoProvider trait, CryptoError (crate-local, not in layer0).

--- Backend stubs (trait + correct shape, no real SDK calls) ---

neuron-secret-env/             EnvResolver: reads process env vars. Dev/test only.
neuron-secret-vault/           VaultResolver: HashiCorp Vault KV. Takes Arc<dyn AuthProvider>.
neuron-secret-aws/             AwsResolver: AWS Secrets Manager. Takes Arc<dyn AuthProvider>.
neuron-secret-gcp/             GcpResolver: GCP Secret Manager. Takes Arc<dyn AuthProvider>.
neuron-secret-keystore/        KeystoreResolver: OS keystores (macOS Keychain, Windows, Linux).
neuron-secret-k8s/             K8sResolver: Kubernetes Secrets / CSI driver.

neuron-auth-static/            StaticAuthProvider: hardcoded token. Dev/test only.
neuron-auth-oidc/              OidcAuthProvider: OIDC client credentials / token exchange.
neuron-auth-k8s/               K8sAuthProvider: k8s ServiceAccount projected token.
neuron-auth-file/              FileTokenProvider: reads token from mounted file.

neuron-crypto-vault/           VaultTransitProvider: Vault Transit engine.
neuron-crypto-hardware/        HardwareProvider: PKCS#11 / YubiKey PIV.

neuron-hook-security/          RedactionHook, ExfilGuardHook.
```

## Layer 0 Types (secret.rs)

### SecretSource — where a secret lives (backend only)

```rust
/// Where a secret is stored. This describes the BACKEND, not the delivery mechanism.
/// Delivery is handled by `CredentialInjection` (env var, file, sidecar).
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SecretSource {
    /// HashiCorp Vault.
    Vault { mount: String, path: String },
    /// AWS Secrets Manager.
    AwsSecretsManager { secret_id: String, region: Option<String> },
    /// GCP Secret Manager.
    GcpSecretManager { project: String, secret_id: String },
    /// Azure Key Vault.
    AzureKeyVault { vault_url: String, secret_name: String },
    /// OS keystore (macOS Keychain, Windows DPAPI, Linux Secret Service).
    OsKeystore { service: String },
    /// Kubernetes Secret.
    Kubernetes { namespace: String, name: String, key: String },
    /// Hardware token (YubiKey, HSM via PKCS#11).
    Hardware { slot: String },
    /// Custom source for future backends.
    Custom { provider: String, config: serde_json::Value },
}
```

Note: `EnvironmentVariable` and `File` are NOT `SecretSource` variants. Reading from an env
var or file is an injection/delivery concern (`CredentialInjection::EnvVar` / `::File`), not a
backend concern. A secret can live in Vault (source) and be delivered as an env var (injection).

### SecretAccessEvent — lifecycle audit event

```rust
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
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretAccessEvent {
    /// The credential name (label, not the secret value).
    pub credential_name: String,
    /// Where resolution was attempted.
    pub source: SecretSource,
    /// What happened.
    pub outcome: SecretAccessOutcome,
    /// When it happened (Unix timestamp millis).
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
```

### Error types

Error types (`SecretError`, `AuthError`, `CryptoError`) live in their respective trait
crates (`neuron-secret`, `neuron-auth`, `neuron-crypto`), NOT in layer0. This keeps the
stability contract minimal — layer0 only has data types that cross protocol boundaries.

### CredentialRef update

```rust
/// A reference to a credential that should be injected into the environment.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRef {
    /// Name of the credential (e.g., "anthropic-api-key").
    pub name: String,
    /// Where the secret lives (backend).
    pub source: SecretSource,
    /// How to inject it into the execution environment (delivery).
    pub injection: CredentialInjection,
}
```

Note: `source` is required (not `Option`). Pre-1.0, we make breaking changes freely.
The `new()` constructor updates to take three arguments.

## Trait Crate: neuron-secret

### SecretValue — in-memory wrapper

```rust
use zeroize::Zeroizing;

/// An opaque secret value. Cannot be logged, serialized, or cloned.
/// Memory is zeroed on drop via `Zeroizing<Vec<u8>>`.
pub struct SecretValue {
    inner: Zeroizing<Vec<u8>>,
}

impl SecretValue {
    /// Create a new secret value. The input vector is moved (not copied).
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { inner: Zeroizing::new(bytes) }
    }

    /// Scoped exposure. The secret bytes are only accessible inside the closure.
    /// This is the ONLY way to read the value.
    pub fn with_bytes<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        f(&self.inner)
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

// No Display, no Clone, no Serialize. Drop zeros memory via Zeroizing.
```

### SecretLease — resolved secret with TTL

```rust
/// A resolved secret with optional lease information.
pub struct SecretLease {
    /// The secret value.
    pub value: SecretValue,
    /// When this lease expires (None = no expiry).
    pub expires_at: Option<std::time::SystemTime>,
    /// Whether this lease can be renewed.
    pub renewable: bool,
    /// Opaque lease ID for renewal/revocation.
    pub lease_id: Option<String>,
}

impl SecretLease {
    /// Check if this lease has expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| std::time::SystemTime::now() > exp)
            .unwrap_or(false)
    }
}
```

### SecretResolver trait

```rust
use layer0::secret::{SecretSource, SecretError};

/// Resolve a secret from a specific backend.
///
/// Implementations are backend-specific: VaultResolver talks to Vault,
/// AwsResolver talks to AWS Secrets Manager, KeystoreResolver talks to
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
```

### SecretRegistry — composition

```rust
/// Composes multiple resolvers. Routes by SecretSource variant.
///
/// When `resolve()` is called, the registry:
/// 1. Matches the SecretSource variant to a registered resolver
/// 2. Delegates to that resolver
/// 3. Emits a SecretAccessEvent (success or failure)
pub struct SecretRegistry {
    resolvers: Vec<(SourceMatcher, Arc<dyn SecretResolver>)>,
    // Event emission hook (optional, for SecretAccessEvent)
}

/// How to match a SecretSource to a resolver.
pub enum SourceMatcher {
    /// Match all Vault sources.
    Vault,
    /// Match all AWS sources.
    Aws,
    /// Match all GCP sources.
    Gcp,
    /// Match all Azure sources.
    Azure,
    /// Match all OS keystore sources.
    OsKeystore,
    /// Match all Kubernetes sources.
    Kubernetes,
    /// Match all hardware sources.
    Hardware,
    /// Match a specific custom provider name.
    Custom(String),
}
```

## Trait Crate: neuron-auth

### AuthRequest — context for authentication

```rust
/// Context for an authentication request.
///
/// Different backends need different context:
/// - OIDC needs audience and scope
/// - AWS needs region and role
/// - K8s needs service account and namespace
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
```

### AuthToken — opaque time-bounded credential

```rust
/// An opaque authentication token with expiry.
/// Uses the same SecretValue wrapper for in-memory protection.
pub struct AuthToken {
    inner: SecretValue,
    expires_at: Option<std::time::SystemTime>,
}

impl AuthToken {
    pub fn new(bytes: Vec<u8>, expires_at: Option<std::time::SystemTime>) -> Self {
        Self {
            inner: SecretValue::new(bytes),
            expires_at,
        }
    }

    /// Scoped exposure of the token bytes.
    pub fn with_bytes<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        self.inner.with_bytes(f)
    }

    /// Check if this token has expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| std::time::SystemTime::now() > exp)
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthToken([REDACTED])")
    }
}
```

### AuthProvider trait

```rust
/// Provide authentication credentials for accessing a secret backend.
///
/// Implementations: OidcAuthProvider (token exchange), K8sAuthProvider
/// (service account token), StaticAuthProvider (hardcoded, dev only),
/// FileTokenProvider (reads from mounted file).
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Provide an authentication token for the given request context.
    async fn provide(&self, request: &AuthRequest) -> Result<AuthToken, AuthError>;
}
```

### AuthProviderChain — composition

```rust
/// Tries providers in order until one succeeds.
/// Modeled after AWS DefaultCredentialsChain.
pub struct AuthProviderChain {
    providers: Vec<Arc<dyn AuthProvider>>,
}

impl AuthProviderChain {
    pub fn new() -> Self { Self { providers: Vec::new() } }
    pub fn with_provider(mut self, provider: Arc<dyn AuthProvider>) -> Self {
        self.providers.push(provider);
        self
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
        Err(last_err.unwrap_or_else(|| AuthError::AuthFailed("no providers configured".into())))
    }
}
```

## Trait Crate: neuron-crypto

### CryptoProvider trait

```rust
/// Cryptographic operations where private keys never leave the provider boundary.
///
/// Implementations: VaultTransitProvider (Vault Transit engine),
/// HardwareProvider (PKCS#11, YubiKey PIV), KmsProvider (AWS/GCP/Azure KMS).
///
/// The key_ref is an opaque identifier meaningful to the implementation:
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
```

## Hooks: neuron-hook-security

### HookAction::ModifyToolOutput (new variant, required)

The existing `HookAction` enum has `ModifyToolInput` but no output mutation variant.
RedactionHook needs to modify tool OUTPUT (not input). Pre-1.0 is the time to add this.

Add to `layer0/src/hook.rs`:
```rust
/// Replace the tool output with a modified version (e.g., redacted).
ModifyToolOutput { new_output: serde_json::Value },
```

### RedactionHook

Fires at `PostToolUse`. Scans tool output for patterns matching known secret formats.
Returns `HookAction::ModifyToolOutput` with matches replaced by `[REDACTED]`.

**Intentionally narrow patterns for v0** (avoid regex false positives):
- AWS access keys: `AKIA[A-Z0-9]{16}`
- Vault tokens: `hvs\.[a-zA-Z0-9_-]+`
- GitHub tokens: `gh[ps]_[a-zA-Z0-9]{36}`
- User-supplied custom patterns

**Not included in v0** (future: fingerprint-based redaction using HMAC of resolved secrets
for exact-match detection without regex arms races).

### ExfilGuardHook

Fires at `PreToolUse`. Checks if tool inputs look like exfiltration attempts
(e.g., base64 blobs sent to external URLs, shell commands piping env vars to curl).
Returns `HookAction::Halt` on match.

Both hooks use the existing `Hook` trait from layer0.

## Composition Example

```rust
// === Production setup ===

// 1. Auth: OIDC for Vault, k8s SA for AWS (via IRSA)
let oidc_auth = OidcAuthProvider::new(issuer_url, client_id, client_secret);
let k8s_auth = K8sAuthProvider::new("/var/run/secrets/kubernetes.io/serviceaccount/token");
let auth_chain = AuthProviderChain::new()
    .with_provider(Arc::new(oidc_auth))
    .with_provider(Arc::new(k8s_auth));

// 2. Secret resolvers (each takes its own auth)
let vault = VaultResolver::new("https://vault.internal:8200", Arc::new(oidc_auth));
let aws = AwsResolver::new(Arc::new(k8s_auth));
let keystore = KeystoreResolver::new();

// 3. Compose into registry
let secrets = SecretRegistry::new()
    .with_resolver(SourceMatcher::Vault, Arc::new(vault))
    .with_resolver(SourceMatcher::Aws, Arc::new(aws))
    .with_resolver(SourceMatcher::OsKeystore, Arc::new(keystore));

// 4. Crypto provider (Vault Transit for payload encryption)
let transit = VaultTransitProvider::new("https://vault.internal:8200", Arc::new(oidc_auth));

// 5. Environment uses registry + crypto
let env = SecureEnvironment::new(operator, secrets, transit);

// 6. Hooks for runtime safety
let hooks = HookRegistry::new();
hooks.add(Arc::new(RedactionHook::new()));
hooks.add(Arc::new(ExfilGuardHook::new()));

// === Dev setup (simple) ===

let secrets = SecretRegistry::new()
    .with_resolver(SourceMatcher::OsKeystore, Arc::new(KeystoreResolver::new()));
let env = LocalEnv::new(operator); // ignores secrets (dev mode)
```

## Mapping to 23 Decision Points

| Decision | How secrets design maps |
|----------|----------------------|
| **D4A (Isolation)** | Environment owns SecretRegistry. Different isolation = different resolution. |
| **D4B (Credentials)** | Primary. Full spectrum: keystore (local) -> Vault+OIDC (production) -> sidecar. |
| **D4C (Backfill)** | RedactionHook sanitizes tool output before re-entering context. |
| **D2D (Tools)** | Operators get capability tools, not secrets. Tool has secret injected at boundary. |
| **C1 (Child context)** | CredentialRef (with source) passes to children. Value resolved by child's Environment. |
| **C4 (Communication)** | SecretValue has no Serialize. Cannot travel through signals/events/state. |
| **C5 (Observation)** | RedactionHook + ExfilGuardHook = guardrail pattern from decision map. |
| **L1 (Memory writes)** | SecretAccessEvent persisted. SecretValue never persisted. |
| **L3 (Crash recovery)** | SecretLease expires. On recovery, re-resolve from backend. |
| **L5 (Observability)** | SecretAccessEvent with outcome, identity, correlation, lease tracking. |

## Dependencies

### layer0/secret.rs
- `serde`, `serde_json` (already in layer0)
- `thiserror` (already in layer0)

### neuron-secret
- `layer0` (for types)
- `async-trait`
- `zeroize` (for SecretValue)

### neuron-auth
- `neuron-secret` (for SecretValue, reused in AuthToken)
- `async-trait`

### neuron-crypto
- `thiserror` (for CryptoError, defined locally)
- `async-trait`

### All stub backend crates
- Their trait crate (`neuron-secret`, `neuron-auth`, or `neuron-crypto`)
- `async-trait`
- No real SDK dependencies (stubs only)

### neuron-hook-security
- `layer0` (for Hook trait)
- `async-trait`
- `regex` (for redaction patterns)

## Testing Strategy

Every crate gets:
1. **Object safety**: `Box<dyn SecretResolver>`, `Arc<dyn AuthProvider>`, etc. are Send + Sync.
2. **Serde roundtrips**: All layer0 data types round-trip through serde_json.
3. **Error Display**: Every error variant produces correct output.
4. **Composition**: Registry dispatches to correct resolver by source variant.
5. **SecretValue safety**: Confirm no Debug leak, no Clone, zeroize on drop.
6. **Hook integration**: RedactionHook catches known patterns, ExfilGuardHook halts on exfil attempts.
