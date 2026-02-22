//! Property-based tests: error classification consistency.

use neuron_types::*;
use proptest::prelude::*;
use std::time::Duration;

fn arb_provider_error() -> impl Strategy<Value = ProviderError> {
    prop_oneof![
        any::<String>().prop_map(ProviderError::Authentication),
        proptest::option::of(0u64..3600).prop_map(|secs| ProviderError::RateLimit {
            retry_after: secs.map(Duration::from_secs),
        }),
        any::<String>().prop_map(ProviderError::InvalidRequest),
        any::<String>().prop_map(ProviderError::ModelNotFound),
        any::<String>().prop_map(ProviderError::ServiceUnavailable),
        any::<String>().prop_map(ProviderError::ModelLoading),
        any::<String>().prop_map(ProviderError::InsufficientResources),
        any::<String>().prop_map(ProviderError::StreamError),
    ]
}

fn arb_embedding_error() -> impl Strategy<Value = EmbeddingError> {
    prop_oneof![
        any::<String>().prop_map(EmbeddingError::Authentication),
        proptest::option::of(0u64..3600).prop_map(|secs| EmbeddingError::RateLimit {
            retry_after: secs.map(Duration::from_secs),
        }),
        any::<String>().prop_map(EmbeddingError::InvalidRequest),
    ]
}

proptest! {
    #[test]
    fn provider_error_retryable_classification(err in arb_provider_error()) {
        let retryable = err.is_retryable();
        match &err {
            ProviderError::RateLimit { .. } => prop_assert!(retryable),
            ProviderError::ServiceUnavailable(_) => prop_assert!(retryable),
            ProviderError::Authentication(_) => prop_assert!(!retryable),
            ProviderError::InvalidRequest(_) => prop_assert!(!retryable),
            ProviderError::ModelNotFound(_) => prop_assert!(!retryable),
            _ => {} // Other variants — just check it doesn't panic
        }
    }

    #[test]
    fn embedding_error_retryable_classification(err in arb_embedding_error()) {
        let retryable = err.is_retryable();
        match &err {
            EmbeddingError::RateLimit { .. } => prop_assert!(retryable),
            EmbeddingError::Authentication(_) => prop_assert!(!retryable),
            EmbeddingError::InvalidRequest(_) => prop_assert!(!retryable),
            _ => {}
        }
    }
}
