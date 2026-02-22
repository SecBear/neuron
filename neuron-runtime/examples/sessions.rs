//! Demonstrate session management with FileSessionStorage.
//!
//! Creates a temporary directory, saves a session with messages,
//! loads it back, lists all sessions, and prints the results.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example sessions -p neuron-runtime
//! ```

use std::path::PathBuf;

use neuron_runtime::{FileSessionStorage, Session, SessionStorage};
use neuron_types::{ContentBlock, Message, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create a FileSessionStorage in a temporary directory.
    //    Using a fixed path under /tmp; in production you would use a
    //    persistent location.
    let storage_dir = PathBuf::from("/tmp/neuron-sessions-example");
    let storage = FileSessionStorage::new(storage_dir.clone());
    println!("Session storage directory: {}", storage_dir.display());

    // 2. Create a new session.
    let mut session = Session::new("demo-session-1", PathBuf::from("/tmp"));
    println!("\nCreated session: {}", session.id);

    // 3. Add some messages to the session.
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text(
            "What is the weather like today?".to_string(),
        )],
    });
    session.messages.push(Message {
        role: Role::Assistant,
        content: vec![ContentBlock::Text(
            "I don't have access to real-time weather data, but I can help you find it!".to_string(),
        )],
    });
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Thanks!".to_string())],
    });
    println!("Added {} messages", session.messages.len());

    // 4. Save the session to disk.
    storage.save(&session).await?;
    println!("Session saved to disk.");

    // 5. Load it back by ID.
    let loaded = storage.load("demo-session-1").await?;
    println!(
        "\nLoaded session '{}' with {} message(s)",
        loaded.id,
        loaded.messages.len()
    );
    for (i, msg) in loaded.messages.iter().enumerate() {
        let preview: String = msg
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        let truncated = if preview.len() > 60 {
            format!("{}...", &preview[..60])
        } else {
            preview
        };
        println!("  [{}] {:?}: {}", i, msg.role, truncated);
    }

    // Save a second session so listing shows multiple results.
    let session2 = Session::new("demo-session-2", PathBuf::from("/tmp"));
    storage.save(&session2).await?;

    // 6. List all sessions.
    let summaries = storage.list().await?;
    println!("\nAll sessions ({}):", summaries.len());
    for summary in &summaries {
        println!(
            "  - id={}, messages={}, created={}",
            summary.id, summary.message_count, summary.created_at
        );
    }

    // Clean up: delete the sessions we created.
    storage.delete("demo-session-1").await?;
    storage.delete("demo-session-2").await?;
    println!("\nCleaned up example sessions.");

    Ok(())
}
