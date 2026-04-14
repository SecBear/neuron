use async_trait::async_trait;
use layer0::operator::TriggerType;
use layer0::{
    ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityFilter, CapabilityId, CapabilityKind,
    CapabilitySource, Content, DispatchContext, DispatchHandle, DispatchId, ExecutionClass,
    Operator, OperatorId, OperatorInput, Outcome, ProtocolError, SchedulingFacts, TerminalOutcome,
};

fn noop_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new(
        CapabilityId::new("test.noop"),
        CapabilityKind::Tool,
        "noop",
        "Returns ok",
        SchedulingFacts::new(ExecutionClass::Shared, false, false, false, None),
        ApprovalFacts::None,
        AuthFacts::Open,
    )
}

struct NoopOperator;

#[async_trait]
impl Operator for NoopOperator {
    fn descriptor(&self) -> CapabilityDescriptor {
        noop_descriptor()
    }

    async fn handle(
        &self,
        _input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<DispatchHandle, ProtocolError> {
        use layer0::{OperatorOutput, completed_handle};
        let output = OperatorOutput::new(
            Content::text("ok"),
            Outcome::Terminal {
                terminal: TerminalOutcome::Completed,
            },
        );
        Ok(completed_handle(ctx.dispatch_id.clone(), output))
    }
}

struct DiscoverOnly;

#[async_trait]
impl CapabilitySource for DiscoverOnly {
    async fn list(
        &self,
        _filter: CapabilityFilter,
    ) -> Result<Vec<CapabilityDescriptor>, ProtocolError> {
        Ok(Vec::new())
    }

    async fn get(&self, _id: &CapabilityId) -> Result<Option<CapabilityDescriptor>, ProtocolError> {
        Ok(None)
    }
}

#[test]
fn operator_and_capability_source_are_distinct_traits() {
    fn accepts_operator(_: &dyn Operator) {}
    fn accepts_source(_: &dyn CapabilitySource) {}

    let op = NoopOperator;
    let source = DiscoverOnly;

    accepts_operator(&op);
    accepts_source(&source);
}

#[tokio::test]
async fn operator_handle_returns_completed_output() {
    let op = NoopOperator;
    let ctx = DispatchContext::new(DispatchId::new("dispatch-1"), OperatorId::new("noop"));
    let input = OperatorInput::new(Content::text("{}"), TriggerType::Task);
    let output = op
        .handle(input, &ctx)
        .await
        .expect("handle")
        .collect()
        .await
        .expect("completed");
    assert_eq!(
        output.outcome,
        Outcome::Terminal {
            terminal: TerminalOutcome::Completed
        }
    );
}
