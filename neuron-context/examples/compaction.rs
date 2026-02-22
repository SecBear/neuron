//! Example: create a conversation, check token counts, and compact with SlidingWindowStrategy.
//!
//! Run with: `cargo run --example compaction -p neuron-context`

use neuron_context::{SlidingWindowStrategy, TokenCounter};
use neuron_types::{ContentBlock, ContextStrategy, Message, Role};

fn user_msg(text: &str) -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

fn assistant_msg(text: &str) -> Message {
    Message {
        role: Role::Assistant,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

fn system_msg(text: &str) -> Message {
    Message {
        role: Role::System,
        content: vec![ContentBlock::Text(text.to_string())],
    }
}

#[tokio::main]
async fn main() {
    // 1. Create a TokenCounter to estimate token usage
    let counter = TokenCounter::new();

    // 2. Build a conversation with several messages
    let messages = vec![
        system_msg("You are a helpful coding assistant."),
        user_msg("Can you explain what a HashMap is in Rust?"),
        assistant_msg(
            "A HashMap in Rust is a collection that stores key-value pairs. \
             It uses a hashing algorithm to map keys to their associated values, \
             providing O(1) average-case lookup, insertion, and deletion.",
        ),
        user_msg("How do I iterate over a HashMap?"),
        assistant_msg(
            "You can iterate over a HashMap using a for loop: \
             `for (key, value) in &map { ... }`. You can also use `.keys()`, \
             `.values()`, or `.iter()` for more specific iteration patterns.",
        ),
        user_msg("What about BTreeMap? When should I use it instead?"),
        assistant_msg(
            "Use BTreeMap when you need keys in sorted order. BTreeMap provides \
             O(log n) operations but maintains ordering. HashMap is faster for \
             unsorted access patterns.",
        ),
        user_msg("Can you show me an example of BTreeMap with custom ordering?"),
        assistant_msg(
            "Sure! You can implement the Ord trait on your key type, or use \
             a wrapper type with a custom Ord implementation. BTreeMap will \
             then automatically sort entries according to your ordering.",
        ),
        user_msg("Thanks! One more question about Vec vs VecDeque."),
        assistant_msg(
            "Vec is a growable array optimized for push/pop at the end. \
             VecDeque is a double-ended queue that supports efficient push/pop \
             at both ends, using a ring buffer internally.",
        ),
    ];

    // 3. Estimate tokens in the full conversation
    let total_tokens = counter.estimate_messages(&messages);
    println!("Conversation has {} messages", messages.len());
    println!("Estimated token count: {total_tokens}");

    // 4. Create a SlidingWindowStrategy
    //    - window_size: keep the last 4 non-system messages
    //    - max_tokens: trigger compaction above 100 tokens
    let strategy = SlidingWindowStrategy::new(4, 100);

    // 5. Check if we should compact
    let should = strategy.should_compact(&messages, total_tokens);
    println!("\nShould compact (threshold=100, current={total_tokens}): {should}");

    if should {
        // 6. Compact and show before/after
        let compacted = strategy
            .compact(messages.clone())
            .await
            .expect("compaction should succeed");

        let compacted_tokens = counter.estimate_messages(&compacted);

        println!("\nBefore compaction:");
        println!("  Messages: {}", messages.len());
        println!("  Tokens:   {total_tokens}");

        println!("\nAfter compaction:");
        println!("  Messages: {}", compacted.len());
        println!("  Tokens:   {compacted_tokens}");

        println!("\nRetained messages:");
        for msg in &compacted {
            let role = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
            };
            let text = msg
                .content
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text(t) = b {
                        Some(t.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");
            // Truncate long messages for display
            let display = if text.len() > 60 {
                format!("{}...", &text[..60])
            } else {
                text
            };
            println!("  [{role}] {display}");
        }
    } else {
        println!("No compaction needed.");
    }
}
