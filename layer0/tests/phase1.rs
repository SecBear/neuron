//! Phase 1 acceptance tests for the Layer 0 trait crate.
//!
//! Tests cover:
//! - Message type serialization round-trips
//! - Trait object safety (Box<dyn Trait> is Send + Sync)
//! - Blanket StateReader impl
//! - Typed ID conversions
//! - Content helper methods
//! - Custom variant round-trips

use layer0::*;
use rust_decimal::Decimal;
use serde_json::json;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Object Safety: Box<dyn Trait> compiles and is Send + Sync
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn _assert_send_sync<T: Send + Sync>() {}

#[test]
fn operator_is_object_safe_send_sync() {
    _assert_send_sync::<Box<dyn Operator>>();
}

#[test]
fn arc_operator_is_send_sync() {
    _assert_send_sync::<std::sync::Arc<dyn Operator>>();
}

#[test]
fn arc_orchestrator_is_send_sync() {
    _assert_send_sync::<std::sync::Arc<dyn Orchestrator>>();
}

#[test]
fn arc_state_store_is_send_sync() {
    _assert_send_sync::<std::sync::Arc<dyn StateStore>>();
}

#[test]
fn arc_state_reader_is_send_sync() {
    _assert_send_sync::<std::sync::Arc<dyn StateReader>>();
}

#[test]
fn arc_environment_is_send_sync() {
    _assert_send_sync::<std::sync::Arc<dyn Environment>>();
}

#[test]
fn arc_hook_is_send_sync() {
    _assert_send_sync::<std::sync::Arc<dyn Hook>>();
}

#[test]
fn orchestrator_is_object_safe_send_sync() {
    _assert_send_sync::<Box<dyn Orchestrator>>();
}

#[test]
fn state_store_is_object_safe_send_sync() {
    _assert_send_sync::<Box<dyn StateStore>>();
}

#[test]
fn state_reader_is_object_safe_send_sync() {
    _assert_send_sync::<Box<dyn StateReader>>();
}

#[test]
fn environment_is_object_safe_send_sync() {
    _assert_send_sync::<Box<dyn Environment>>();
}

#[test]
fn hook_is_object_safe_send_sync() {
    _assert_send_sync::<Box<dyn Hook>>();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Typed ID conversions
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn agent_id_from_str() {
    let id = AgentId::from("agent-1");
    assert_eq!(id.as_str(), "agent-1");
    assert_eq!(id.to_string(), "agent-1");
}

#[test]
fn session_id_from_string() {
    let id = SessionId::from(String::from("sess-abc"));
    assert_eq!(id.as_str(), "sess-abc");
}

#[test]
fn workflow_id_new() {
    let id = WorkflowId::new("wf-123");
    assert_eq!(id.0, "wf-123");
}

#[test]
fn scope_id_equality() {
    let a = ScopeId::new("scope-1");
    let b = ScopeId::new("scope-1");
    assert_eq!(a, b);
}

#[test]
fn typed_id_serde_round_trip() {
    let id = AgentId::new("test-agent");
    let json = serde_json::to_string(&id).unwrap();
    let back: AgentId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Content helpers and round-trips
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn content_text_helper() {
    let c = Content::text("hello");
    assert_eq!(c.as_text(), Some("hello"));
}

#[test]
fn content_blocks_as_text_returns_first_text() {
    let c = Content::Blocks(vec![
        ContentBlock::Text {
            text: "first".into(),
        },
        ContentBlock::Text {
            text: "second".into(),
        },
    ]);
    assert_eq!(c.as_text(), Some("first"));
}

#[test]
fn content_blocks_as_text_skips_non_text() {
    let c = Content::Blocks(vec![
        ContentBlock::ToolResult {
            tool_use_id: "id".into(),
            content: "result".into(),
            is_error: false,
        },
        ContentBlock::Text {
            text: "found".into(),
        },
    ]);
    assert_eq!(c.as_text(), Some("found"));
}

#[test]
fn content_text_serde_round_trip() {
    let c = Content::text("hello world");
    let json = serde_json::to_string(&c).unwrap();
    let back: Content = serde_json::from_str(&json).unwrap();
    assert_eq!(c, back);
}

#[test]
fn content_blocks_serde_round_trip() {
    let c = Content::Blocks(vec![
        ContentBlock::Text {
            text: "hello".into(),
        },
        ContentBlock::Image {
            source: layer0::content::ImageSource::Url {
                url: "https://example.com/img.png".into(),
            },
            media_type: "image/png".into(),
        },
        ContentBlock::ToolUse {
            id: "tu_1".into(),
            name: "read_file".into(),
            input: json!({"path": "/tmp/test"}),
        },
        ContentBlock::ToolResult {
            tool_use_id: "tu_1".into(),
            content: "file contents".into(),
            is_error: false,
        },
    ]);
    let json = serde_json::to_string(&c).unwrap();
    let back: Content = serde_json::from_str(&json).unwrap();
    assert_eq!(c, back);
}

#[test]
fn content_custom_block_round_trip() {
    let c = Content::Blocks(vec![ContentBlock::Custom {
        content_type: "audio".into(),
        data: json!({"codec": "opus", "samples": 48000}),
    }]);
    let json = serde_json::to_string(&c).unwrap();
    let back: Content = serde_json::from_str(&json).unwrap();
    assert_eq!(c, back);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// OperatorInput / OperatorOutput round-trips
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn sample_operator_input() -> OperatorInput {
    let mut config = OperatorConfig::default();
    config.max_turns = Some(10);
    config.max_cost = Some(Decimal::new(100, 2)); // $1.00
    config.max_duration = Some(DurationMs::from_secs(60));
    config.model = Some("claude-sonnet-4-20250514".into());
    config.allowed_tools = Some(vec!["read_file".into()]);
    config.system_addendum = Some("Be concise.".into());

    let mut input = OperatorInput::new(Content::text("do something"), layer0::operator::TriggerType::User);
    input.session = Some(SessionId::new("sess-1"));
    input.config = Some(config);
    input.metadata = json!({"trace_id": "abc123"});
    input
}

#[test]
fn operator_input_serde_round_trip() {
    let input = sample_operator_input();
    let json = serde_json::to_string(&input).unwrap();
    let back: OperatorInput = serde_json::from_str(&json).unwrap();
    assert_eq!(input.message, back.message);
    assert_eq!(input.trigger, back.trigger);
    assert_eq!(input.session, back.session);
    assert_eq!(input.metadata, back.metadata);
}

fn sample_operator_output() -> OperatorOutput {
    let mut meta = OperatorMetadata::default();
    meta.tokens_in = 100;
    meta.tokens_out = 50;
    meta.cost = Decimal::new(5, 3); // $0.005
    meta.turns_used = 1;
    meta.tools_called = vec![ToolCallRecord::new("read_file", DurationMs::from_millis(150), true)];
    meta.duration = DurationMs::from_secs(2);

    let mut output = OperatorOutput::new(Content::text("done"), ExitReason::Complete);
    output.metadata = meta;
    output
}

#[test]
fn operator_output_serde_round_trip() {
    let output = sample_operator_output();
    let json = serde_json::to_string(&output).unwrap();
    let back: OperatorOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(output.message, back.message);
    assert_eq!(output.exit_reason, back.exit_reason);
}

#[test]
fn operator_metadata_default() {
    let m = OperatorMetadata::default();
    assert_eq!(m.tokens_in, 0);
    assert_eq!(m.tokens_out, 0);
    assert_eq!(m.cost, Decimal::ZERO);
    assert_eq!(m.turns_used, 0);
    assert!(m.tools_called.is_empty());
    assert_eq!(m.duration, DurationMs::ZERO);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// TriggerType / ExitReason Custom variant round-trips
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn trigger_type_custom_round_trip() {
    let t = layer0::operator::TriggerType::Custom("webhook".into());
    let json = serde_json::to_string(&t).unwrap();
    let back: layer0::operator::TriggerType = serde_json::from_str(&json).unwrap();
    assert_eq!(t, back);
}

#[test]
fn exit_reason_custom_round_trip() {
    let e = ExitReason::Custom("user_cancelled".into());
    let json = serde_json::to_string(&e).unwrap();
    let back: ExitReason = serde_json::from_str(&json).unwrap();
    assert_eq!(e, back);
}

#[test]
fn exit_reason_observer_halt_round_trip() {
    let e = ExitReason::ObserverHalt {
        reason: "budget exceeded".into(),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: ExitReason = serde_json::from_str(&json).unwrap();
    assert_eq!(e, back);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Effect round-trips (including Custom)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn effect_write_memory_round_trip() {
    let e = Effect::WriteMemory {
        scope: Scope::Session(SessionId::new("s1")),
        key: "notes".into(),
        value: json!({"text": "remember this"}),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: Effect = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn effect_signal_round_trip() {
    let e = Effect::Signal {
        target: WorkflowId::new("wf-1"),
        payload: SignalPayload::new("user_feedback", json!({"rating": 5})),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: Effect = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn effect_delegate_round_trip() {
    let e = Effect::Delegate {
        agent: AgentId::new("helper"),
        input: Box::new(sample_operator_input()),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: Effect = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn effect_custom_round_trip() {
    let e = Effect::Custom {
        effect_type: "send_email".into(),
        data: json!({"to": "user@example.com", "subject": "done"}),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: Effect = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Scope round-trips (including Custom)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn scope_session_round_trip() {
    let s = Scope::Session(SessionId::new("s1"));
    let json = serde_json::to_string(&s).unwrap();
    let back: Scope = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn scope_agent_round_trip() {
    let s = Scope::Agent {
        workflow: WorkflowId::new("wf-1"),
        agent: AgentId::new("a-1"),
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: Scope = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn scope_global_round_trip() {
    let s = Scope::Global;
    let json = serde_json::to_string(&s).unwrap();
    let back: Scope = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn scope_custom_round_trip() {
    let s = Scope::Custom("tenant:acme".into());
    let json = serde_json::to_string(&s).unwrap();
    let back: Scope = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// IsolationBoundary Custom round-trip
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn isolation_boundary_custom_round_trip() {
    let b = layer0::environment::IsolationBoundary::Custom {
        boundary_type: "firecracker".into(),
        config: json!({"kernel": "vmlinux", "rootfs": "rootfs.ext4"}),
    };
    let json = serde_json::to_string(&b).unwrap();
    let back: layer0::environment::IsolationBoundary = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn isolation_boundary_all_variants_round_trip() {
    let variants: Vec<layer0::environment::IsolationBoundary> = vec![
        layer0::environment::IsolationBoundary::Process,
        layer0::environment::IsolationBoundary::Container {
            image: Some("ubuntu:24.04".into()),
        },
        layer0::environment::IsolationBoundary::Gvisor,
        layer0::environment::IsolationBoundary::MicroVm,
        layer0::environment::IsolationBoundary::Wasm {
            runtime: Some("wasmtime".into()),
        },
        layer0::environment::IsolationBoundary::NetworkPolicy {
            rules: vec![{
                let mut rule = layer0::environment::NetworkRule::new("10.0.0.0/8", layer0::environment::NetworkAction::Allow);
                rule.port = Some(443);
                rule
            }],
        },
    ];
    for v in variants {
        let json = serde_json::to_string(&v).unwrap();
        let back: layer0::environment::IsolationBoundary = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// EnvironmentSpec round-trip
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn environment_spec_round_trip() {
    let mut resources = layer0::environment::ResourceLimits::default();
    resources.cpu = Some("1.0".into());
    resources.memory = Some("2Gi".into());

    let mut api_rule = layer0::environment::NetworkRule::new(
        "api.anthropic.com",
        layer0::environment::NetworkAction::Allow,
    );
    api_rule.port = Some(443);

    let mut spec = EnvironmentSpec::default();
    spec.isolation = vec![layer0::environment::IsolationBoundary::Container {
        image: Some("python:3.12".into()),
    }];
    spec.credentials = vec![layer0::environment::CredentialRef::new(
        "api-key",
        layer0::environment::CredentialInjection::EnvVar {
            var_name: "API_KEY".into(),
        },
    )];
    spec.resources = Some(resources);
    spec.network = Some(layer0::environment::NetworkPolicy::new(
        layer0::environment::NetworkAction::Deny,
        vec![api_rule],
    ));
    let json = serde_json::to_string(&spec).unwrap();
    let back: EnvironmentSpec = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Hook types round-trips
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn hook_point_round_trip() {
    let points = vec![
        HookPoint::PreInference,
        HookPoint::PostInference,
        HookPoint::PreToolUse,
        HookPoint::PostToolUse,
        HookPoint::ExitCheck,
    ];
    for p in points {
        let json = serde_json::to_string(&p).unwrap();
        let back: HookPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Lifecycle event round-trips
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn budget_event_cost_incurred_round_trip() {
    let e = BudgetEvent::CostIncurred {
        agent: AgentId::new("a1"),
        cost: Decimal::new(5, 3),
        cumulative: Decimal::new(150, 3),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: BudgetEvent = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn compaction_event_round_trip() {
    let e = CompactionEvent::ContextPressure {
        agent: AgentId::new("a1"),
        fill_percent: 0.85,
        tokens_used: 85000,
        tokens_available: 15000,
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: CompactionEvent = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn observable_event_round_trip() {
    let mut e = ObservableEvent::new(
        layer0::lifecycle::EventSource::Turn,
        "turn.complete",
        DurationMs::from_millis(1500),
        json!({"tokens": 100}),
    );
    e.trace_id = Some("trace-abc".into());
    e.workflow_id = Some(WorkflowId::new("wf-1"));
    e.agent_id = Some(AgentId::new("a1"));
    let json = serde_json::to_string(&e).unwrap();
    let back: ObservableEvent = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Orchestrator QueryPayload round-trip
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn query_payload_round_trip() {
    let q = QueryPayload::new("status", json!({"verbose": true}));
    let json = serde_json::to_string(&q).unwrap();
    let back: QueryPayload = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// State SearchResult round-trip
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn search_result_round_trip() {
    let mut r = SearchResult::new("notes/meeting", 0.95);
    r.snippet = Some("discussed the architecture...".into());
    let json = serde_json::to_string(&r).unwrap();
    let back: SearchResult = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// LogLevel round-trip
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn log_level_round_trip() {
    let levels = vec![
        layer0::effect::LogLevel::Trace,
        layer0::effect::LogLevel::Debug,
        layer0::effect::LogLevel::Info,
        layer0::effect::LogLevel::Warn,
        layer0::effect::LogLevel::Error,
    ];
    for l in levels {
        let json = serde_json::to_string(&l).unwrap();
        let back: layer0::effect::LogLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(l, back);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Wire format stability: Decimal serializes as string
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn decimal_serializes_as_string_not_number() {
    // With rust_decimal's serde-str feature, Decimal serializes as "1.23"
    // not as 1.23 (number) or {"lo":...,"mid":...,...} (struct).
    // This is critical for wire-format stability across implementations.
    let cost = Decimal::new(123, 2); // 1.23
    let json = serde_json::to_value(&cost).unwrap();
    assert!(json.is_string(), "Decimal must serialize as a JSON string, got: {json}");
    assert_eq!(json.as_str().unwrap(), "1.23");
}

#[test]
fn decimal_zero_serializes_as_string() {
    let cost = Decimal::ZERO;
    let json = serde_json::to_value(&cost).unwrap();
    assert!(json.is_string(), "Decimal::ZERO must serialize as string, got: {json}");
    assert_eq!(json.as_str().unwrap(), "0");
}

#[test]
fn decimal_in_operator_metadata_wire_format() {
    // Verify Decimal format is preserved when nested in protocol types.
    let mut meta = OperatorMetadata::default();
    meta.tokens_in = 100;
    meta.tokens_out = 50;
    meta.cost = Decimal::new(5, 3); // 0.005
    meta.turns_used = 1;
    let json = serde_json::to_value(&meta).unwrap();
    let cost_val = &json["cost"];
    assert!(cost_val.is_string(), "cost in OperatorMetadata must be string, got: {cost_val}");
    assert_eq!(cost_val.as_str().unwrap(), "0.005");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Wire format stability: Content serialization
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn content_text_serializes_as_bare_string() {
    // Content::Text serializes as a bare JSON string (untagged).
    let c = Content::text("hello");
    let json = serde_json::to_value(&c).unwrap();
    assert!(json.is_string(), "Content::Text must serialize as bare string, got: {json}");
    assert_eq!(json.as_str().unwrap(), "hello");
}

#[test]
fn content_blocks_serializes_as_array() {
    // Content::Blocks serializes as a JSON array (untagged).
    let c = Content::Blocks(vec![ContentBlock::Text {
        text: "hello".into(),
    }]);
    let json = serde_json::to_value(&c).unwrap();
    assert!(json.is_array(), "Content::Blocks must serialize as array, got: {json}");
}

#[test]
fn content_text_and_blocks_are_structurally_distinct() {
    // The untagged Content enum is safe because String and Array
    // are structurally distinct JSON types. Verify round-trip of both
    // from the same test to prove no cross-contamination.
    let text = Content::text("hello");
    let blocks = Content::Blocks(vec![ContentBlock::Text {
        text: "hello".into(),
    }]);

    let text_json = serde_json::to_string(&text).unwrap();
    let blocks_json = serde_json::to_string(&blocks).unwrap();

    let text_back: Content = serde_json::from_str(&text_json).unwrap();
    let blocks_back: Content = serde_json::from_str(&blocks_json).unwrap();

    assert_eq!(text, text_back);
    assert_eq!(blocks, blocks_back);
    // Verify they don't cross-contaminate
    assert_ne!(text_json, blocks_json);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Wire format stability: DurationMs serializes as integer
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn duration_ms_serializes_as_integer() {
    let d = DurationMs::from_millis(1500);
    let json = serde_json::to_value(&d).unwrap();
    assert!(json.is_u64(), "DurationMs must serialize as integer, got: {json}");
    assert_eq!(json.as_u64().unwrap(), 1500);
}

#[test]
fn duration_ms_zero_serializes_as_zero() {
    let d = DurationMs::ZERO;
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json.as_u64().unwrap(), 0);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Forward compatibility: Custom variants accept unknown data
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn trigger_type_custom_preserves_unknown_variant() {
    // A new trigger type should survive round-trip through Custom.
    let json = r#"{"custom":"iot_sensor_event"}"#;
    let t: layer0::operator::TriggerType = serde_json::from_str(json).unwrap();
    assert_eq!(t, layer0::operator::TriggerType::Custom("iot_sensor_event".into()));
}

#[test]
fn exit_reason_custom_preserves_unknown_variant() {
    let json = r#"{"custom":"human_takeover"}"#;
    let e: ExitReason = serde_json::from_str(json).unwrap();
    assert_eq!(e, ExitReason::Custom("human_takeover".into()));
}

#[test]
fn scope_custom_preserves_unknown_scope() {
    let json = r#"{"custom":"tenant:acme-corp"}"#;
    let s: Scope = serde_json::from_str(json).unwrap();
    assert_eq!(s, Scope::Custom("tenant:acme-corp".into()));
}

#[test]
fn effect_custom_preserves_unknown_effect() {
    let json = r##"{"type":"custom","effect_type":"send_slack","data":{"channel":"#ops"}}"##;
    let e: Effect = serde_json::from_str(json).unwrap();
    let reserialized = serde_json::to_string(&e).unwrap();
    let reparsed: serde_json::Value = serde_json::from_str(&reserialized).unwrap();
    assert_eq!(reparsed["type"], "custom");
    assert_eq!(reparsed["effect_type"], "send_slack");
}

#[test]
fn content_block_custom_preserves_unknown_modality() {
    let json = r#"{"type":"custom","content_type":"audio","data":{"codec":"opus","sample_rate":48000}}"#;
    let b: ContentBlock = serde_json::from_str(json).unwrap();
    let reserialized = serde_json::to_string(&b).unwrap();
    let reparsed: serde_json::Value = serde_json::from_str(&reserialized).unwrap();
    assert_eq!(reparsed["type"], "custom");
    assert_eq!(reparsed["content_type"], "audio");
    assert_eq!(reparsed["data"]["codec"], "opus");
}

#[test]
fn isolation_boundary_custom_preserves_unknown_isolation() {
    let json = r#"{"type":"custom","boundary_type":"kata_container","config":{"runtime":"qemu"}}"#;
    let b: layer0::environment::IsolationBoundary = serde_json::from_str(json).unwrap();
    let reserialized = serde_json::to_string(&b).unwrap();
    let reparsed: serde_json::Value = serde_json::from_str(&reserialized).unwrap();
    assert_eq!(reparsed["type"], "custom");
    assert_eq!(reparsed["boundary_type"], "kata_container");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Blanket StateReader impl
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

// Verify that any concrete StateStore type implements StateReader
// via the blanket impl (trait object upcasting is not stable,
// so we test with generics)
fn _takes_state_reader<T: StateReader + ?Sized>(_r: &T) {}
fn _takes_state_store<T: StateStore>(s: &T) {
    // This compiles because of the blanket impl: T: StateStore => T: StateReader
    _takes_state_reader(s);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Error types display
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn operator_error_display() {
    let e = OperatorError::Model("rate limited".into());
    assert_eq!(e.to_string(), "model error: rate limited");

    let e = OperatorError::Tool {
        tool: "bash".into(),
        message: "command failed".into(),
    };
    assert_eq!(e.to_string(), "tool error in bash: command failed");
}

#[test]
fn orch_error_display() {
    let e = OrchError::AgentNotFound("missing-agent".into());
    assert_eq!(e.to_string(), "agent not found: missing-agent");
}

#[test]
fn state_error_display() {
    let e = StateError::NotFound {
        scope: "session".into(),
        key: "notes".into(),
    };
    assert_eq!(e.to_string(), "not found: session/notes");
}

#[test]
fn env_error_display() {
    let e = EnvError::ProvisionFailed("docker not available".into());
    assert_eq!(e.to_string(), "provisioning failed: docker not available");
}

#[test]
fn hook_error_display() {
    let e = HookError::Failed("timeout".into());
    assert_eq!(e.to_string(), "hook failed: timeout");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Error Display — remaining variants
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn operator_error_display_remaining_variants() {
    assert_eq!(
        OperatorError::ContextAssembly("bad ctx".into()).to_string(),
        "context assembly failed: bad ctx"
    );
    assert_eq!(
        OperatorError::Retryable("timeout".into()).to_string(),
        "retryable: timeout"
    );
    assert_eq!(
        OperatorError::NonRetryable("invalid".into()).to_string(),
        "non-retryable: invalid"
    );
    let boxed: Box<dyn std::error::Error + Send + Sync> = "inner error".into();
    assert_eq!(OperatorError::Other(boxed).to_string(), "inner error");
}

#[test]
fn orch_error_display_remaining_variants() {
    assert_eq!(
        OrchError::WorkflowNotFound("wf-1".into()).to_string(),
        "workflow not found: wf-1"
    );
    assert_eq!(
        OrchError::DispatchFailed("timeout".into()).to_string(),
        "dispatch failed: timeout"
    );
    assert_eq!(
        OrchError::SignalFailed("no handler".into()).to_string(),
        "signal delivery failed: no handler"
    );
    let inner = OperatorError::Model("provider down".into());
    assert_eq!(
        OrchError::OperatorError(inner).to_string(),
        "operator error: model error: provider down"
    );
    let boxed: Box<dyn std::error::Error + Send + Sync> = "orch inner".into();
    assert_eq!(OrchError::Other(boxed).to_string(), "orch inner");
}

#[test]
fn state_error_display_remaining_variants() {
    assert_eq!(
        StateError::WriteFailed("disk full".into()).to_string(),
        "write failed: disk full"
    );
    assert_eq!(
        StateError::Serialization("invalid json".into()).to_string(),
        "serialization error: invalid json"
    );
    let boxed: Box<dyn std::error::Error + Send + Sync> = "state inner".into();
    assert_eq!(StateError::Other(boxed).to_string(), "state inner");
}

#[test]
fn env_error_display_remaining_variants() {
    assert_eq!(
        EnvError::IsolationViolation("escaped sandbox".into()).to_string(),
        "isolation violation: escaped sandbox"
    );
    assert_eq!(
        EnvError::CredentialFailed("secret not found".into()).to_string(),
        "credential injection failed: secret not found"
    );
    assert_eq!(
        EnvError::ResourceExceeded("OOM".into()).to_string(),
        "resource limit exceeded: OOM"
    );
    let inner = OperatorError::Model("provider down".into());
    assert_eq!(
        EnvError::OperatorError(inner).to_string(),
        "operator error: model error: provider down"
    );
    let boxed: Box<dyn std::error::Error + Send + Sync> = "env inner".into();
    assert_eq!(EnvError::Other(boxed).to_string(), "env inner");
}

#[test]
fn hook_error_display_other_variant() {
    let boxed: Box<dyn std::error::Error + Send + Sync> = "hook inner".into();
    assert_eq!(HookError::Other(boxed).to_string(), "hook inner");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// BudgetDecision round-trips
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn budget_decision_downgrade_round_trip() {
    let d = layer0::lifecycle::BudgetDecision::DowngradeModel {
        from: "claude-opus-4-20250514".into(),
        to: "claude-haiku-4-5-20251001".into(),
    };
    let json = serde_json::to_string(&d).unwrap();
    let back: layer0::lifecycle::BudgetDecision = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Effect::Log and Effect::Handoff round-trips
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn effect_log_round_trip() {
    let e = Effect::Log {
        level: layer0::effect::LogLevel::Info,
        message: "turn completed".into(),
        data: Some(json!({"duration_ms": 1500})),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: Effect = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn effect_handoff_round_trip() {
    let e = Effect::Handoff {
        agent: AgentId::new("specialist"),
        state: json!({"context": "user needs help with billing"}),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: Effect = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn effect_delete_memory_round_trip() {
    let e = Effect::DeleteMemory {
        scope: Scope::Global,
        key: "temp_notes".into(),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: Effect = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// OperatorConfig default
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn operator_config_default_all_none() {
    let c = OperatorConfig::default();
    assert!(c.max_turns.is_none());
    assert!(c.max_cost.is_none());
    assert!(c.max_duration.is_none());
    assert!(c.model.is_none());
    assert!(c.allowed_tools.is_none());
    assert!(c.system_addendum.is_none());
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CredentialInjection variants round-trip
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn credential_injection_variants_round_trip() {
    let variants: Vec<layer0::environment::CredentialInjection> = vec![
        layer0::environment::CredentialInjection::EnvVar {
            var_name: "API_KEY".into(),
        },
        layer0::environment::CredentialInjection::File {
            path: "/run/secrets/key".into(),
        },
        layer0::environment::CredentialInjection::Sidecar,
    ];
    for v in variants {
        let json = serde_json::to_string(&v).unwrap();
        let back: layer0::environment::CredentialInjection =
            serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// EventSource round-trip
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn event_source_round_trip() {
    let sources = vec![
        layer0::lifecycle::EventSource::Turn,
        layer0::lifecycle::EventSource::Orchestration,
        layer0::lifecycle::EventSource::State,
        layer0::lifecycle::EventSource::Environment,
        layer0::lifecycle::EventSource::Hook,
    ];
    for s in sources {
        let json = serde_json::to_string(&s).unwrap();
        let back: layer0::lifecycle::EventSource = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Compaction event variants
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn compaction_pre_flush_round_trip() {
    let e = CompactionEvent::PreCompactionFlush {
        agent: AgentId::new("a1"),
        scope: Scope::Session(SessionId::new("s1")),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: CompactionEvent = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn compaction_complete_round_trip() {
    let e = CompactionEvent::CompactionComplete {
        agent: AgentId::new("a1"),
        strategy: "sliding_window".into(),
        tokens_freed: 50000,
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: CompactionEvent = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

#[test]
fn compaction_provider_managed_round_trip() {
    let e = CompactionEvent::ProviderManaged {
        agent: AgentId::new("a1"),
        provider: "anthropic".into(),
        tokens_before: 100000,
        tokens_after: 50000,
        summary: Some(Content::text("Summarized prior conversation")),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: CompactionEvent = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&back).unwrap();
    assert_eq!(json, json2);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// HookAction serde round-trips
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn hook_action_variants_round_trip() {
    let actions = vec![
        HookAction::Continue,
        HookAction::Halt {
            reason: "policy violation".into(),
        },
        HookAction::SkipTool {
            reason: "not allowed".into(),
        },
        HookAction::ModifyToolInput {
            new_input: json!({"key": "modified"}),
        },
    ];
    for action in actions {
        let json = serde_json::to_string(&action).unwrap();
        let back: HookAction = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// BudgetEvent/BudgetDecision remaining variants
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn budget_event_all_variants_round_trip() {
    let events: Vec<BudgetEvent> = vec![
        BudgetEvent::BudgetWarning {
            workflow: WorkflowId::new("wf-1"),
            spent: Decimal::new(800, 2),
            limit: Decimal::new(1000, 2),
        },
        BudgetEvent::BudgetAction {
            workflow: WorkflowId::new("wf-1"),
            action: layer0::lifecycle::BudgetDecision::Continue,
        },
        BudgetEvent::BudgetAction {
            workflow: WorkflowId::new("wf-1"),
            action: layer0::lifecycle::BudgetDecision::HaltWorkflow,
        },
        BudgetEvent::BudgetAction {
            workflow: WorkflowId::new("wf-1"),
            action: layer0::lifecycle::BudgetDecision::RequestIncrease {
                amount: Decimal::new(500, 2),
            },
        },
    ];
    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let back: BudgetEvent = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ExitReason all variants round-trip
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn exit_reason_all_variants_round_trip() {
    let reasons = vec![
        ExitReason::Complete,
        ExitReason::MaxTurns,
        ExitReason::BudgetExhausted,
        ExitReason::CircuitBreaker,
        ExitReason::Timeout,
        ExitReason::ObserverHalt {
            reason: "safety".into(),
        },
        ExitReason::Error,
        ExitReason::Custom("special".into()),
    ];
    for reason in &reasons {
        let json = serde_json::to_string(reason).unwrap();
        let back: ExitReason = serde_json::from_str(&json).unwrap();
        assert_eq!(*reason, back);
    }
}
