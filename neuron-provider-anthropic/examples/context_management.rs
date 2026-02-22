//! Server-side context management with Anthropic.
//!
//! Enables Anthropic's server-side compaction so the provider automatically
//! manages context window size. When compaction occurs, the response includes
//! `StopReason::Compaction` and per-iteration token usage.
//!
//! Set ANTHROPIC_API_KEY in your environment and run:
//!   cargo run --example context_management -p neuron-provider-anthropic

use neuron_provider_anthropic::Anthropic;
use neuron_types::{
    CompletionRequest, ContentBlock, ContextEdit, ContextManagement, Message, Provider, Role,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY environment variable must be set");

    let provider = Anthropic::new(api_key);

    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text(
                "Summarize the history of computing in detail.".into(),
            )],
        }],
        max_tokens: Some(4096),
        context_management: Some(ContextManagement {
            edits: vec![ContextEdit::Compact {
                strategy: "compact_20260112".into(),
            }],
        }),
        ..Default::default()
    };

    let response = provider.complete(request).await?;

    println!("Stop reason: {:?}", response.stop_reason);
    println!(
        "Tokens: {} in / {} out",
        response.usage.input_tokens, response.usage.output_tokens
    );

    if let Some(iterations) = &response.usage.iterations {
        for (i, iter) in iterations.iter().enumerate() {
            println!(
                "  Iteration {}: {} in / {} out",
                i, iter.input_tokens, iter.output_tokens
            );
        }
    }

    for block in &response.message.content {
        match block {
            ContentBlock::Text(text) => println!("\n{text}"),
            ContentBlock::Compaction { content } => {
                println!("\n[Compaction summary: {}...]", &content[..content.len().min(100)])
            }
            _ => {}
        }
    }

    Ok(())
}
