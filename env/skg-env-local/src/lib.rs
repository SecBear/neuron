#![deny(missing_docs)]
//! Local (no-isolation) environment provider for skelegent.
//!
//! [`LocalEnvironmentProvider`] is a pass-through implementation of
//! [`EnvironmentProvider`] that applies no isolation boundaries. The inner
//! operator runs in the same process, filesystem, and network as the caller.
//!
//! This is the default for local development. Production deployments should
//! use `skg-env-docker` or `skg-env-nix` for real isolation.
//!
//! # What it does
//!
//! - **Isolation**: none. All boundaries in the spec are ignored.
//! - **Credentials**: not injected. Credential refs are acknowledged but
//!   no resolution or injection occurs — the operator is expected to obtain
//!   credentials through constructor injection (Tier 2 security model).
//! - **Resources**: not enforced. Resource limits are ignored.
//! - **Network**: not enforced. Network policies are ignored.
//!
//! # What it doesn't do
//!
//! This provider does not reject specs with container/VM/Wasm isolation
//! requirements. It returns `false` from [`supports`](EnvironmentProvider::supports)
//! for those specs, letting the caller decide whether to proceed without
//! isolation or fail loudly.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use layer0::environment::{
    EnvironmentProvider, EnvironmentSpec, IsolationBoundary, ProvisionedEnv,
};
use layer0::error::EnvError;
use layer0::operator::Operator;

/// Local (no-isolation) environment provider.
///
/// Wraps operators with zero overhead — the returned [`ProvisionedEnv`]
/// delegates directly to the inner operator. Teardown is a no-op.
///
/// # Supported specs
///
/// [`supports`](EnvironmentProvider::supports) returns `true` only when the
/// spec requires no isolation or only [`IsolationBoundary::Process`] isolation
/// (which the local provider satisfies trivially — everything already runs in
/// a process).
///
/// Specs requiring container, VM, Wasm, gVisor, or custom isolation return
/// `false`.
pub struct LocalEnvironmentProvider {
    counter: AtomicU64,
}

impl LocalEnvironmentProvider {
    /// Create a new local environment provider.
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
        }
    }
}

impl Default for LocalEnvironmentProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LocalEnvironmentProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalEnvironmentProvider")
            .field("provisioned", &self.counter.load(Ordering::Relaxed))
            .finish()
    }
}

#[async_trait]
impl EnvironmentProvider for LocalEnvironmentProvider {
    fn supports(&self, spec: &EnvironmentSpec) -> bool {
        // Accept specs with no isolation or only Process isolation.
        // Reject anything that requires real sandboxing.
        spec.isolation
            .iter()
            .all(|boundary| matches!(boundary, IsolationBoundary::Process))
    }

    async fn provision(
        &self,
        _spec: &EnvironmentSpec,
        inner: Arc<dyn Operator>,
    ) -> Result<ProvisionedEnv, EnvError> {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        Ok(ProvisionedEnv::new(format!("local-{id}"), inner))
    }

    async fn teardown(&self, _env_id: &str) -> Result<(), EnvError> {
        // No resources to release in local mode.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layer0::environment::IsolationBoundary;
    use layer0::test_utils::EchoOperator;

    fn local() -> LocalEnvironmentProvider {
        LocalEnvironmentProvider::new()
    }

    fn echo() -> Arc<dyn Operator> {
        Arc::new(EchoOperator)
    }

    // -- supports --

    #[test]
    fn supports_empty_spec() {
        assert!(local().supports(&EnvironmentSpec::default()));
    }

    #[test]
    fn supports_process_isolation() {
        let spec = EnvironmentSpec::new().with_isolation(vec![IsolationBoundary::Process]);
        assert!(local().supports(&spec));
    }

    #[test]
    fn rejects_container_isolation() {
        let spec = EnvironmentSpec::new()
            .with_isolation(vec![IsolationBoundary::Container { image: None }]);
        assert!(!local().supports(&spec));
    }

    #[test]
    fn rejects_mixed_isolation_with_container() {
        let spec = EnvironmentSpec::new().with_isolation(vec![
            IsolationBoundary::Process,
            IsolationBoundary::Container {
                image: Some("ubuntu:latest".into()),
            },
        ]);
        assert!(!local().supports(&spec));
    }

    #[test]
    fn rejects_gvisor() {
        let spec = EnvironmentSpec::new().with_isolation(vec![IsolationBoundary::Gvisor]);
        assert!(!local().supports(&spec));
    }

    #[test]
    fn rejects_micro_vm() {
        let spec = EnvironmentSpec::new().with_isolation(vec![IsolationBoundary::MicroVm]);
        assert!(!local().supports(&spec));
    }

    #[test]
    fn rejects_wasm() {
        let spec =
            EnvironmentSpec::new().with_isolation(vec![IsolationBoundary::Wasm { runtime: None }]);
        assert!(!local().supports(&spec));
    }

    // -- provision / teardown --

    #[tokio::test]
    async fn provision_returns_env_with_sequential_ids() {
        let provider = local();
        let env0 = provider
            .provision(&EnvironmentSpec::default(), echo())
            .await
            .expect("provision 0");
        let env1 = provider
            .provision(&EnvironmentSpec::default(), echo())
            .await
            .expect("provision 1");

        assert_eq!(env0.env_id, "local-0");
        assert_eq!(env1.env_id, "local-1");
    }

    #[tokio::test]
    async fn provisioned_env_delegates_to_inner_operator() {
        use layer0::content::Content;
        use layer0::dispatch_context::DispatchContext;
        use layer0::id::{DispatchId, OperatorId};
        use layer0::operator::{OperatorInput, Outcome, TerminalOutcome, TriggerType};

        let provider = local();
        let env = provider
            .provision(&EnvironmentSpec::default(), echo())
            .await
            .expect("provision");

        let input = OperatorInput::new(Content::text("hello"), TriggerType::User);
        let ctx = DispatchContext::new(DispatchId::new("test"), OperatorId::new("echo"));
        let handle = env.handle(input, &ctx).await.expect("handle");
        let output = handle.collect().await.expect("collect");

        assert_eq!(output.message, Content::text("hello"));
        assert!(matches!(
            output.outcome,
            Outcome::Terminal {
                terminal: TerminalOutcome::Completed,
            }
        ));
    }

    #[tokio::test]
    async fn teardown_is_idempotent() {
        let provider = local();
        let env = provider
            .provision(&EnvironmentSpec::default(), echo())
            .await
            .expect("provision");

        provider.teardown(&env.env_id).await.expect("teardown 1");
        provider.teardown(&env.env_id).await.expect("teardown 2");
        provider
            .teardown("nonexistent")
            .await
            .expect("teardown unknown");
    }

    #[test]
    fn debug_impl_shows_counter() {
        let provider = local();
        let debug = format!("{provider:?}");
        assert!(debug.contains("LocalEnvironmentProvider"));
        assert!(debug.contains("provisioned"));
    }

    #[test]
    fn default_impl_works() {
        let _provider: LocalEnvironmentProvider = Default::default();
    }
}
