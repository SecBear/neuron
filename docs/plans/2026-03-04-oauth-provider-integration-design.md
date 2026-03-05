# Design: OAuth Provider Integration for neuron-auth

**Date:** 2026-03-04
**Status:** Draft
**Crate:** `neuron/auth/neuron-auth/` (trait), `neuron/auth/neuron-auth-oauth/` (new impl)
**Depends on:** `neuron-auth`, `neuron-secret`, `layer0`

---

## 1. Research Findings

### 1.1 Anthropic API Authentication

**Current state: API key only.** Anthropic's public API (`api.anthropic.com/v1/messages`) authenticates exclusively via the `x-api-key` header with a static API key. There is no publicly documented OAuth 2.0 authorization server or token endpoint for the Anthropic API as of March 2026.

**Claude Code's OAuth 2.1 (MCP context):** Claude Code uses OAuth 2.1 with PKCE for authenticating to MCP (Model Context Protocol) tool servers — not for API access itself. The MCP November 2025 spec defines an OAuth 2.1 authorization flow where:
- The MCP server exposes `/.well-known/oauth-authorization-server` metadata
- Claude Code acts as a public client (PKCE, no client secret)
- Token exchange follows RFC 8414 / RFC 7636
- Scoped, short-lived tokens are issued per-tool

This is relevant as a pattern but does not grant API access to Anthropic's model endpoints. Claude Code itself uses a proxy-brokered credential model — the agent never sees the Anthropic API key directly. The key is held server-side by Anthropic's infrastructure.

**Source:** MCP spec (Nov 2025).

### 1.2 OpenAI Authentication

OpenAI's API uses bearer tokens (`Authorization: Bearer sk-...`). There is no OAuth 2.0 device flow for obtaining API keys programmatically. OpenAI's ChatGPT and Codex tools authenticate through their own session infrastructure, not via a reusable OAuth flow.

OpenAI does support OAuth for ChatGPT Plugins (now GPT Actions), where third-party services authenticate ChatGPT users via standard OAuth 2.0 authorization code flow. This is the reverse direction — ChatGPT as OAuth client to external services, not external clients authenticating to OpenAI.

### 1.3 OMP / Pi Agent Authentication

OMP (Oh My Pi) stores provider credentials in a SQLite database (`~/.omp/agent/agent.db`):

```sql
CREATE TABLE auth_credentials (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider TEXT NOT NULL,
  credential_type TEXT NOT NULL,  -- 'oauth' | 'apiKey'
  data TEXT NOT NULL,             -- JSON: {"apiKey": "..."} or {"accessToken": "...", "refreshToken": "..."}
  disabled INTEGER DEFAULT 0,
  created_at INTEGER DEFAULT (unixepoch()),
  updated_at INTEGER DEFAULT (unixepoch())
);
```

Key observations:
- Credentials are stored with `credential_type` distinguishing `oauth` from `apiKey`
- The `parallel-auth` extension bridges stored keys to environment variables at runtime
- No full OAuth flows (authorization code, device code, PKCE) are implemented — "oauth" type stores pre-obtained tokens
- CLI commands (`/parallel-login`, `/parallel-logout`) handle credential lifecycle

This validates our credential chain approach: store → resolve → inject → expose-via-env-or-header.

### 1.4 Subscription-Based Access Feasibility

**Not viable today.** Anthropic Max/Pro subscription tokens are bound to browser sessions and the `claude.ai` web application. They cannot be extracted for programmatic API use:
- Session cookies are HTTP-only, tied to browser fingerprint
- No documented path to exchange a subscription for API credentials
- Anthropic's Terms of Service likely prohibit automated use of subscription access
- Rate limits and fair use policies assume interactive human usage

**Extraction of Claude Code tokens:** Claude Code's credentials are managed by Anthropic's backend infrastructure. The agent process receives scoped, short-lived tokens via IPC — not environment variables. These tokens are:
- Bound to the Claude Code session
- Not transferable to third-party applications
- Revoked on session end

**Recommendation:** Do not pursue subscription token reuse. Use official API keys or wait for Anthropic to offer OAuth-based API access.

### 1.5 Industry Direction

The MCP OAuth 2.1 spec signals that Anthropic is investing in OAuth infrastructure. It is reasonable to expect that OAuth-based API access may become available. Our architecture should be ready for it without requiring it today.

---

## 2. Current State

neuron's auth infrastructure is well-factored but not yet wired into the provider layer:

```
neuron-auth (trait layer)          neuron-op-sweep (operator layer)
┌─────────────────────┐           ┌────────────────────────┐
│ AuthProvider trait   │           │ SweepProvider          │
│ AuthToken            │──(not)──→│   resolve_env_key()    │
│ AuthProviderChain    │  wired   │   std::env::var(...)   │
│ AuthRequest          │           └────────────────────────┘
└─────────────────────┘
```

### What exists

| Component | Location | Purpose |
|-----------|----------|---------|
| `AuthProvider` trait | `neuron-auth/src/lib.rs:139` | `async fn provide(&self, &AuthRequest) -> Result<AuthToken, AuthError>` |
| `AuthToken` | `neuron-auth/src/lib.rs:91` | Wraps `SecretValue`, has `expires_at()`, `is_expired()`, `with_bytes()` |
| `AuthProviderChain` | `neuron-auth/src/lib.rs:145` | Tries providers in order (AWS DefaultCredentialsChain pattern) |
| `AuthRequest` | `neuron-auth/src/lib.rs:47` | Builder with `audience`, `scopes`, `resource`, `actor` |
| `SecretValue` | `neuron-secret/src/lib.rs:58` | Zeroizing in-memory wrapper, no Clone/Display/Serialize |
| `SecretSource` | `layer0/src/secret.rs:18` | Enum: Vault, AWS, GCP, Azure, **OsKeystore**, K8s, Hardware, Custom |
| `CredentialRef` | `layer0/src/environment.rs:109` | Declarative credential binding (name, source, injection method) |
| `SweepProvider` | `neuron-op-sweep/src/sweep_provider.rs:124` | Uses `resolve_env_key()` → `std::env::var()` |
| `AnthropicProvider` | `neuron-provider-anthropic/src/lib.rs:22` | `ApiKeySource::Static` or `ApiKeySource::EnvVar` |
| `OpenAIProvider` | `neuron-provider-openai/src/lib.rs:14` | Same `ApiKeySource` pattern |

### What's missing

1. No `AuthProvider` implementation that does OAuth (device flow, refresh)
2. `SweepProvider` and `AnthropicProvider`/`OpenAIProvider` bypass `AuthProvider` entirely
3. No token caching or persistence layer
4. No CLI for interactive credential acquisition

---

## 3. OAuth Integration Architecture

### 3.1 New Types

```rust
// neuron/auth/neuron-auth-oauth/src/lib.rs

/// OAuth 2.0 Device Authorization Flow provider.
///
/// Implements RFC 8628 (Device Authorization Grant):
/// 1. POST to device authorization endpoint → device_code + user_code + verification_uri
/// 2. Display user_code and verification_uri to user
/// 3. Poll token endpoint with device_code until user authorizes
/// 4. Return access_token as AuthToken with expiry
#[derive(Debug, Clone)]
pub struct OAuthDeviceFlowProvider {
    /// OAuth client ID (public client, no secret needed for device flow).
    client_id: String,
    /// Device authorization endpoint.
    device_auth_url: String,
    /// Token endpoint.
    token_url: String,
    /// Scopes to request.
    scopes: Vec<String>,
    /// Audience (API identifier).
    audience: Option<String>,
    /// HTTP client for token requests.
    client: reqwest::Client,
    /// Cached token (interior mutability for trait compliance).
    cached_token: Arc<tokio::sync::RwLock<Option<CachedToken>>>,
}

struct CachedToken {
    access_token: SecretValue,
    refresh_token: Option<SecretValue>,
    expires_at: Option<SystemTime>,
}
```

### 3.2 Device Authorization Flow

```
User                    neuron CLI             Auth Server
 │                         │                       │
 │  neuron auth login      │                       │
 │────────────────────────>│                       │
 │                         │  POST /device/code    │
 │                         │──────────────────────>│
 │                         │  device_code,         │
 │                         │  user_code,           │
 │                         │  verification_uri     │
 │                         │<──────────────────────│
 │  "Visit {uri},          │                       │
 │   enter code: {code}"   │                       │
 │<────────────────────────│                       │
 │                         │                       │
 │  User authorizes        │                       │
 │  in browser             │                       │
 │                         │  POST /token          │
 │                         │  (poll with           │
 │                         │   device_code)        │
 │                         │──────────────────────>│
 │                         │  access_token,        │
 │                         │  refresh_token,       │
 │                         │  expires_in           │
 │                         │<──────────────────────│
 │                         │                       │
 │                         │  Store to OsKeystore  │
 │                         │  via SecretSource     │
 │  "Authenticated!"       │                       │
 │<────────────────────────│                       │
```

### 3.3 Token Refresh and Caching

```rust
#[async_trait]
impl AuthProvider for OAuthDeviceFlowProvider {
    async fn provide(&self, request: &AuthRequest) -> Result<AuthToken, AuthError> {
        // 1. Check in-memory cache
        let cached = self.cached_token.read().await;
        if let Some(ref token) = *cached {
            if !is_near_expiry(token.expires_at) {
                return Ok(token.to_auth_token());
            }
        }
        drop(cached);

        // 2. Try refresh (if we have a refresh token)
        let mut cached = self.cached_token.write().await;
        if let Some(ref token) = *cached {
            if let Some(ref refresh) = token.refresh_token {
                match self.refresh(refresh).await {
                    Ok(new_token) => {
                        *cached = Some(new_token.clone());
                        self.persist(&new_token).await?;
                        return Ok(new_token.to_auth_token());
                    }
                    Err(_) => {} // Fall through to re-auth
                }
            }
        }

        // 3. Try loading from persistent storage (OsKeystore)
        if let Ok(persisted) = self.load_persisted().await {
            if !is_near_expiry(persisted.expires_at) {
                *cached = Some(persisted.clone());
                return Ok(persisted.to_auth_token());
            }
        }

        // 4. No valid token — return error (CLI must run `neuron auth login`)
        Err(AuthError::AuthFailed(
            "no valid OAuth token; run `neuron auth login`".into(),
        ))
    }
}

/// Consider a token "near expiry" if it expires within 5 minutes.
fn is_near_expiry(expires_at: Option<SystemTime>) -> bool {
    expires_at
        .map(|exp| SystemTime::now() + Duration::from_secs(300) > exp)
        .unwrap_or(false)
}
```

### 3.4 Token Persistence

Tokens are persisted to OS keystore via `SecretSource::OsKeystore`:

```rust
// Store
SecretSource::OsKeystore { service: "neuron-oauth-anthropic".into() }

// The access token and refresh token are stored as a JSON blob:
// {"access_token": "...", "refresh_token": "...", "expires_at": 1741234567}
// Encrypted at rest by the OS keystore (macOS Keychain, DPAPI, Secret Service).
```

Fallback for headless/CI: `SecretSource::Custom { provider: "file", config: {"path": "~/.neuron/tokens.enc"} }` with file-level encryption.

---

## 4. Provider Injection — Option Analysis

### Option A: Add `auth` to SweepProvider (Recommended)

```rust
pub struct SweepProvider {
    // ... existing fields ...
    /// Optional auth provider chain for credential resolution.
    auth: Option<Arc<dyn AuthProvider>>,
}

impl SweepProvider {
    fn resolve_anthropic_key(&self) -> Result<String, SweepError> {
        // Try auth provider first
        if let Some(ref auth) = self.auth {
            let request = AuthRequest::new()
                .with_audience("https://api.anthropic.com")
                .with_scope("messages:write");
            if let Ok(token) = block_on_or_spawn(auth.provide(&request)) {
                return Ok(token.with_bytes(|b| String::from_utf8_lossy(b).into_owned()));
            }
        }
        // Fallback to env var
        resolve_env_key(&self.anthropic_key_var)
    }
}
```

| Pros | Cons |
|------|------|
| Backward compatible — `auth: None` preserves current behavior | SweepProvider accumulates concerns |
| Single integration point | Auth is resolved synchronously in some call paths |
| Follows existing builder pattern | Couples sweep to auth crate |

### Option B: Add auth to Provider trait

```rust
pub trait Provider {
    fn complete(&self, request: ProviderRequest) -> impl Future<...> + Send;
    fn auth_provider(&self) -> Option<&dyn AuthProvider> { None }
}
```

| Pros | Cons |
|------|------|
| Every provider can resolve credentials uniformly | Provider trait is intentionally not object-safe (RPITIT); adding auth changes contract |
| Clean separation | Forces all providers to know about auth even if they don't need it |

### Option C: Resolve via Environment trait's CredentialRef

```rust
CredentialRef {
    name: "anthropic-api-key",
    source: SecretSource::OsKeystore { service: "neuron-oauth-anthropic" },
    injection: CredentialInjection::EnvVar { var_name: "ANTHROPIC_API_KEY" },
}
```

| Pros | Cons |
|------|------|
| Zero changes to SweepProvider or Provider | Requires Environment trait wiring (not yet implemented) |
| Declarative, auditable | Indirection makes debugging harder |
| Aligns with `EnvironmentSpec` credential injection | Only works for operators that use Environment |

### Recommendation: Option A for Phase 1, Option C long-term

Option A is the least invasive path to wire OAuth into the existing system. It keeps the fallback chain explicit and testable. Once the Environment trait's credential injection is fully implemented, Option C becomes the canonical path — credentials are declared in `EnvironmentSpec` and injected before the operator runs. At that point, the `auth` field on SweepProvider becomes unnecessary.

---

## 5. Edge Cases

### 5.1 Token Expires Mid-Sweep

A sweep operation may make multiple API calls (search → compare → plan). If the OAuth token expires between calls:

**Mitigation:** The `is_near_expiry()` check uses a 5-minute buffer. Since individual API calls complete in seconds, this buffer is sufficient. If a token does expire mid-call:
1. The API returns HTTP 401
2. `SweepError::Permanent` is returned (not retryable with same token)
3. The `AuthProviderChain` is consulted again on the next attempt — refresh happens automatically
4. If refresh fails, fallback to env var

**Design rule:** Never cache the resolved key string. Always resolve via `AuthProvider` at call time. This is already the pattern in `resolve_env_key()` (reads env var on each call).

### 5.2 Fallback Chain

```
OAuthDeviceFlowProvider → EnvVarAuthProvider → Error
         │                        │
    "neuron-oauth-anthropic"  std::env::var("ANTHROPIC_API_KEY")
    from OsKeystore              at call time
```

When SweepProvider has `auth: Some(chain)`:
1. Try chain first (OAuth token from keystore → refresh if needed)
2. On `AuthError` → fall back to env var (existing behavior)
3. On env var missing → `SweepError::Permanent` with variable name only

### 5.3 Security Invariants

- **Tokens in memory only:** All tokens wrapped in `SecretValue` (zeroized on drop)
- **No tokens in error messages:** `AuthError` variants contain context strings, never token material
- **No tokens in logs:** `AuthToken` Debug impl prints `[REDACTED]`
- **Persistent storage:** OS keystore (encrypted at rest) or encrypted file — never plaintext
- **Scoped exposure:** `with_bytes()` closure pattern ensures tokens aren't accidentally held in variables

---

## 6. Implementation Plan

### Phase 1: Wire AuthProvider into SweepProvider (immediate)

**Effort:** 1-2 days
**Changes:**
- Add `auth: Option<Arc<dyn AuthProvider>>` to `SweepProvider`
- Add `with_auth(auth: Arc<dyn AuthProvider>) -> Self` builder method
- Modify `resolve_anthropic_key()` and `resolve_parallel_key()` to try auth first
- Implement `EnvVarAuthProvider` (wraps current `std::env::var` logic as an `AuthProvider`)
- Update `LocalOrch` wiring to pass auth chain through

**Tests:**
- `SweepProvider` with `auth: None` behaves identically to today
- `SweepProvider` with mock `AuthProvider` uses provided token
- Fallback from failed auth to env var works
- No real HTTP calls (mock everything)

### Phase 2: OAuthDeviceFlowProvider (when Anthropic offers OAuth, or for third-party providers)

**Effort:** 3-5 days
**Changes:**
- New crate `neuron-auth-oauth` with `OAuthDeviceFlowProvider`
- RFC 8628 device flow implementation (POST device auth → poll token endpoint)
- Token refresh via RFC 6749 refresh_token grant
- In-memory caching with `Arc<RwLock<Option<CachedToken>>>`

**Blocked on:** An actual OAuth authorization server to test against. Can develop against a mock server or Auth0/Keycloak for validation.

### Phase 3: Token Persistence (after Phase 2)

**Effort:** 2-3 days
**Changes:**
- Implement `OsKeystoreResolver` in `neuron-secret` (macOS Keychain via `security-framework`, Linux via `secret-service`)
- `OAuthDeviceFlowProvider::persist()` and `load_persisted()` methods
- Encrypted file fallback for headless environments

### Phase 4: CLI — `neuron auth login` (after Phase 3)

**Effort:** 2-3 days
**Changes:**
- New CLI subcommand: `neuron auth login [--provider anthropic|openai|custom]`
- Triggers device flow, displays user code, polls for completion
- `neuron auth status` — shows which providers have valid tokens
- `neuron auth logout` — clears stored tokens

---

## 7. Immediate Next Step

**Phase 1 is actionable now.** It requires no external OAuth server and produces immediate value:

1. Add `neuron-auth` dependency to `neuron-op-sweep`
2. Add `auth: Option<Arc<dyn AuthProvider>>` field to `SweepProvider`
3. Implement `EnvVarAuthProvider` — trivial `AuthProvider` impl that wraps `std::env::var()`
4. Wire the fallback: auth chain → env var → error
5. This unblocks all future auth work without changing any external behavior

The key insight: by wrapping the existing env-var logic in an `AuthProvider`, we make it composable with future OAuth providers via `AuthProviderChain`. The transition from "env vars only" to "OAuth with env var fallback" becomes adding a provider to the chain — zero changes to SweepProvider.

---

## Appendix A: MCP OAuth 2.1 Reference Flow

For reference, the MCP spec's OAuth 2.1 flow that Claude Code uses for tool authentication:

```
Client                              MCP Server
  │                                     │
  │  GET /.well-known/                  │
  │    oauth-authorization-server       │
  │────────────────────────────────────>│
  │  { authorization_endpoint,          │
  │    token_endpoint, ... }            │
  │<────────────────────────────────────│
  │                                     │
  │  GET /authorize?                    │
  │    response_type=code&              │
  │    client_id=...&                   │
  │    code_challenge=...&              │
  │    code_challenge_method=S256       │
  │────────────────────────────────────>│
  │  redirect with ?code=...            │
  │<────────────────────────────────────│
  │                                     │
  │  POST /token                        │
  │    grant_type=authorization_code&   │
  │    code=...&                        │
  │    code_verifier=...                │
  │────────────────────────────────────>│
  │  { access_token, refresh_token,     │
  │    expires_in, token_type }         │
  │<────────────────────────────────────│
```

This flow is relevant when neuron acts as an MCP client connecting to tool servers that require OAuth. It is not directly applicable to Anthropic API access today.

## Appendix B: OMP Credential Storage Pattern

OMP's approach provides a useful reference implementation:

```typescript
// Store credential (upsert)
INSERT OR REPLACE INTO auth_credentials (provider, credential_type, data, updated_at)
VALUES ('anthropic', 'apiKey', '{"apiKey":"sk-..."}', unixepoch());

// Bridge to environment
process.env.ANTHROPIC_API_KEY = JSON.parse(data).apiKey;
```

neuron's equivalent would use `AuthProviderChain` instead of direct env var injection, providing:
- Type safety (Rust vs runtime JSON parsing)
- Memory protection (`SecretValue` with zeroize vs `process.env` string)
- Composability (chain multiple providers vs single lookup)
