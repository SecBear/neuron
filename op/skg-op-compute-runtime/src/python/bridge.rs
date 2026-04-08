//! Internal Python bridge scaffolding (placeholder for Task 3).
//! Future tasks will implement subprocess orchestration here.

use crate::backend::{BackendExecRequest, BackendExecResponse};

/// Render the full code payload to send to the backend.
/// For Task 3 this only injects the core prelude and user code passthrough.
#[allow(dead_code)]
pub(crate) fn compose_backend_request(prelude: &str, user_code: &str) -> BackendExecRequest {
    let mut code = String::with_capacity(prelude.len() + user_code.len() + 2);
    code.push_str(prelude);
    if !prelude.ends_with('\n') {
        code.push('\n');
    }
    code.push_str(user_code);
    BackendExecRequest { code }
}

/// Placeholder transform from backend response to higher-level reporting.
#[allow(dead_code)]
pub(crate) fn interpret_backend_response(resp: BackendExecResponse) -> BackendExecResponse {
    // No-op for now: the runtime uses BackendExecResponse directly in Task 2 tests.
    resp
}
