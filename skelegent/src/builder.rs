//! Ergonomic builder for assembling [`AgentLoop`]s.
//!
//! The builder wires a [`Provider`], an [`AgentBehaviour`], a [`Router`] of
//! tools, and a [`ReactivePipeline`] factory into a ready-to-use
//! [`AgentLoop`].
//!
//! ```no_run
//! use skelegent::prelude::*;
//! use skelegent::builder::agent;
//!
//! # #[derive(Default)] struct MyBehaviour;
//! # #[async_trait] impl AgentBehaviour for MyBehaviour {
//! #     async fn init_context(&self, _: &OperatorInput, _: &DispatchContext) -> Context { Context::new() }
//! #     fn capabilities(&self, _: &Context) -> Vec<CapabilityDescriptor> { vec![] }
//! #     async fn handle_response(&self, _: &InferResponse, _: &mut Context) -> LoopDecision {
//! #         LoopDecision::Complete(OperatorOutput::new(
//! #             Content::text("done"),
//! #             Outcome::Terminal { terminal: TerminalOutcome::Completed },
//! #         ))
//! #     }
//! #     async fn handle_action_result(&self, _: &OperatorId, _: &OperatorOutput, _: &mut Context) -> LoopDecision {
//! #         LoopDecision::Continue
//! #     }
//! # }
//! # fn example<P: Provider + 'static>(provider: P) {
//! let my_agent = agent(provider, MyBehaviour::default())
//!     .id("my.agent")
//!     .name("my-agent")
//!     .description("Solves problems")
//!     .build();
//! # }
//! ```

use std::sync::Arc;

use layer0::OperatorId;
use layer0::capability::{
    ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityId, CapabilityKind, ExecutionClass,
    SchedulingFacts,
};
use layer0::operator::Operator;
use skg_context_engine::{AgentBehaviour, AgentLoop, PipelineFactory, ReactivePipeline, Router};
use skg_turn::Provider;

/// Start building an agent.
///
/// The caller supplies the [`Provider`] and [`AgentBehaviour`] because both
/// carry generic parameters. The builder provides defaults for everything
/// else (empty router, empty pipeline).
///
/// This is a convenience wrapper for [`AgentBuilder::new`].
pub fn agent<P, B>(provider: P, behaviour: B) -> AgentBuilder<P, B>
where
    P: Provider + 'static,
    B: AgentBehaviour + 'static,
{
    AgentBuilder::new(provider, behaviour)
}

/// Builder for [`AgentLoop`].
///
/// Configure the agent step-by-step, then call [`build`](AgentBuilder::build).
///
/// # Generic parameters
///
/// - `P`: the [`Provider`] implementation. [`Provider`] is RPITIT, so this
///   generic is monomorphized at build time.
/// - `B`: the [`AgentBehaviour`] that drives the loop's decisions.
pub struct AgentBuilder<P, B>
where
    P: Provider,
    B: AgentBehaviour,
{
    provider: P,
    behaviour: B,
    router: Router,
    descriptor: CapabilityDescriptor,
    pipeline_factory: Option<PipelineFactory>,
}

impl<P, B> AgentBuilder<P, B>
where
    P: Provider + 'static,
    B: AgentBehaviour + 'static,
{
    /// Create a new builder with default descriptor and empty router.
    pub fn new(provider: P, behaviour: B) -> Self {
        let descriptor = default_descriptor();
        Self {
            provider,
            behaviour,
            router: Router::new("skelegent.agent", "skelegent-agent"),
            descriptor,
            pipeline_factory: None,
        }
    }

    /// Set the agent's [`CapabilityId`].
    pub fn id(mut self, id: impl Into<CapabilityId>) -> Self {
        self.descriptor.id = id.into();
        self
    }

    /// Set the human-readable agent name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.descriptor.name = name.into();
        self
    }

    /// Set the human-readable agent description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.descriptor.description = description.into();
        self
    }

    /// Register a tool operator under the given [`OperatorId`].
    ///
    /// Adds `op` to the internal [`Router`] under `id`. When the model
    /// issues a tool call with this name, the router dispatches to `op`.
    pub fn tool(mut self, id: impl Into<OperatorId>, op: Arc<dyn Operator>) -> Self {
        self.router.register(id, op);
        self
    }

    /// Replace the router entirely.
    ///
    /// Use this when you need custom routing behaviour that goes beyond
    /// the default name-matching [`Router`] (e.g., a composed router, a
    /// supervisor, or a provisioned environment wrapping the router).
    pub fn router(mut self, router: Router) -> Self {
        self.router = router;
        self
    }

    /// Set a custom pipeline factory.
    ///
    /// The factory is invoked once per [`AgentLoop`] invocation to produce a
    /// fresh [`ReactivePipeline`]. When no factory is set, each invocation
    /// uses an empty pipeline.
    ///
    /// Use this to install [`ContextOp`](skg_context_engine::ContextOp)s for
    /// budget guards, approval gates, output sanitizers, etc.
    pub fn pipeline(mut self, factory: PipelineFactory) -> Self {
        self.pipeline_factory = Some(factory);
        self
    }

    /// Consume the builder and produce a configured [`AgentLoop`].
    pub fn build(self) -> AgentLoop<P, B> {
        let factory = self
            .pipeline_factory
            .unwrap_or_else(|| Arc::new(ReactivePipeline::new));
        AgentLoop::new(
            self.descriptor,
            self.provider,
            self.behaviour,
            Arc::new(self.router),
            factory,
        )
    }
}

/// Default capability descriptor for unnamed agents.
///
/// Callers override `id`, `name`, and `description` via the builder.
fn default_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new(
        CapabilityId::new("skelegent.agent"),
        CapabilityKind::Agent,
        "skelegent-agent",
        "Skelegent agent loop",
        SchedulingFacts::new(ExecutionClass::Shared, false, false, true, None),
        ApprovalFacts::None,
        AuthFacts::Open,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use layer0::content::Content;
    use layer0::dispatch_context::DispatchContext;
    use layer0::id::DispatchId;
    use layer0::operator::{OperatorInput, OperatorOutput, Outcome, TerminalOutcome, TriggerType};
    use layer0::test_utils::EchoOperator;
    use skg_context_engine::{AgentBehaviour, Context, LoopDecision};
    use skg_turn::InferResponse;
    use skg_turn::test_utils::{TestProvider, make_text_response};

    // Minimal behaviour: completes on first response.
    struct Completer;

    #[async_trait]
    impl AgentBehaviour for Completer {
        async fn init_context(&self, _input: &OperatorInput, _ctx: &DispatchContext) -> Context {
            Context::new()
        }

        fn capabilities(&self, _ctx: &Context) -> Vec<CapabilityDescriptor> {
            vec![]
        }

        async fn handle_response(
            &self,
            response: &InferResponse,
            _ctx: &mut Context,
        ) -> LoopDecision {
            let text = response.text().unwrap_or("").to_string();
            LoopDecision::Complete(OperatorOutput::new(
                Content::text(text),
                Outcome::Terminal {
                    terminal: TerminalOutcome::Completed,
                },
            ))
        }

        async fn handle_action_result(
            &self,
            _action: &OperatorId,
            _result: &OperatorOutput,
            _ctx: &mut Context,
        ) -> LoopDecision {
            LoopDecision::Continue
        }
    }

    fn test_provider(msg: &str) -> TestProvider {
        TestProvider::with_responses(vec![make_text_response(msg)])
    }

    #[tokio::test]
    async fn builder_produces_working_agent_loop() {
        let agent_op = agent(test_provider("hello from provider"), Completer)
            .id("test.agent")
            .name("test-agent")
            .description("test")
            .build();

        let input = OperatorInput::new(Content::text("go"), TriggerType::User);
        let ctx = DispatchContext::new(DispatchId::new("d-1"), OperatorId::new("test.agent"));
        let output = agent_op
            .handle(input, &ctx)
            .await
            .expect("handle")
            .collect()
            .await
            .expect("collect");

        assert_eq!(output.message.as_text(), Some("hello from provider"));
        assert!(matches!(
            output.outcome,
            Outcome::Terminal {
                terminal: TerminalOutcome::Completed,
            }
        ));
    }

    #[tokio::test]
    async fn builder_accepts_tools() {
        let agent_op = agent(test_provider("x"), Completer)
            .tool("echo", Arc::new(EchoOperator))
            .build();

        let input = OperatorInput::new(Content::text("go"), TriggerType::User);
        let ctx = DispatchContext::new(DispatchId::new("d-1"), OperatorId::new("skelegent.agent"));
        let _ = agent_op.handle(input, &ctx).await.expect("handle");
    }

    #[test]
    fn builder_accepts_custom_router() {
        let router = Router::new("custom.router", "Custom").route("echo", Arc::new(EchoOperator));
        let _ = agent(test_provider("x"), Completer).router(router);
    }

    #[test]
    fn builder_descriptor_reflects_metadata() {
        let agent_op = agent(test_provider("x"), Completer)
            .id("my.id")
            .name("MyName")
            .description("My description")
            .build();
        let desc = agent_op.descriptor();
        assert_eq!(desc.id.as_str(), "my.id");
        assert_eq!(desc.name, "MyName");
        assert_eq!(desc.description, "My description");
        assert_eq!(desc.kind, CapabilityKind::Agent);
    }

    #[tokio::test]
    async fn builder_accepts_custom_pipeline() {
        let factory: PipelineFactory = Arc::new(ReactivePipeline::new);
        let _agent = agent(test_provider("x"), Completer)
            .pipeline(factory)
            .build();
    }
}
