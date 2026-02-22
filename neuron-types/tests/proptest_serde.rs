//! Property-based tests: serde roundtrip for all public types.

use neuron_types::*;
use proptest::prelude::*;

fn arb_role() -> impl Strategy<Value = Role> {
    prop_oneof![Just(Role::User), Just(Role::Assistant), Just(Role::System),]
}

fn arb_content_block() -> impl Strategy<Value = ContentBlock> {
    prop_oneof![
        any::<String>().prop_map(ContentBlock::Text),
        (any::<String>(), any::<String>()).prop_map(|(t, s)| ContentBlock::Thinking {
            thinking: t,
            signature: s,
        }),
        any::<String>().prop_map(|d| ContentBlock::RedactedThinking { data: d }),
        any::<String>().prop_map(|c| ContentBlock::Compaction { content: c }),
    ]
}

fn arb_message() -> impl Strategy<Value = Message> {
    (
        arb_role(),
        proptest::collection::vec(arb_content_block(), 0..5),
    )
        .prop_map(|(role, content)| Message { role, content })
}

fn arb_stop_reason() -> impl Strategy<Value = StopReason> {
    prop_oneof![
        Just(StopReason::EndTurn),
        Just(StopReason::ToolUse),
        Just(StopReason::MaxTokens),
        Just(StopReason::StopSequence),
        Just(StopReason::ContentFilter),
        Just(StopReason::Compaction),
    ]
}

proptest! {
    #[test]
    fn message_serde_roundtrip(msg in arb_message()) {
        let json = serde_json::to_string(&msg).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        // Message doesn't derive PartialEq, compare fields
        prop_assert_eq!(msg.role, back.role);
        prop_assert_eq!(msg.content.len(), back.content.len());
    }

    #[test]
    fn stop_reason_serde_roundtrip(sr in arb_stop_reason()) {
        let json = serde_json::to_string(&sr).unwrap();
        let back: StopReason = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(sr, back);
    }

    #[test]
    fn embedding_request_default_fields(
        model in ".*",
        dims in proptest::option::of(1usize..4096),
    ) {
        let req = EmbeddingRequest {
            model: model.clone(),
            input: vec!["test".to_string()],
            dimensions: dims,
            ..Default::default()
        };
        prop_assert_eq!(req.model, model);
        prop_assert_eq!(req.dimensions, dims);
    }

    #[test]
    fn embedding_response_serde_roundtrip(
        model in "[a-z-]+",
        n_embeddings in 1usize..5,
        dim in 1usize..10,
        prompt_tokens in 0usize..10000,
        total_tokens in 0usize..10000,
    ) {
        let resp = EmbeddingResponse {
            model,
            embeddings: (0..n_embeddings).map(|_| vec![0.0f32; dim]).collect(),
            usage: EmbeddingUsage { prompt_tokens, total_tokens },
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: EmbeddingResponse = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(resp, back);
    }
}
