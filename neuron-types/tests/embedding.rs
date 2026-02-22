use neuron_types::{EmbeddingRequest, EmbeddingResponse, EmbeddingUsage};

#[test]
fn embedding_request_default() {
    let req = EmbeddingRequest::default();
    assert!(req.model.is_empty());
    assert!(req.input.is_empty());
    assert_eq!(req.dimensions, None);
    assert!(req.extra.is_empty());
}

#[test]
fn embedding_response_serde_roundtrip() {
    let original = EmbeddingResponse {
        model: "text-embedding-3-small".to_string(),
        embeddings: vec![vec![0.1, 0.2, 0.3], vec![0.4, 0.5, 0.6]],
        usage: EmbeddingUsage {
            prompt_tokens: 10,
            total_tokens: 10,
        },
    };

    let json = serde_json::to_string(&original).expect("serialize");
    let deserialized: EmbeddingResponse = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(original, deserialized);
    assert_eq!(deserialized.model, "text-embedding-3-small");
    assert_eq!(deserialized.embeddings.len(), 2);
    assert_eq!(deserialized.embeddings[0], vec![0.1, 0.2, 0.3]);
    assert_eq!(deserialized.usage.prompt_tokens, 10);
    assert_eq!(deserialized.usage.total_tokens, 10);
}

#[test]
fn embedding_usage_default() {
    let usage = EmbeddingUsage::default();
    assert_eq!(usage.prompt_tokens, 0);
    assert_eq!(usage.total_tokens, 0);
}
