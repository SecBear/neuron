use crate::backend::{BackendExecRequest, BackendExecResponse, ComputeBackend};
use crate::error::ComputeError;
use crate::profile::{ExecutionProfile, SessionReuseMode};
use crate::report::ExecutionReport;
use crate::session::ComputeSession;
use async_trait::async_trait;
use layer0::id::SessionId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Session-scoped programmable execution runtime.
#[async_trait]
pub trait ComputeRuntime: Send + Sync {
    /// Execute source code in the given session under the provided profile.
    async fn exec(
        &self,
        session_id: &SessionId,
        code: &str,
        profile: &ExecutionProfile,
    ) -> Result<ExecutionReport, ComputeError>;

    /// Reset the given session.
    async fn reset(&self, session_id: &SessionId) -> Result<(), ComputeError>;

    /// Close the given session and release its resources.
    async fn close(&self, session_id: &SessionId) -> Result<(), ComputeError>;

    /// Inspect metadata for the given session.
    async fn inspect(&self, session_id: &SessionId) -> Result<ComputeSession, ComputeError>;
}

type SessionMap<H> = HashMap<SessionId, Arc<Mutex<SessionEntry<H>>>>;

/// Simple in-memory sessioned compute runtime backed by a `ComputeBackend`.
pub struct InMemoryComputeRuntime<B: ComputeBackend> {
    backend: B,
    runtime_kind: String,
    sessions: Mutex<SessionMap<B::Handle>>,
}

struct SessionEntry<H> {
    handle: Option<H>,
    session: ComputeSession,
    last_profile: ExecutionProfile,
    profile_hash: String,
    closed: bool,
}

impl<B: ComputeBackend> InMemoryComputeRuntime<B> {
    /// Create a new runtime using the provided backend and runtime kind label.
    pub fn new(backend: B, runtime_kind: impl Into<String>) -> Self {
        Self {
            backend,
            runtime_kind: runtime_kind.into(),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn profile_hash(profile: &ExecutionProfile) -> Result<String, ComputeError> {
        serde_json::to_string(profile).map_err(|e| ComputeError::Execution(e.to_string()))
    }

    fn report_from_backend(resp: BackendExecResponse) -> Result<ExecutionReport, ComputeError> {
        if resp.exit_code != 0 {
            let detail = if !resp.stderr.is_empty() {
                resp.stderr
            } else if !resp.stdout.is_empty() {
                resp.stdout
            } else {
                format!("backend exited with {}", resp.exit_code)
            };
            return Err(ComputeError::Execution(detail));
        }
        Ok(ExecutionReport {
            final_result: resp.final_result,
            notes: resp.notes,
            stdout: resp.stdout,
            stderr: resp.stderr,
            ..ExecutionReport::default()
        })
    }

    async fn get_or_create_entry(
        &self,
        session_id: &SessionId,
        profile: &ExecutionProfile,
    ) -> Result<Arc<Mutex<SessionEntry<B::Handle>>>, ComputeError> {
        {
            let sessions = self.sessions.lock().await;
            if let Some(entry) = sessions.get(session_id) {
                return Ok(Arc::clone(entry));
            }
        }

        let handle = self.backend.start(profile).await?;
        let profile_hash = Self::profile_hash(profile)?;
        let mut session = ComputeSession::new(session_id.clone(), self.runtime_kind.clone());
        session.profile_hash = profile_hash.clone();
        let candidate = Arc::new(Mutex::new(SessionEntry {
            handle: Some(handle),
            session,
            last_profile: profile.clone(),
            profile_hash,
            closed: false,
        }));

        let mut sessions = self.sessions.lock().await;
        if let Some(existing) = sessions.get(session_id).cloned() {
            drop(sessions);
            let mut candidate = candidate.lock().await;
            if let Some(handle) = candidate.handle.take() {
                let _ = self.backend.stop(handle).await;
            }
            return Ok(existing);
        }
        sessions.insert(session_id.clone(), Arc::clone(&candidate));
        Ok(candidate)
    }

    async fn ensure_valid_reused_session(
        &self,
        entry: &mut SessionEntry<B::Handle>,
        session_id: &SessionId,
        profile: &ExecutionProfile,
    ) -> Result<(), ComputeError> {
        if entry.closed {
            return Err(ComputeError::SessionNotFound(
                session_id.as_str().to_string(),
            ));
        }
        let requested_hash = Self::profile_hash(profile)?;
        if entry.profile_hash != requested_hash {
            return Err(ComputeError::Execution(format!(
                "session '{}' was created with a different execution profile",
                session_id.as_str()
            )));
        }

        let idle_expired = entry.session.last_used_at.elapsed() > profile.session.idle_timeout;
        let lifetime_expired = entry.session.created_at.elapsed() > profile.session.max_lifetime;
        if idle_expired || lifetime_expired {
            if let Some(old_handle) = entry.handle.take() {
                self.backend.stop(old_handle).await?;
            }
            let new_handle = self.backend.start(profile).await?;
            entry.handle = Some(new_handle);
            let now = std::time::Instant::now();
            entry.session.created_at = now;
            entry.session.last_used_at = now;
        }

        if entry.handle.is_none() {
            let new_handle = self.backend.start(profile).await?;
            entry.handle = Some(new_handle);
        }
        Ok(())
    }

    async fn recycle_session_after_failure(
        &self,
        entry: &mut SessionEntry<B::Handle>,
        profile: &ExecutionProfile,
    ) -> Result<(), ComputeError> {
        let new_handle = self.backend.start(profile).await?;
        let old_handle = entry.handle.replace(new_handle);
        if let Some(old_handle) = old_handle {
            self.backend.stop(old_handle).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl<B> ComputeRuntime for InMemoryComputeRuntime<B>
where
    B: ComputeBackend + Send + Sync,
{
    async fn exec(
        &self,
        session_id: &SessionId,
        code: &str,
        profile: &ExecutionProfile,
    ) -> Result<ExecutionReport, ComputeError> {
        match profile.session.reuse {
            SessionReuseMode::Fresh => {
                let handle = self.backend.start(profile).await?;
                let exec_res = self
                    .backend
                    .exec(&handle, BackendExecRequest { code: code.into() })
                    .await;
                let stop_res = self.backend.stop(handle).await;
                match (exec_res, stop_res) {
                    (Ok(resp), Ok(())) => Self::report_from_backend(resp),
                    (Ok(_), Err(e)) => Err(e.into()),
                    (Err(e), Ok(())) => Err(e.into()),
                    (Err(e), Err(_)) => Err(e.into()),
                }
            }
            SessionReuseMode::Reuse => {
                let entry = self.get_or_create_entry(session_id, profile).await?;
                let mut entry = entry.lock().await;
                self.ensure_valid_reused_session(&mut entry, session_id, profile)
                    .await?;

                let resp = self
                    .backend
                    .exec(
                        entry.handle.as_ref().expect("handle present"),
                        BackendExecRequest { code: code.into() },
                    )
                    .await;
                match resp {
                    Ok(resp) => {
                        let report = Self::report_from_backend(resp);
                        if report.is_err() && profile.session.reset_on_error {
                            self.recycle_session_after_failure(&mut entry, profile)
                                .await?;
                        }
                        entry.session.last_used_at = std::time::Instant::now();
                        report
                    }
                    Err(err) => {
                        if profile.session.reset_on_error {
                            self.recycle_session_after_failure(&mut entry, profile)
                                .await?;
                        }
                        Err(err.into())
                    }
                }
            }
        }
    }

    async fn reset(&self, session_id: &SessionId) -> Result<(), ComputeError> {
        let entry = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| ComputeError::SessionNotFound(session_id.as_str().to_string()))?
        };
        let mut entry = entry.lock().await;
        if entry.closed {
            return Err(ComputeError::SessionNotFound(
                session_id.as_str().to_string(),
            ));
        }
        let new_handle = self.backend.start(&entry.last_profile).await?;
        let old_handle = entry
            .handle
            .replace(new_handle)
            .ok_or_else(|| ComputeError::Execution("session handle missing".into()))?;
        self.backend.stop(old_handle).await?;
        entry.session.last_used_at = std::time::Instant::now();
        Ok(())
    }

    async fn close(&self, session_id: &SessionId) -> Result<(), ComputeError> {
        let entry = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| ComputeError::SessionNotFound(session_id.as_str().to_string()))?
        };
        let handle = {
            let mut entry = entry.lock().await;
            entry.closed = true;
            entry
                .handle
                .take()
                .ok_or_else(|| ComputeError::Execution("session handle missing".into()))?
        };
        {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(session_id);
        }
        self.backend.stop(handle).await?;
        Ok(())
    }

    async fn inspect(&self, session_id: &SessionId) -> Result<ComputeSession, ComputeError> {
        let entry = {
            let sessions = self.sessions.lock().await;
            sessions
                .get(session_id)
                .cloned()
                .ok_or_else(|| ComputeError::SessionNotFound(session_id.as_str().to_string()))?
        };
        let entry = entry.lock().await;
        if entry.closed {
            return Err(ComputeError::SessionNotFound(
                session_id.as_str().to_string(),
            ));
        }
        Ok(entry.session.clone())
    }
}
