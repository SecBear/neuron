use layer0::environment::EnvironmentSpec;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Session reuse policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionReuseMode {
    /// Reuse a session across multiple executions.
    Reuse,
    /// Always execute in a fresh ephemeral session.
    Fresh,
}

/// Policy controlling session lifecycle and reset behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPolicy {
    /// Whether sessions are reused or created fresh.
    pub reuse: SessionReuseMode,
    /// Idle timeout before an inactive session is reclaimed.
    pub idle_timeout: Duration,
    /// Maximum lifetime for a session regardless of activity.
    pub max_lifetime: Duration,
    /// Whether a failed execution should force a reset before the next exec.
    pub reset_on_error: bool,
}

impl Default for SessionPolicy {
    fn default() -> Self {
        Self {
            reuse: SessionReuseMode::Reuse,
            idle_timeout: Duration::from_secs(300),
            max_lifetime: Duration::from_secs(3600),
            reset_on_error: false,
        }
    }
}

/// Declarative execution requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionProfile {
    /// Environment policy reused from layer0.
    pub environment: EnvironmentSpec,
    /// Working directory inside the compute environment, when supported.
    ///
    /// When `None`, the backend may inherit its host process default.
    pub working_dir: Option<PathBuf>,
    /// Session lifecycle policy.
    pub session: SessionPolicy,
}

impl Default for ExecutionProfile {
    fn default() -> Self {
        Self {
            environment: EnvironmentSpec::default(),
            working_dir: None,
            session: SessionPolicy::default(),
        }
    }
}
