//! Token count estimation from messages and tool definitions.

use agent_types::{ContentBlock, ContentItem, Message, ToolDefinition};

/// Estimates token counts from text using a configurable chars-per-token ratio.
///
/// This is a heuristic estimator — real tokenization varies per model. The
/// default ratio of 4.0 chars/token approximates GPT-family and Claude models.
///
/// # Example
///
/// ```
/// use agent_context::TokenCounter;
///
/// let counter = TokenCounter::new();
/// let estimate = counter.estimate_text("Hello, world!");
/// assert!(estimate > 0);
/// ```
pub struct TokenCounter {
    chars_per_token: f32,
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter {
    /// Creates a new `TokenCounter` with the default ratio of 4.0 chars/token.
    pub fn new() -> Self {
        Self { chars_per_token: 4.0 }
    }

    /// Creates a new `TokenCounter` with a custom chars-per-token ratio.
    pub fn with_ratio(chars_per_token: f32) -> Self {
        Self { chars_per_token }
    }

    /// Estimates the number of tokens in a text string.
    pub fn estimate_text(&self, text: &str) -> usize {
        (text.len() as f32 / self.chars_per_token).ceil() as usize
    }

    /// Estimates the total token count for a slice of messages.
    ///
    /// Iterates all content blocks and sums their estimated token counts.
    pub fn estimate_messages(&self, messages: &[Message]) -> usize {
        messages.iter().map(|m| self.estimate_message(m)).sum()
    }

    /// Estimates the total token count for a slice of tool definitions.
    pub fn estimate_tools(&self, tools: &[ToolDefinition]) -> usize {
        tools
            .iter()
            .map(|t| {
                let name_tokens = self.estimate_text(&t.name);
                let desc_tokens = self.estimate_text(&t.description);
                let schema_str = t.input_schema.to_string();
                let schema_tokens = self.estimate_text(&schema_str);
                name_tokens + desc_tokens + schema_tokens
            })
            .sum()
    }

    fn estimate_message(&self, message: &Message) -> usize {
        // Add a small overhead per message for role markers / formatting
        let role_overhead = 4;
        let content_tokens: usize = message
            .content
            .iter()
            .map(|block| self.estimate_content_block(block))
            .sum();
        role_overhead + content_tokens
    }

    fn estimate_content_block(&self, block: &ContentBlock) -> usize {
        match block {
            ContentBlock::Text(text) => self.estimate_text(text),
            ContentBlock::Thinking { thinking, .. } => self.estimate_text(thinking),
            ContentBlock::RedactedThinking { data } => self.estimate_text(data),
            ContentBlock::ToolUse { name, input, .. } => {
                let name_tokens = self.estimate_text(name);
                let input_str = input.to_string();
                let input_tokens = self.estimate_text(&input_str);
                name_tokens + input_tokens
            }
            ContentBlock::ToolResult { content, .. } => {
                content.iter().map(|item| self.estimate_content_item(item)).sum()
            }
            ContentBlock::Image { .. } => {
                // Images are expensive; use a fixed estimate
                300
            }
            ContentBlock::Document { .. } => {
                // Documents are expensive; use a fixed estimate
                500
            }
        }
    }

    fn estimate_content_item(&self, item: &ContentItem) -> usize {
        match item {
            ContentItem::Text(text) => self.estimate_text(text),
            ContentItem::Image { .. } => 300,
        }
    }
}
