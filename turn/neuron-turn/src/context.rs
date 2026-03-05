//! Context strategy for managing the conversation window.
//!
//! The [`ContextStrategy`] trait handles client-side context compaction.
//! Provider-native truncation (e.g., OpenAI `truncation: auto`) is
//! invisible to the strategy — handled by the Provider impl internally.

use crate::types::ProviderMessage;
use serde::{Deserialize, Serialize};

/// Error from a context compaction operation.
#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    /// Transient failure that may succeed on retry (e.g., API timeout during summarization).
    #[error("transient compaction error: {0}")]
    Transient(String),
    /// Semantic failure that will not succeed on retry (e.g., bad summary quality).
    #[error("semantic compaction error: {0}")]
    Semantic(String),
}

/// A provider message with optional compaction and source metadata.
///
/// All metadata fields are optional. An unannotated `ProviderMessage` behaves
/// exactly as today when wrapped via `AnnotatedMessage::from(msg)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotatedMessage {
    /// The underlying provider message.
    pub message: ProviderMessage,
    /// Compaction policy for this message. Default: `Normal`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<layer0::CompactionPolicy>,
    /// Source of this message (e.g. `"mcp:github"`, `"user"`, `"tool:shell"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Write-time importance hint (0.0–1.0). Does not replace `SearchResult.score`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salience: Option<f64>,
}

impl From<ProviderMessage> for AnnotatedMessage {
    fn from(message: ProviderMessage) -> Self {
        Self {
            message,
            policy: None,
            source: None,
            salience: None,
        }
    }
}

impl AnnotatedMessage {
    /// Create a pinned message that survives all compaction.
    pub fn pinned(message: ProviderMessage) -> Self {
        Self {
            message,
            policy: Some(layer0::CompactionPolicy::Pinned),
            source: None,
            salience: None,
        }
    }

    /// Create a message tagged as originating from an MCP tool.
    pub fn from_mcp(message: ProviderMessage, server_name: impl Into<String>) -> Self {
        Self {
            message,
            policy: Some(layer0::CompactionPolicy::DiscardWhenDone),
            source: Some(format!("mcp:{}", server_name.into())),
            salience: None,
        }
    }
}

/// Strategy for managing context window size.
///
/// Implementations: `NoCompaction` (passthrough), `SlidingWindow`
/// (drop oldest messages), `Summarization` (future).
pub trait ContextStrategy: Send + Sync {
    /// Estimate token count for a message list.
    fn token_estimate(&self, messages: &[AnnotatedMessage]) -> usize;

    /// Whether compaction should run given the current messages and limit.
    fn should_compact(&self, messages: &[AnnotatedMessage], limit: usize) -> bool;

    /// Compact the message list. Returns a shorter list, or an error.
    fn compact(
        &self,
        messages: Vec<AnnotatedMessage>,
    ) -> Result<Vec<AnnotatedMessage>, CompactionError>;
}

/// A no-op context strategy that never compacts.
///
/// Useful for short conversations or when the provider handles
/// truncation natively.
pub struct NoCompaction;

impl ContextStrategy for NoCompaction {
    fn token_estimate(&self, messages: &[AnnotatedMessage]) -> usize {
        // Rough estimate: 4 chars per token
        messages
            .iter()
            .flat_map(|m| &m.message.content)
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

    fn should_compact(&self, _messages: &[AnnotatedMessage], _limit: usize) -> bool {
        false
    }

    fn compact(
        &self,
        messages: Vec<AnnotatedMessage>,
    ) -> Result<Vec<AnnotatedMessage>, CompactionError> {
        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ContentPart, Role};

    fn make_msg(role: Role, text: &str) -> AnnotatedMessage {
        AnnotatedMessage::from(ProviderMessage {
            role,
            content: vec![ContentPart::Text {
                text: text.to_string(),
            }],
        })
    }

    #[test]
    fn no_compaction_never_compacts() {
        let strategy = NoCompaction;
        let messages = vec![make_msg(Role::User, "hello")];

        assert!(!strategy.should_compact(&messages, 100));
        let compacted = strategy.compact(messages.clone()).unwrap();
        assert_eq!(compacted.len(), messages.len());
    }

    #[test]
    fn no_compaction_estimates_tokens() {
        let strategy = NoCompaction;
        let messages = vec![make_msg(Role::User, &"a".repeat(400))];

        let estimate = strategy.token_estimate(&messages);
        assert_eq!(estimate, 100); // 400 chars / 4
    }

    #[test]
    fn no_compaction_preserves_all_messages() {
        let strategy = NoCompaction;
        let messages = vec![
            make_msg(Role::User, "msg1"),
            make_msg(Role::Assistant, "msg2"),
            make_msg(Role::User, "msg3"),
        ];

        let compacted = strategy.compact(messages.clone()).unwrap();
        assert_eq!(compacted.len(), 3);
        assert_eq!(compacted[0].message.content, messages[0].message.content);
        assert_eq!(compacted[1].message.content, messages[1].message.content);
        assert_eq!(compacted[2].message.content, messages[2].message.content);
    }

    #[test]
    fn no_compaction_estimates_tool_use_tokens() {
        let strategy = NoCompaction;
        let messages = vec![AnnotatedMessage::from(ProviderMessage {
            role: Role::Assistant,
            content: vec![ContentPart::ToolUse {
                id: "tu_1".into(),
                name: "bash".into(),
                input: serde_json::json!({"command": "ls"}),
            }],
        })];

        let estimate = strategy.token_estimate(&messages);
        // The JSON representation of the input will be tokenized
        assert!(estimate > 0);
    }

    #[test]
    fn no_compaction_estimates_tool_result_tokens() {
        let strategy = NoCompaction;
        let messages = vec![AnnotatedMessage::from(ProviderMessage {
            role: Role::User,
            content: vec![ContentPart::ToolResult {
                tool_use_id: "tu_1".into(),
                content: "a".repeat(200),
                is_error: false,
            }],
        })];

        let estimate = strategy.token_estimate(&messages);
        assert_eq!(estimate, 50); // 200 chars / 4
    }

    #[test]
    fn no_compaction_estimates_image_tokens() {
        let strategy = NoCompaction;
        let messages = vec![AnnotatedMessage::from(ProviderMessage {
            role: Role::User,
            content: vec![ContentPart::Image {
                source: crate::types::ImageSource::Url {
                    url: "https://example.com/img.png".into(),
                },
                media_type: "image/png".into(),
            }],
        })];

        let estimate = strategy.token_estimate(&messages);
        assert_eq!(estimate, 1000); // rough image estimate
    }

    #[test]
    fn context_strategy_is_object_safe() {
        fn _assert_object_safe(_: &dyn ContextStrategy) {}
        let nc = NoCompaction;
        _assert_object_safe(&nc);
    }

    #[test]
    fn annotated_message_from_provider_message() {
        let msg = ProviderMessage {
            role: Role::User,
            content: vec![ContentPart::Text { text: "hi".into() }],
        };
        let am = AnnotatedMessage::from(msg.clone());
        assert_eq!(am.message.role, Role::User);
        assert!(am.policy.is_none());
    }

    #[test]
    fn compaction_policy_round_trip() {
        use layer0::CompactionPolicy;
        for v in [
            CompactionPolicy::Pinned,
            CompactionPolicy::Normal,
            CompactionPolicy::CompressFirst,
            CompactionPolicy::DiscardWhenDone,
        ] {
            let json = serde_json::to_string(&v).unwrap();
            let back: CompactionPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }
}
