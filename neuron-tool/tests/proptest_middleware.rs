//! Property-based tests: middleware chain ordering.

use proptest::prelude::*;
use neuron_tool::*;
use neuron_types::*;
use std::sync::{Arc, Mutex};

/// A logging middleware that records its index when invoked.
struct OrderMiddleware {
    index: usize,
    log: Arc<Mutex<Vec<usize>>>,
}

impl ToolMiddleware for OrderMiddleware {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            self.log.lock().unwrap().push(self.index);
            next.run(call, ctx).await
        })
    }
}

/// A no-op tool for middleware ordering tests.
struct NoOpTool;

impl Tool for NoOpTool {
    const NAME: &'static str = "noop";
    type Args = serde_json::Value;
    type Output = serde_json::Value;
    type Error = ToolError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "noop".to_string(),
            title: None,
            description: "no-op tool for testing".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    fn call(
        &self,
        _args: Self::Args,
        _ctx: &ToolContext,
    ) -> impl std::future::Future<Output = Result<Self::Output, Self::Error>> + Send {
        async { Ok(serde_json::json!(null)) }
    }
}

proptest! {
    #[test]
    fn middleware_execution_order(n_middleware in 2usize..6) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let log = Arc::new(Mutex::new(Vec::new()));
            let mut registry = ToolRegistry::new();
            registry.register(NoOpTool);

            for i in 0..n_middleware {
                registry.add_middleware(OrderMiddleware {
                    index: i,
                    log: log.clone(),
                });
            }

            let ctx = ToolContext::default();
            let _ = registry.execute("noop", serde_json::json!(null), &ctx).await;

            let recorded = log.lock().unwrap().clone();
            assert_eq!(recorded.len(), n_middleware,
                "Expected {} middleware calls, got {}", n_middleware, recorded.len());
            for (idx, &val) in recorded.iter().enumerate() {
                assert_eq!(idx, val,
                    "Middleware {} ran at position {}", val, idx);
            }
        });
    }
}
