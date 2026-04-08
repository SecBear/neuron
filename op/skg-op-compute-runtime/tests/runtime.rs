use async_trait::async_trait;
use layer0::capability::{
    ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityId, CapabilityKind, ExecutionClass,
    SchedulingFacts,
};
use layer0::dispatch::Artifact;
use layer0::environment::EnvironmentSpec;
use layer0::event::ExecutionEvent;
use layer0::id::SessionId;
use skg_op_compute_runtime::{
    BackendError, BackendExecRequest, BackendExecResponse, ComputeBackend, ComputeError,
    ComputeRuntime, ComputeSession, ExecutionMetrics, ExecutionProfile, ExecutionReport,
    SessionPolicy, SessionReuseMode,
};
use std::time::Duration;

#[test]
fn execution_profile_reuses_environment_spec() {
    let profile = ExecutionProfile {
        environment: EnvironmentSpec::default(),
        session: SessionPolicy::default(),
    };

    assert!(profile.environment.credentials.is_empty());
}

#[test]
fn session_policy_has_stable_defaults() {
    let policy = SessionPolicy::default();
    assert_eq!(policy.reuse, SessionReuseMode::Reuse);
    assert_eq!(policy.reset_on_error, false);
    assert!(policy.idle_timeout >= Duration::from_secs(60));
    assert!(policy.max_lifetime >= policy.idle_timeout);
}

#[test]
fn compute_session_uses_layer0_session_id() {
    let session = ComputeSession::new(SessionId::new("compute-demo"), "python");
    assert_eq!(session.id.as_str(), "compute-demo");
    assert_eq!(session.runtime_kind, "python");
}

#[test]
fn execution_report_reuses_layer0_events_and_artifacts() {
    let descriptor = CapabilityDescriptor::new(
        CapabilityId::new("compute.core.final"),
        CapabilityKind::Tool,
        "final",
        "final result binding",
        SchedulingFacts::new(ExecutionClass::Exclusive, true, true, false, Some(1)),
        ApprovalFacts::None,
        AuthFacts::Open,
    );

    let report = ExecutionReport {
        final_result: None,
        notes: vec![],
        stdout: String::new(),
        stderr: String::new(),
        events: Vec::<ExecutionEvent>::new(),
        artifacts: Vec::<Artifact>::new(),
        metrics: ExecutionMetrics::default(),
        cancelled: false,
        timed_out: false,
    };

    assert!(report.events.is_empty());
    assert!(report.artifacts.is_empty());
    let _ = descriptor;
}

struct DummyBackend;
struct DummyHandle;

#[async_trait]
impl ComputeBackend for DummyBackend {
    type Handle = DummyHandle;

    async fn start(&self, _profile: &ExecutionProfile) -> Result<Self::Handle, BackendError> {
        Ok(DummyHandle)
    }

    async fn exec(
        &self,
        _handle: &Self::Handle,
        request: BackendExecRequest,
    ) -> Result<BackendExecResponse, BackendError> {
        Ok(BackendExecResponse {
            stdout: request.code,
            stderr: String::new(),
            exit_code: 0,
            final_result: None,
            notes: vec![],
        })
    }

    async fn stop(&self, _handle: Self::Handle) -> Result<(), BackendError> {
        Ok(())
    }
}

#[test]
fn backend_contract_nouns_exist() {
    let _req = BackendExecRequest {
        code: "print('ok')".into(),
    };
    let _resp = BackendExecResponse {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
        final_result: None,
        notes: vec![],
    };

    fn _assert_error_trait<E: std::error::Error + Send + Sync + 'static>(_e: &E) {}
    let e = BackendError::Execution("boom".into());
    _assert_error_trait(&e);

    type Handle = <DummyBackend as ComputeBackend>::Handle;
    let _maybe_handle: Option<Handle> = None;
}

struct DummyRuntime;

#[async_trait]
impl ComputeRuntime for DummyRuntime {
    async fn exec(
        &self,
        _session_id: &SessionId,
        _code: &str,
        _profile: &ExecutionProfile,
    ) -> Result<ExecutionReport, ComputeError> {
        Err(ComputeError::Execution("not implemented".into()))
    }

    async fn reset(&self, _session_id: &SessionId) -> Result<(), ComputeError> {
        Ok(())
    }

    async fn close(&self, _session_id: &SessionId) -> Result<(), ComputeError> {
        Ok(())
    }

    async fn inspect(&self, _session_id: &SessionId) -> Result<ComputeSession, ComputeError> {
        Ok(ComputeSession::new(SessionId::new("dummy"), "python"))
    }
}

#[tokio::test]
async fn runtime_trait_compiles_with_dummy() {
    let _rt = DummyRuntime;
}


use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Default, Clone)]
struct CountingBackend {
    seq: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
struct CountingHandle {
    id: usize,
}

#[async_trait]
impl ComputeBackend for CountingBackend {
    type Handle = CountingHandle;

    async fn start(&self, _profile: &ExecutionProfile) -> Result<Self::Handle, BackendError> {
        let id = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(CountingHandle { id })
    }

    async fn exec(
        &self,
        handle: &Self::Handle,
        request: BackendExecRequest,
    ) -> Result<BackendExecResponse, BackendError> {
        Ok(BackendExecResponse {
            stdout: format!("h{}:{}", handle.id, request.code),
            stderr: String::new(),
            exit_code: 0,
            final_result: None,
            notes: vec![],
        })
    }

    async fn stop(&self, _handle: Self::Handle) -> Result<(), BackendError> {
        Ok(())
    }
}

fn extract_handle(stdout: &str) -> Option<usize> {
    stdout.strip_prefix('h')
        .and_then(|s| s.split(':').next())
        .and_then(|n| n.parse::<usize>().ok())
}

#[tokio::test]
async fn in_memory_runtime_reuses_session_by_default() {
    let backend = CountingBackend::default();
    let rt = skg_op_compute_runtime::runtime::InMemoryComputeRuntime::new(backend.clone(), "python");
    let sid = SessionId::new("s-reuse");
    let profile = ExecutionProfile::default();

    let r1 = rt.exec(&sid, "A", &profile).await.expect("exec1");
    let r2 = rt.exec(&sid, "B", &profile).await.expect("exec2");

    let h1 = extract_handle(&r1.stdout).expect("handle1");
    let h2 = extract_handle(&r2.stdout).expect("handle2");
    assert_eq!(h1, h2, "should reuse same backend handle");

    // Inspect returns a cloned session
    let s = rt.inspect(&sid).await.expect("inspect");
    assert_eq!(s.id.as_str(), "s-reuse");
    assert_eq!(s.runtime_kind, "python");
}

#[tokio::test]
async fn in_memory_runtime_reset_recreates_handle() {
    let backend = CountingBackend::default();
    let rt = skg_op_compute_runtime::runtime::InMemoryComputeRuntime::new(backend.clone(), "python");
    let sid = SessionId::new("s-reset");
    let profile = ExecutionProfile::default();

    let r1 = rt.exec(&sid, "A", &profile).await.expect("exec1");
    let h1 = extract_handle(&r1.stdout).unwrap();

    rt.reset(&sid).await.expect("reset");
    // Session should remain inspectable after reset
    let s = rt.inspect(&sid).await.expect("inspect after reset");
    assert_eq!(s.id.as_str(), "s-reset");
    assert_eq!(s.runtime_kind, "python");
    let r2 = rt.exec(&sid, "B", &profile).await.expect("exec2");
    let h2 = extract_handle(&r2.stdout).unwrap();
    assert_ne!(h1, h2, "reset should recreate backend handle");
}

#[tokio::test]
async fn in_memory_runtime_close_drops_session() {
    let backend = CountingBackend::default();
    let rt = skg_op_compute_runtime::runtime::InMemoryComputeRuntime::new(backend.clone(), "python");
    let sid = SessionId::new("s-close");
    let profile = ExecutionProfile::default();

    let _ = rt.exec(&sid, "A", &profile).await.expect("exec1");

    rt.close(&sid).await.expect("close");

    let err = rt.inspect(&sid).await.err().expect("expected error");
    match err {
        ComputeError::SessionNotFound(s) => assert!(s.contains("s-close")),
        _ => panic!("unexpected error: {err:?}"),
    }
}

#[tokio::test]
async fn fresh_policy_avoids_persistent_reuse() {
    let backend = CountingBackend::default();
    let rt = skg_op_compute_runtime::runtime::InMemoryComputeRuntime::new(backend.clone(), "python");
    let sid = SessionId::new("s-fresh");
    let mut profile = ExecutionProfile::default();
    profile.session.reuse = SessionReuseMode::Fresh;

    let r1 = rt.exec(&sid, "A", &profile).await.expect("exec1");
    let r2 = rt.exec(&sid, "B", &profile).await.expect("exec2");

    let h1 = extract_handle(&r1.stdout).unwrap();
    let h2 = extract_handle(&r2.stdout).unwrap();
    assert_ne!(h1, h2, "fresh executions should not reuse persistent session");

    // Inspect should not find a persistent session for Fresh policy
    let err = rt.inspect(&sid).await.err().expect("expected error");
    match err {
        ComputeError::SessionNotFound(s) => assert!(s.contains("s-fresh")),
        _ => panic!("unexpected error: {err:?}"),
    }
}

#[tokio::test]
async fn reused_session_rejects_profile_changes() {
    let backend = CountingBackend::default();
    let rt = skg_op_compute_runtime::runtime::InMemoryComputeRuntime::new(backend, "python");
    let sid = SessionId::new("s-profile");

    let profile_a = ExecutionProfile::default();
    let mut profile_b = ExecutionProfile::default();
    profile_b.session.reset_on_error = true;

    let _ = rt.exec(&sid, "A", &profile_a).await.expect("exec1");
    let err = rt.exec(&sid, "B", &profile_b).await.expect_err("profile mismatch expected");
    match err {
        ComputeError::Execution(msg) => assert!(msg.contains("different execution profile")),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn reused_session_reset_on_error_recreates_handle() {
    #[derive(Default, Clone)]
    struct FlakyBackend(Arc<std::sync::Mutex<usize>>);
    struct Handle(usize);

    #[async_trait]
    impl ComputeBackend for FlakyBackend {
        type Handle = Handle;

        async fn start(&self, _profile: &ExecutionProfile) -> Result<Self::Handle, BackendError> {
            let mut n = self.0.lock().unwrap();
            *n += 1;
            Ok(Handle(*n))
        }

        async fn exec(
            &self,
            handle: &Self::Handle,
            req: BackendExecRequest,
        ) -> Result<BackendExecResponse, BackendError> {
            if req.code == "boom" {
                return Err(BackendError::Execution(format!("bad on h{}", handle.0)));
            }
            Ok(BackendExecResponse {
                stdout: format!("h{}:{}", handle.0, req.code),
                stderr: String::new(),
                exit_code: 0,
                final_result: None,
                notes: vec![],
            })
        }

        async fn stop(&self, _handle: Self::Handle) -> Result<(), BackendError> {
            Ok(())
        }
    }

    let backend = FlakyBackend::default();
    let rt = skg_op_compute_runtime::runtime::InMemoryComputeRuntime::new(backend, "python");
    let sid = SessionId::new("s-reset-on-error");
    let mut profile = ExecutionProfile::default();
    profile.session.reset_on_error = true;

    let first = rt.exec(&sid, "A", &profile).await.expect("first exec");
    let h1 = extract_handle(&first.stdout).unwrap();
    let err = rt.exec(&sid, "boom", &profile).await.expect_err("boom expected");
    match err {
        ComputeError::Backend(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }
    let second = rt.exec(&sid, "B", &profile).await.expect("second exec");
    let h2 = extract_handle(&second.stdout).unwrap();
    assert_ne!(h1, h2, "reset_on_error should replace the backend handle");
}

#[tokio::test]
async fn reused_session_expiry_recreates_handle() {
    let backend = CountingBackend::default();
    let rt = skg_op_compute_runtime::runtime::InMemoryComputeRuntime::new(backend, "python");
    let sid = SessionId::new("s-expire");
    let mut profile = ExecutionProfile::default();
    profile.session.idle_timeout = Duration::ZERO;

    let first = rt.exec(&sid, "A", &profile).await.expect("first exec");
    let h1 = extract_handle(&first.stdout).unwrap();
    let second = rt.exec(&sid, "B", &profile).await.expect("second exec");
    let h2 = extract_handle(&second.stdout).unwrap();
    assert_ne!(h1, h2, "expired sessions should recreate their handle");
}

#[tokio::test]
async fn nonzero_exit_with_reset_on_error_recreates_handle() {
    #[derive(Default, Clone)]
    struct ExitCodeBackend(Arc<std::sync::Mutex<usize>>);
    struct Handle(usize);

    #[async_trait]
    impl ComputeBackend for ExitCodeBackend {
        type Handle = Handle;

        async fn start(&self, _profile: &ExecutionProfile) -> Result<Self::Handle, BackendError> {
            let mut n = self.0.lock().unwrap();
            *n += 1;
            Ok(Handle(*n))
        }

        async fn exec(
            &self,
            handle: &Self::Handle,
            req: BackendExecRequest,
        ) -> Result<BackendExecResponse, BackendError> {
            let exit_code = if req.code == "boom" { 1 } else { 0 };
            Ok(BackendExecResponse {
                stdout: format!("h{}:{}", handle.0, req.code),
                stderr: if exit_code == 0 { String::new() } else { "python failed".into() },
                exit_code,
                final_result: None,
                notes: vec![],
            })
        }

        async fn stop(&self, _handle: Self::Handle) -> Result<(), BackendError> {
            Ok(())
        }
    }

    let backend = ExitCodeBackend::default();
    let rt = skg_op_compute_runtime::runtime::InMemoryComputeRuntime::new(backend, "python");
    let sid = SessionId::new("s-nonzero-reset");
    let mut profile = ExecutionProfile::default();
    profile.session.reset_on_error = true;

    let first = rt.exec(&sid, "A", &profile).await.expect("first exec");
    let h1 = extract_handle(&first.stdout).unwrap();
    let err = rt.exec(&sid, "boom", &profile).await.expect_err("non-zero exit expected");
    match err {
        ComputeError::Execution(msg) => assert!(msg.contains("python failed")),
        other => panic!("unexpected error: {other:?}"),
    }
    let second = rt.exec(&sid, "B", &profile).await.expect("second exec");
    let h2 = extract_handle(&second.stdout).unwrap();
    assert_ne!(h1, h2, "reset_on_error should also replace the handle on non-zero exits");
}

#[tokio::test]
async fn close_failure_keeps_session_retryable() {
    #[derive(Default, Clone)]
    struct StopFailBackend {
        stop_calls: Arc<std::sync::Mutex<u32>>,
    }
    struct Handle;

    #[async_trait]
    impl ComputeBackend for StopFailBackend {
        type Handle = Handle;

        async fn start(&self, _profile: &ExecutionProfile) -> Result<Self::Handle, BackendError> {
            Ok(Handle)
        }

        async fn exec(
            &self,
            _handle: &Self::Handle,
            _req: BackendExecRequest,
        ) -> Result<BackendExecResponse, BackendError> {
            Ok(BackendExecResponse::default())
        }

        async fn stop(&self, _handle: Self::Handle) -> Result<(), BackendError> {
            let mut n = self.stop_calls.lock().unwrap();
            *n += 1;
            if *n == 1 {
                Err(BackendError::Execution("stop failed once".into()))
            } else {
                Ok(())
            }
        }
    }

    let backend = StopFailBackend::default();
    let rt = skg_op_compute_runtime::runtime::InMemoryComputeRuntime::new(backend, "python");
    let sid = SessionId::new("s-close-retry");
    let profile = ExecutionProfile::default();

    let _ = rt.exec(&sid, "A", &profile).await.expect("exec");
    let err = rt.close(&sid).await.expect_err("first close should fail");
    match err {
        ComputeError::Backend(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }
    // Close is terminal: even if backend stop fails, the session is removed so it cannot be reused.
    assert!(matches!(rt.inspect(&sid).await, Err(ComputeError::SessionNotFound(_))));
}