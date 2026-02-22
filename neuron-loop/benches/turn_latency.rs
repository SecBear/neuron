use criterion::{criterion_group, criterion_main, Criterion};
use neuron_loop::AgentLoop;
use neuron_tool::ToolRegistry;
use neuron_types::*;

/// A mock provider that returns immediately with a fixed response.
#[derive(Clone)]
struct InstantProvider;

impl Provider for InstantProvider {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        Ok(CompletionResponse {
            id: "bench".to_string(),
            model: "mock".to_string(),
            message: Message::assistant("Done"),
            usage: TokenUsage::default(),
            stop_reason: StopReason::EndTurn,
        })
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamHandle, ProviderError> {
        Err(ProviderError::InvalidRequest(
            "not implemented".to_string(),
        ))
    }
}

/// A no-op context strategy that never compacts.
struct NoOpContext;

impl ContextStrategy for NoOpContext {
    fn should_compact(&self, _messages: &[Message], _token_count: usize) -> bool {
        false
    }

    async fn compact(&self, _messages: Vec<Message>) -> Result<Vec<Message>, ContextError> {
        unreachable!()
    }

    fn token_estimate(&self, _messages: &[Message]) -> usize {
        0
    }
}

fn bench_single_turn(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("single_turn_no_tools", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut agent = AgentLoop::builder(InstantProvider, NoOpContext)
                    .max_turns(1)
                    .tools(ToolRegistry::new())
                    .build();
                let ctx = ToolContext::default();
                let _ = agent.run(Message::user("Hello"), &ctx).await;
            })
        })
    });
}

criterion_group!(benches, bench_single_turn);
criterion_main!(benches);
