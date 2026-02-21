//! Basic usage of the OpenAI provider.
//!
//! Set OPENAI_API_KEY in your environment and run:
//!   cargo run --example basic

use agent_provider_openai::OpenAi;
use agent_types::{CompletionRequest, ContentBlock, Message, Provider, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key =
        std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY environment variable must be set");

    let provider = OpenAi::new(api_key);

    let request = CompletionRequest {
        model: String::new(), // use default (gpt-4o)
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Say hello in one sentence.".into())],
        }],
        system: None,
        tools: vec![],
        max_tokens: Some(128),
        temperature: None,
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    };

    let response = provider.complete(request).await?;
    println!("Response: {:?}", response.message.content);
    println!(
        "Tokens: {} in / {} out",
        response.usage.input_tokens, response.usage.output_tokens
    );

    Ok(())
}
