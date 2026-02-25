//! Context strategy for managing the conversation window.
//!
//! The [`ContextStrategy`] trait handles client-side context compaction.
//! Provider-native truncation (e.g., OpenAI `truncation: auto`) is
//! invisible to the strategy — handled by the Provider impl internally.

use crate::types::ProviderMessage;

/// Strategy for managing context window size.
///
/// Implementations: `NoCompaction` (passthrough), `SlidingWindow`
/// (drop oldest messages), `Summarization` (future).
pub trait ContextStrategy: Send + Sync {
    /// Estimate token count for a message list.
    fn token_estimate(&self, messages: &[ProviderMessage]) -> usize;

    /// Whether compaction should run given the current messages and limit.
    fn should_compact(&self, messages: &[ProviderMessage], limit: usize) -> bool;

    /// Compact the message list. Returns a shorter list.
    fn compact(&self, messages: Vec<ProviderMessage>) -> Vec<ProviderMessage>;
}

/// A no-op context strategy that never compacts.
///
/// Useful for short conversations or when the provider handles
/// truncation natively.
pub struct NoCompaction;

impl ContextStrategy for NoCompaction {
    fn token_estimate(&self, messages: &[ProviderMessage]) -> usize {
        // Rough estimate: 4 chars per token
        messages
            .iter()
            .flat_map(|m| &m.content)
            .map(|part| {
                use crate::types::ContentPart;
                match part {
                    ContentPart::Text { text } => text.len() / 4,
                    ContentPart::ToolUse { input, .. } => input.to_string().len() / 4,
                    ContentPart::ToolResult { content, .. } => content.len() / 4,
                    ContentPart::Image { .. } => 1000, // rough image token estimate
                }
            })
            .sum()
    }

    fn should_compact(&self, _messages: &[ProviderMessage], _limit: usize) -> bool {
        false
    }

    fn compact(&self, messages: Vec<ProviderMessage>) -> Vec<ProviderMessage> {
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ContentPart, Role};

    #[test]
    fn no_compaction_never_compacts() {
        let strategy = NoCompaction;
        let messages = vec![ProviderMessage {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: "hello".into(),
            }],
        }];

        assert!(!strategy.should_compact(&messages, 100));
        let compacted = strategy.compact(messages.clone());
        assert_eq!(compacted.len(), messages.len());
    }

    #[test]
    fn no_compaction_estimates_tokens() {
        let strategy = NoCompaction;
        let messages = vec![ProviderMessage {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: "a".repeat(400),
            }],
        }];

        let estimate = strategy.token_estimate(&messages);
        assert_eq!(estimate, 100); // 400 chars / 4
    }
}
