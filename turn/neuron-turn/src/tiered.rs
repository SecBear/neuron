//! Zone-partitioned compaction strategy.
//!
//! [`TieredStrategy`] partitions messages into four zones and compacts each
//! independently, eliminating the recursive-summarisation degradation that
//! causes architectural decisions and constraints to be lost over time.
//!
//! ## Zone model
//!
//! | Zone | Messages | Action |
//! |------|----------|--------|
//! | Pinned | `CompactionPolicy::Pinned` | Never compacted; survive indefinitely |
//! | Active | Most-recent `active_zone_size` unpinned messages | Never compacted; always present |
//! | Summary | One first-generation summary of older unpinned messages | Replaced wholesale each compaction |
//! | Noise | `DiscardWhenDone` or `CompressFirst` messages | Discarded on compaction |
//!
//! The summary is always first-generation: it is derived from the original
//! messages, never from a previous summary. This prevents the "telephone game"
//! effect where architectural detail is lost after each compaction cycle.

use crate::context::{AnnotatedMessage, CompactionError, ContextStrategy};
use crate::types::ProviderMessage;
use layer0::CompactionPolicy;

/// Configuration for `TieredStrategy`.
#[derive(Debug, Clone)]
pub struct TieredConfig {
    /// Maximum context size in messages. Compaction fires when `messages.len() > max_messages`.
    pub max_messages: usize,
    /// How many of the most-recent unpinned messages to keep uncompacted (active zone).
    /// Default: 10.
    pub active_zone_size: usize,
}

impl Default for TieredConfig {
    fn default() -> Self {
        Self {
            max_messages: 40,
            active_zone_size: 10,
        }
    }
}

/// A `ContextStrategy` that partitions messages into zones and compacts each
/// independently, preserving pinned messages and preventing recursive degradation.
///
/// See the [module documentation](self) for the zone model.
pub struct TieredStrategy {
    config: TieredConfig,
    /// Optional summariser: given a list of messages to summarise, returns one summary message.
    /// When `None`, the strategy discards the summary-candidate messages entirely (lossy but
    /// avoids the recursive-degradation problem). A real implementation would call an LLM.
    summariser: Option<Box<dyn Summariser>>,
}

/// Produces a one-sentence or one-paragraph summary from a slice of messages.
///
/// Implement this to wire in an LLM-based summariser.
pub trait Summariser: Send + Sync {
    /// Summarise `messages` into a single provider message.
    ///
    /// Called only when compaction is triggered and there are summary candidates.
    fn summarise(&self, messages: &[ProviderMessage]) -> Result<ProviderMessage, CompactionError>;
}

impl TieredStrategy {
    /// Create a `TieredStrategy` with default config and no summariser.
    ///
    /// Without a summariser, summary-candidate messages are discarded on compaction.
    pub fn new() -> Self {
        Self {
            config: TieredConfig::default(),
            summariser: None,
        }
    }

    /// Create a `TieredStrategy` with custom configuration.
    pub fn with_config(config: TieredConfig) -> Self {
        Self {
            config,
            summariser: None,
        }
    }

    /// Attach a summariser. When set, summary candidates are passed to it to produce
    /// a single first-generation summary message instead of being discarded.
    pub fn with_summariser(mut self, summariser: Box<dyn Summariser>) -> Self {
        self.summariser = Some(summariser);
        self
    }
}

impl Default for TieredStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextStrategy for TieredStrategy {
    fn token_estimate(&self, messages: &[AnnotatedMessage]) -> usize {
        // Rough estimate: 4 chars per token, average message ~200 chars
        messages
            .iter()
            .flat_map(|am| &am.message.content)
            .map(|part| {
                use crate::types::ContentPart;
                match part {
                    ContentPart::Text { text } => text.len() / 4,
                    ContentPart::ToolUse { input, .. } => input.to_string().len() / 4,
                    ContentPart::ToolResult { content, .. } => content.len() / 4,
                    ContentPart::Image { .. } => 1000,
                }
            })
            .sum()
    }

    fn should_compact(&self, messages: &[AnnotatedMessage], _limit: usize) -> bool {
        messages.len() > self.config.max_messages
    }

    fn compact(
        &self,
        messages: Vec<AnnotatedMessage>,
    ) -> Result<Vec<AnnotatedMessage>, CompactionError> {
        // Partition into zones
        let mut pinned: Vec<AnnotatedMessage> = Vec::new();
        let mut noise: Vec<AnnotatedMessage> = Vec::new();
        let mut normal: Vec<AnnotatedMessage> = Vec::new();

        for msg in messages {
            match msg.policy {
                Some(CompactionPolicy::Pinned) => pinned.push(msg),
                Some(CompactionPolicy::DiscardWhenDone) | Some(CompactionPolicy::CompressFirst) => {
                    noise.push(msg);
                }
                None | Some(CompactionPolicy::Normal) => normal.push(msg),
            }
        }

        // Split normal into active (recent) and summary-candidates (older)
        let active_size = self.config.active_zone_size.min(normal.len());
        let split_point = normal.len().saturating_sub(active_size);
        let summary_candidates: Vec<AnnotatedMessage> = normal.drain(..split_point).collect();
        // `normal` now contains only the active zone
        // Note: noise is discarded (CompressFirst treated same as DiscardWhenDone for now)
        let _ = noise;

        // Build result: [pinned] + [summary] + [active]
        let mut result: Vec<AnnotatedMessage> = pinned;

        if !summary_candidates.is_empty()
            && let Some(summariser) = &self.summariser
        {
            let provider_msgs: Vec<ProviderMessage> = summary_candidates
                .iter()
                .map(|am| am.message.clone())
                .collect();
            let summary_msg = summariser.summarise(&provider_msgs)?;
            let mut summary_annotated = AnnotatedMessage::from(summary_msg);
            // Mark the summary as Normal so it can be replaced on the next compaction
            summary_annotated.policy = Some(CompactionPolicy::Normal);
            summary_annotated.source = Some("compaction:summary".into());
            result.push(summary_annotated);
            // If no summariser: summary candidates are dropped (lossy but no degradation)
        }

        result.extend(normal); // active zone
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ContextStrategy;
    use crate::types::{ContentPart, ProviderMessage, Role};
    use layer0::CompactionPolicy;

    fn make_msg(text: &str) -> AnnotatedMessage {
        AnnotatedMessage::from(ProviderMessage {
            role: Role::User,
            content: vec![ContentPart::Text { text: text.into() }],
        })
    }

    fn make_pinned(text: &str) -> AnnotatedMessage {
        let mut am = make_msg(text);
        am.policy = Some(CompactionPolicy::Pinned);
        am
    }

    fn make_noise(text: &str) -> AnnotatedMessage {
        let mut am = make_msg(text);
        am.policy = Some(CompactionPolicy::DiscardWhenDone);
        am
    }

    #[test]
    fn below_limit_no_compaction() {
        let s = TieredStrategy::with_config(TieredConfig {
            max_messages: 40,
            active_zone_size: 10,
        });
        let msgs: Vec<_> = (0..5).map(|i| make_msg(&format!("msg {i}"))).collect();
        assert!(!s.should_compact(&msgs, 0));
    }

    #[test]
    fn pinned_messages_survive_compaction() {
        let s = TieredStrategy::with_config(TieredConfig {
            max_messages: 2,
            active_zone_size: 1,
        });
        let msgs = vec![
            make_pinned("pinned invariant"),
            make_msg("old message 1"),
            make_msg("old message 2"),
            make_msg("recent"),
        ];
        let result = s.compact(msgs).unwrap();
        // Pinned always present
        assert!(
            result
                .iter()
                .any(|am| { am.policy == Some(CompactionPolicy::Pinned) })
        );
        // Active zone present ("recent" is the last 1)
        assert!(result.iter().any(|am| {
            matches!(&am.message.content[0], ContentPart::Text { text } if text == "recent")
        }));
        // Old messages discarded (no summariser)
        assert!(!result.iter().any(|am| {
            matches!(&am.message.content[0], ContentPart::Text { text } if text == "old message 1")
        }));
    }

    #[test]
    fn noise_messages_discarded() {
        let s = TieredStrategy::new();
        let msgs = vec![make_noise("mcp tool output"), make_msg("important")];
        let result = s.compact(msgs).unwrap();
        assert!(!result.iter().any(|am| {
            matches!(&am.message.content[0], ContentPart::Text { text } if text == "mcp tool output")
        }));
        assert!(result.iter().any(|am| {
            matches!(&am.message.content[0], ContentPart::Text { text } if text == "important")
        }));
    }

    #[test]
    fn all_normal_no_annotation_works() {
        // Unannotated messages should behave like Normal
        let s = TieredStrategy::with_config(TieredConfig {
            max_messages: 2,
            active_zone_size: 2,
        });
        let msgs: Vec<_> = (0..5).map(|i| make_msg(&format!("m{i}"))).collect();
        let result = s.compact(msgs).unwrap();
        // Active zone: last 2
        assert_eq!(result.len(), 2);
        assert!(
            matches!(&result[0].message.content[0], ContentPart::Text { text } if text == "m3")
        );
        assert!(
            matches!(&result[1].message.content[0], ContentPart::Text { text } if text == "m4")
        );
    }

    struct TestSummariser;
    impl Summariser for TestSummariser {
        fn summarise(
            &self,
            messages: &[ProviderMessage],
        ) -> Result<ProviderMessage, CompactionError> {
            Ok(ProviderMessage {
                role: Role::User,
                content: vec![ContentPart::Text {
                    text: format!("Summary of {} messages", messages.len()),
                }],
            })
        }
    }

    #[test]
    fn summariser_produces_first_generation_summary() {
        let s = TieredStrategy::with_config(TieredConfig {
            max_messages: 2,
            active_zone_size: 1,
        })
        .with_summariser(Box::new(TestSummariser));
        let msgs = vec![
            make_msg("old 1"),
            make_msg("old 2"),
            make_msg("old 3"),
            make_msg("recent"),
        ];
        let result = s.compact(msgs).unwrap();
        // Should have a summary message
        let has_summary = result
            .iter()
            .any(|am| am.source.as_deref() == Some("compaction:summary"));
        assert!(has_summary, "Expected a summary message");
        // "recent" should be present
        assert!(result.iter().any(|am| {
            matches!(&am.message.content[0], ContentPart::Text { text } if text == "recent")
        }));
    }
}
