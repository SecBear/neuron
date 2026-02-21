//! Integration tests for agent-runtime.

use std::path::PathBuf;

use agent_runtime::{FileSessionStorage, InMemorySessionStorage, Session, SessionStorage};
use agent_types::{ContentBlock, Message, Role, TokenUsage};

// ============================================================================
// Task 9.2 tests: Session and SessionState types
// ============================================================================

#[test]
fn test_session_creation() {
    let session = Session::new("test-session", PathBuf::from("/tmp"));
    assert_eq!(session.id, "test-session");
    assert!(session.messages.is_empty());
    assert_eq!(session.state.cwd, PathBuf::from("/tmp"));
    assert_eq!(session.state.event_count, 0);
    assert!(session.state.custom.is_empty());
}

#[test]
fn test_session_timestamps() {
    let before = chrono::Utc::now();
    let session = Session::new("ts-test", PathBuf::from("/tmp"));
    let after = chrono::Utc::now();

    assert!(session.created_at >= before);
    assert!(session.created_at <= after);
    assert!(session.updated_at >= before);
    assert!(session.updated_at <= after);
}

#[test]
fn test_session_summary() {
    let mut session = Session::new("summary-test", PathBuf::from("/tmp"));
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello".to_string())],
    });

    let summary = session.summary();
    assert_eq!(summary.id, "summary-test");
    assert_eq!(summary.message_count, 1);
    assert_eq!(summary.created_at, session.created_at);
    assert_eq!(summary.updated_at, session.updated_at);
}

#[test]
fn test_session_serialize_deserialize() {
    let mut session = Session::new("serde-test", PathBuf::from("/home/user"));
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hi".to_string())],
    });
    session.state.token_usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        ..Default::default()
    };
    session.state.event_count = 3;
    session
        .state
        .custom
        .insert("key".to_string(), serde_json::json!("value"));

    let json = serde_json::to_string(&session).expect("serialize");
    let deserialized: Session = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.id, "serde-test");
    assert_eq!(deserialized.messages.len(), 1);
    assert_eq!(deserialized.state.token_usage.input_tokens, 100);
    assert_eq!(deserialized.state.token_usage.output_tokens, 50);
    assert_eq!(deserialized.state.event_count, 3);
    assert_eq!(
        deserialized.state.custom.get("key"),
        Some(&serde_json::json!("value"))
    );
}

// ============================================================================
// Task 9.3 tests: InMemorySessionStorage
// ============================================================================

#[tokio::test]
async fn test_in_memory_save_and_load() {
    let storage = InMemorySessionStorage::new();
    let session = Session::new("mem-1", PathBuf::from("/tmp"));

    storage.save(&session).await.expect("save should succeed");
    let loaded = storage.load("mem-1").await.expect("load should succeed");

    assert_eq!(loaded.id, "mem-1");
    assert_eq!(loaded.state.cwd, PathBuf::from("/tmp"));
}

#[tokio::test]
async fn test_in_memory_load_not_found() {
    let storage = InMemorySessionStorage::new();
    let err = storage.load("nonexistent").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_in_memory_list() {
    let storage = InMemorySessionStorage::new();
    storage
        .save(&Session::new("list-1", PathBuf::from("/a")))
        .await
        .expect("save");
    storage
        .save(&Session::new("list-2", PathBuf::from("/b")))
        .await
        .expect("save");

    let summaries = storage.list().await.expect("list should succeed");
    assert_eq!(summaries.len(), 2);

    let ids: Vec<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"list-1"));
    assert!(ids.contains(&"list-2"));
}

#[tokio::test]
async fn test_in_memory_delete() {
    let storage = InMemorySessionStorage::new();
    storage
        .save(&Session::new("del-1", PathBuf::from("/tmp")))
        .await
        .expect("save");

    storage.delete("del-1").await.expect("delete should succeed");

    let err = storage.load("del-1").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_in_memory_delete_not_found() {
    let storage = InMemorySessionStorage::new();
    let err = storage.delete("nope").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_in_memory_save_overwrites() {
    let storage = InMemorySessionStorage::new();
    let mut session = Session::new("overwrite", PathBuf::from("/tmp"));
    storage.save(&session).await.expect("save");

    session.state.event_count = 42;
    storage.save(&session).await.expect("save again");

    let loaded = storage.load("overwrite").await.expect("load");
    assert_eq!(loaded.state.event_count, 42);
}

// ============================================================================
// Task 9.4 tests: FileSessionStorage
// ============================================================================

#[tokio::test]
async fn test_file_save_and_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    let mut session = Session::new("file-1", PathBuf::from("/work"));
    session.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text("Hello file".to_string())],
    });

    storage.save(&session).await.expect("save should succeed");

    // Verify JSON file exists
    let file_path = dir.path().join("file-1.json");
    assert!(file_path.exists());

    let loaded = storage.load("file-1").await.expect("load should succeed");
    assert_eq!(loaded.id, "file-1");
    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(loaded.state.cwd, PathBuf::from("/work"));
}

#[tokio::test]
async fn test_file_load_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    let err = storage.load("nonexistent").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_file_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    storage
        .save(&Session::new("flist-1", PathBuf::from("/a")))
        .await
        .expect("save");
    storage
        .save(&Session::new("flist-2", PathBuf::from("/b")))
        .await
        .expect("save");

    let summaries = storage.list().await.expect("list");
    assert_eq!(summaries.len(), 2);

    let ids: Vec<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"flist-1"));
    assert!(ids.contains(&"flist-2"));
}

#[tokio::test]
async fn test_file_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    storage
        .save(&Session::new("fdel-1", PathBuf::from("/tmp")))
        .await
        .expect("save");
    storage.delete("fdel-1").await.expect("delete");

    let err = storage.load("fdel-1").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
    assert!(!dir.path().join("fdel-1.json").exists());
}

#[tokio::test]
async fn test_file_delete_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = FileSessionStorage::new(dir.path().to_path_buf());

    let err = storage.delete("nope").await.unwrap_err();
    assert!(matches!(err, agent_types::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_file_creates_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("nested").join("sessions");
    let storage = FileSessionStorage::new(nested.clone());

    storage
        .save(&Session::new("nested-1", PathBuf::from("/tmp")))
        .await
        .expect("save should create nested dirs");

    assert!(nested.join("nested-1.json").exists());
}

#[tokio::test]
async fn test_file_list_empty_nonexistent_dir() {
    let storage = FileSessionStorage::new(PathBuf::from("/tmp/nonexistent_agent_runtime_test"));
    let summaries = storage.list().await.expect("list should succeed");
    assert!(summaries.is_empty());
}
