//! The Environment protocol — isolation, credentials, and resource constraints.
//!
//! This module defines the declarative [`EnvironmentSpec`] (what isolation and
//! credentials an operator needs) and the [`EnvironmentProvider`] trait (who
//! provisions that isolation). Together they implement the "environment as
//! provisioning, not execution" design principle.
//!
//! # Architecture
//!
//! Environments don't execute operators — they wrap them. An [`EnvironmentProvider`]
//! takes an `Arc<dyn Operator>` and returns a [`ProvisionedEnv`] that delegates
//! to the inner operator within the provisioned isolation boundary. The operator
//! inside doesn't know where it's running.
//!
//! # Security
//!
//! Credential injection follows the three-tier security model:
//! - **Tier 1**: Provider keys via [`CredentialInjection::Sidecar`] — the operator
//!   never sees the credential.
//! - **Tier 2**: Tool credentials via [`CredentialInjection::EnvVar`] or
//!   [`CredentialInjection::File`] — resolved from the secret store and injected
//!   into the environment, never into the operator's input schema.

use crate::capability::CapabilityDescriptor;
use crate::dispatch::DispatchHandle;
use crate::dispatch_context::DispatchContext;
use crate::error::{EnvError, ProtocolError};
use crate::operator::{Operator, OperatorInput};
use crate::secret::SecretSource;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Declarative specification for an execution environment.
/// This is serializable so it can live in config files (YAML, TOML).
#[non_exhaustive]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentSpec {
    /// Isolation boundaries to apply, outermost first.
    #[serde(default)]
    pub isolation: Vec<IsolationBoundary>,

    /// Credentials to make available inside the environment.
    #[serde(default)]
    pub credentials: Vec<CredentialRef>,

    /// Resource limits.
    pub resources: Option<ResourceLimits>,

    /// Network policy.
    pub network: Option<NetworkPolicy>,
}

impl EnvironmentSpec {
    /// Create a new empty environment spec (no isolation, no credentials).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the isolation boundaries.
    pub fn with_isolation(mut self, isolation: Vec<IsolationBoundary>) -> Self {
        self.isolation = isolation;
        self
    }

    /// Set the credential references.
    pub fn with_credentials(mut self, credentials: Vec<CredentialRef>) -> Self {
        self.credentials = credentials;
        self
    }

    /// Set the resource limits.
    pub fn with_resources(mut self, resources: ResourceLimits) -> Self {
        self.resources = Some(resources);
        self
    }

    /// Set the network policy.
    pub fn with_network(mut self, network: NetworkPolicy) -> Self {
        self.network = Some(network);
        self
    }
}

/// A single isolation boundary. Multiple boundaries compose
/// (e.g., container + gVisor + network policy = defense in depth).
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IsolationBoundary {
    /// OS process boundary.
    Process,
    /// Container (Docker, containerd, etc.).
    Container {
        /// Optional container image to use.
        image: Option<String>,
    },
    /// Syscall interception (gVisor runsc).
    Gvisor,
    /// Hardware-enforced VM (Kata Containers).
    MicroVm,
    /// WebAssembly sandbox.
    Wasm {
        /// Optional Wasm runtime to use.
        runtime: Option<String>,
    },
    /// Network-level isolation.
    NetworkPolicy {
        /// Network rules to apply.
        rules: Vec<NetworkRule>,
    },
    /// Future isolation types.
    Custom {
        /// The custom boundary type identifier.
        boundary_type: String,
        /// Configuration for this boundary.
        config: serde_json::Value,
    },
}

/// A reference to a credential that should be injected into the environment.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRef {
    /// Name of the credential (e.g., "anthropic-api-key").
    pub name: String,
    /// Where the secret is stored (backend).
    pub source: SecretSource,
    /// How to inject it.
    pub injection: CredentialInjection,
}

/// How a credential is injected into the environment.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialInjection {
    /// Set as environment variable.
    EnvVar {
        /// The environment variable name.
        var_name: String,
    },
    /// Mount as file.
    File {
        /// The file path to mount the credential at.
        path: String,
    },
    /// Inject via sidecar/proxy (agent never sees the secret).
    Sidecar,
}

/// Resource limits for the execution environment.
#[non_exhaustive]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU limit, e.g. "1.0", "500m".
    pub cpu: Option<String>,
    /// Memory limit, e.g. "2Gi", "512Mi".
    pub memory: Option<String>,
    /// Disk limit, e.g. "10Gi".
    pub disk: Option<String>,
    /// GPU allocation, e.g. "1" or "nvidia.com/gpu: 1".
    pub gpu: Option<String>,
}

/// Network policy for the execution environment.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Default action for traffic not matching any rule.
    pub default: NetworkAction,
    /// Explicit rules.
    pub rules: Vec<NetworkRule>,
}

/// A single network rule.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRule {
    /// Domain or CIDR to match.
    pub destination: String,
    /// Port (optional, None = all ports).
    pub port: Option<u16>,
    /// Allow or deny.
    pub action: NetworkAction,
}

/// Network traffic action.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkAction {
    /// Allow the traffic.
    Allow,
    /// Deny the traffic.
    Deny,
}

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

impl NetworkPolicy {
    /// Create a new network policy.
    pub fn new(default: NetworkAction, rules: Vec<NetworkRule>) -> Self {
        Self { default, rules }
    }
}

impl NetworkRule {
    /// Create a new network rule.
    pub fn new(destination: impl Into<String>, action: NetworkAction) -> Self {
        Self {
            destination: destination.into(),
            port: None,
            action,
        }
    }
}

// ── EnvironmentProvider ────────────────────────────────────────────────────────

/// A provisioned execution environment wrapping an operator with isolation.
///
/// Created by [`EnvironmentProvider::provision`]. Implements [`Operator`] by
/// delegating to the wrapped inner operator. The caller must call
/// [`EnvironmentProvider::teardown`] with [`env_id`](ProvisionedEnv::env_id)
/// when the environment is no longer needed.
pub struct ProvisionedEnv {
    /// Opaque identifier for this provisioned environment.
    ///
    /// Pass this to [`EnvironmentProvider::teardown`] to release resources.
    /// For local environments this is a synthetic ID. For containers it is
    /// the container ID. For VMs it is the instance ID.
    pub env_id: String,
    /// The wrapped operator running inside this environment.
    operator: Arc<dyn Operator>,
}

impl ProvisionedEnv {
    /// Create a new provisioned environment.
    pub fn new(env_id: impl Into<String>, operator: Arc<dyn Operator>) -> Self {
        Self {
            env_id: env_id.into(),
            operator,
        }
    }

    /// Get a reference to the wrapped operator.
    pub fn operator(&self) -> &Arc<dyn Operator> {
        &self.operator
    }

    /// Consume this provisioned environment and return the wrapped operator.
    ///
    /// The caller takes ownership of teardown responsibility — the environment
    /// resources are NOT released by this call.
    pub fn into_operator(self) -> Arc<dyn Operator> {
        self.operator
    }
}

impl std::fmt::Debug for ProvisionedEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionedEnv")
            .field("env_id", &self.env_id)
            .field("operator", &self.operator.descriptor().id)
            .finish()
    }
}

#[async_trait]
impl Operator for ProvisionedEnv {
    fn descriptor(&self) -> CapabilityDescriptor {
        self.operator.descriptor()
    }

    async fn handle(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<DispatchHandle, ProtocolError> {
        self.operator.handle(input, ctx).await
    }
}

/// Provisions execution environments that wrap operators with isolation.
///
/// An environment provider creates sandboxed execution contexts — containers,
/// VMs, Nix sandboxes, or plain processes — and wraps operators to run inside
/// them. The wrapped operator's callers are unaware of the isolation layer;
/// they interact through the standard [`Operator`] interface.
///
/// # Lifecycle
///
/// 1. [`supports`](EnvironmentProvider::supports) — check compatibility.
/// 2. [`provision`](EnvironmentProvider::provision) — create the environment,
///    get back a [`ProvisionedEnv`] implementing [`Operator`].
/// 3. Use the `ProvisionedEnv` as an `Operator` for the session's lifetime.
/// 4. [`teardown`](EnvironmentProvider::teardown) — release resources.
///
/// # Implementations
///
/// - `skg-env-local`: no isolation, pass-through (dev mode)
/// - `skg-env-docker`: container isolation (future)
/// - `skg-env-nix`: Nix sandbox isolation (future)
#[async_trait]
pub trait EnvironmentProvider: Send + Sync {
    /// Check whether this provider can satisfy the given environment spec.
    ///
    /// Returns `true` if all isolation boundaries, credential injection
    /// methods, and resource constraints are supported.
    fn supports(&self, spec: &EnvironmentSpec) -> bool;

    /// Provision an environment and wrap the inner operator to run inside it.
    ///
    /// The returned [`ProvisionedEnv`] delegates [`Operator::handle`] calls
    /// to the inner operator within the provisioned isolation boundary.
    /// Credentials from `spec.credentials` are resolved and injected according
    /// to their [`CredentialInjection`] method.
    ///
    /// # Errors
    ///
    /// - [`EnvError::ProvisionFailed`] if the environment cannot be created.
    /// - [`EnvError::CredentialFailed`] if credential resolution or injection fails.
    async fn provision(
        &self,
        spec: &EnvironmentSpec,
        inner: Arc<dyn Operator>,
    ) -> Result<ProvisionedEnv, EnvError>;

    /// Tear down a provisioned environment and release its resources.
    ///
    /// `env_id` must match a previously provisioned [`ProvisionedEnv::env_id`].
    /// After teardown, the wrapped operator is no longer usable.
    ///
    /// Implementations should be idempotent — tearing down an already-destroyed
    /// environment should succeed or return a descriptive error, not panic.
    async fn teardown(&self, env_id: &str) -> Result<(), EnvError>;
}
