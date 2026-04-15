//! Python-specific implementation details for the compute runtime.
//!
//! Binding catalogs, prelude generation, worker protocol, and bridge wiring stay
//! internal to the crate until the extension surface is proven stable.

/// Embedded Python worker script (compiled into the binary).
pub(crate) const WORKER_PY: &str = include_str!("worker.py");

pub(crate) mod prelude_generator;
pub(crate) mod worker_protocol;

use crate::backend::{BackendError, BackendExecRequest, BackendExecResponse, ComputeBackend};
use crate::profile::ExecutionProfile;
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use worker_protocol::{WorkerRequest, WorkerResponse};

#[derive(Clone)]
struct ProcIo {
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Arc<Mutex<ChildStdout>>,
    #[allow(dead_code)]
    stderr: Arc<Mutex<ChildStderr>>,
}

/// Handle for a running Python worker subprocess.
#[derive(Clone)]
pub struct LocalPythonHandle {
    io: ProcIo,
}

/// Minimal local subprocess backend that speaks the worker protocol over stdio.
#[derive(Debug, Default, Clone)]
pub struct LocalPythonBackend;

impl LocalPythonBackend {
    async fn send(&self, io: &ProcIo, msg: &WorkerRequest) -> Result<(), BackendError> {
        let mut w = io.stdin.lock().await;
        let data = serde_json::to_vec(msg).map_err(|e| BackendError::Unavailable(e.to_string()))?;
        let len = (data.len() as u32).to_be_bytes();
        w.write_all(&len)
            .await
            .map_err(|e| BackendError::Execution(e.to_string()))?;
        w.write_all(&data)
            .await
            .map_err(|e| BackendError::Execution(e.to_string()))?;
        w.flush()
            .await
            .map_err(|e| BackendError::Execution(e.to_string()))?;
        Ok(())
    }

    async fn recv(&self, io: &ProcIo) -> Result<WorkerResponse, BackendError> {
        let mut r = io.stdout.lock().await;
        let mut len_buf = [0u8; 4];
        r.read_exact(&mut len_buf)
            .await
            .map_err(|e| BackendError::Execution(e.to_string()))?;
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        r.read_exact(&mut buf)
            .await
            .map_err(|e| BackendError::Execution(e.to_string()))?;
        let resp: WorkerResponse =
            serde_json::from_slice(&buf).map_err(|e| BackendError::Execution(e.to_string()))?;
        Ok(resp)
    }

    fn worker_script() -> Result<PathBuf, BackendError> {
        static SCRIPT_PATH: OnceLock<PathBuf> = OnceLock::new();
        if let Some(path) = SCRIPT_PATH.get() {
            return Ok(path.clone());
        }
        let dir = std::env::temp_dir().join("skelegent-compute");
        std::fs::create_dir_all(&dir)
            .map_err(|e| BackendError::Unavailable(format!("cannot create temp dir: {e}")))?;
        let path = dir.join("worker.py");
        std::fs::write(&path, WORKER_PY)
            .map_err(|e| BackendError::Unavailable(format!("cannot write worker script: {e}")))?;
        Ok(SCRIPT_PATH.get_or_init(|| path).clone())
    }

    async fn spawn_worker(&self, profile: &ExecutionProfile) -> Result<ProcIo, BackendError> {
        let script = Self::worker_script()?;
        let mut cmd = Command::new("python3");
        cmd.arg("-u")
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(working_dir) = &profile.working_dir {
            cmd.current_dir(working_dir);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| BackendError::Unavailable(e.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BackendError::Unavailable("stdin missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BackendError::Unavailable("stdout missing".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| BackendError::Unavailable("stderr missing".into()))?;
        Ok(ProcIo {
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(stdout)),
            stderr: Arc::new(Mutex::new(stderr)),
        })
    }
}

#[async_trait]
impl ComputeBackend for LocalPythonBackend {
    type Handle = LocalPythonHandle;

    async fn start(&self, profile: &ExecutionProfile) -> Result<Self::Handle, BackendError> {
        let io = self.spawn_worker(profile).await?;
        let prelude = prelude_generator::render_default_prelude();
        self.send(&io, &WorkerRequest::Init { prelude }).await?;
        let resp = self.recv(&io).await?;
        if !resp.ok {
            return Err(BackendError::Unavailable(
                resp.error.unwrap_or_else(|| "init failed".into()),
            ));
        }
        Ok(LocalPythonHandle { io })
    }

    async fn exec(
        &self,
        handle: &Self::Handle,
        request: BackendExecRequest,
    ) -> Result<BackendExecResponse, BackendError> {
        self.send(&handle.io, &WorkerRequest::Exec { code: request.code })
            .await?;
        let resp = self.recv(&handle.io).await?;
        Ok(BackendExecResponse {
            stdout: resp.stdout,
            stderr: resp.stderr,
            exit_code: resp.exit_code,
            final_result: resp.final_result,
            notes: resp.notes,
        })
    }

    async fn stop(&self, handle: Self::Handle) -> Result<(), BackendError> {
        let _ = self.send(&handle.io, &WorkerRequest::Close).await;
        Ok(())
    }
}
