use neuron_types::*;

#[test]
fn stream_event_text_delta() {
    let event = StreamEvent::TextDelta("hello".into());
    match event {
        StreamEvent::TextDelta(s) => assert_eq!(s, "hello"),
        _ => panic!("expected TextDelta"),
    }
}

#[test]
fn stream_event_tool_use_demux() {
    let events = vec![
        StreamEvent::ToolUseStart {
            id: "t1".into(),
            name: "read_file".into(),
        },
        StreamEvent::ToolUseInputDelta {
            id: "t1".into(),
            delta: r#"{"path""#.into(),
        },
        StreamEvent::ToolUseInputDelta {
            id: "t1".into(),
            delta: r#": "/tmp"}"#.into(),
        },
        StreamEvent::ToolUseEnd { id: "t1".into() },
    ];
    // Verify we can match on id for parallel tool call demux
    let t1_deltas: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::ToolUseInputDelta { id, delta } if id == "t1" => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(t1_deltas.join(""), r#"{"path": "/tmp"}"#);
}

#[test]
fn stream_event_message_complete() {
    let msg = Message {
        role: Role::Assistant,
        content: vec![ContentBlock::Text("done".into())],
    };
    let event = StreamEvent::MessageComplete(msg);
    if let StreamEvent::MessageComplete(m) = event {
        assert_eq!(m.role, Role::Assistant);
    } else {
        panic!("expected MessageComplete");
    }
}

#[test]
fn stream_event_error_with_stream_error() {
    let error = StreamError {
        message: "connection reset".to_string(),
        is_retryable: true,
    };
    let event = StreamEvent::Error(error);
    if let StreamEvent::Error(e) = event {
        assert_eq!(e.message, "connection reset");
        assert!(e.is_retryable);
    } else {
        panic!("expected Error variant");
    }
}

#[test]
fn stream_error_non_retryable() {
    let error = StreamError {
        message: "invalid API key".to_string(),
        is_retryable: false,
    };
    assert_eq!(error.message, "invalid API key");
    assert!(!error.is_retryable);
}

#[test]
fn stream_error_clone() {
    let error = StreamError {
        message: "timeout".to_string(),
        is_retryable: true,
    };
    let cloned = error.clone();
    assert_eq!(cloned.message, error.message);
    assert_eq!(cloned.is_retryable, error.is_retryable);
}

// --- StreamError convenience constructors ---

#[test]
fn stream_error_non_retryable_constructor() {
    let err = StreamError::non_retryable("auth failure");
    assert_eq!(err.message, "auth failure");
    assert!(!err.is_retryable);
}

#[test]
fn stream_error_non_retryable_constructor_with_string() {
    let err = StreamError::non_retryable(String::from("auth failure"));
    assert_eq!(err.message, "auth failure");
    assert!(!err.is_retryable);
}

#[test]
fn stream_error_retryable_constructor() {
    let err = StreamError::retryable("rate limited");
    assert_eq!(err.message, "rate limited");
    assert!(err.is_retryable);
}

#[test]
fn stream_error_retryable_constructor_with_string() {
    let err = StreamError::retryable(String::from("connection timeout"));
    assert_eq!(err.message, "connection timeout");
    assert!(err.is_retryable);
}

// --- StreamError Display impl ---

#[test]
fn stream_error_display() {
    let err = StreamError {
        message: "unexpected EOF".to_string(),
        is_retryable: false,
    };
    // Display should show the message
    assert_eq!(format!("{err}"), "unexpected EOF");
}

#[test]
fn stream_error_display_retryable() {
    let err = StreamError::retryable("server overloaded");
    assert_eq!(format!("{err}"), "server overloaded");
}

// --- StreamEvent::ThinkingDelta ---

#[test]
fn stream_event_thinking_delta() {
    let event = StreamEvent::ThinkingDelta("Let me reason about this...".into());
    if let StreamEvent::ThinkingDelta(text) = event {
        assert_eq!(text, "Let me reason about this...");
    } else {
        panic!("expected ThinkingDelta");
    }
}

// --- StreamEvent::SignatureDelta ---

#[test]
fn stream_event_signature_delta() {
    let event = StreamEvent::SignatureDelta("abc123signature".into());
    if let StreamEvent::SignatureDelta(sig) = event {
        assert_eq!(sig, "abc123signature");
    } else {
        panic!("expected SignatureDelta");
    }
}

// --- StreamEvent::Usage ---

#[test]
fn stream_event_usage() {
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        ..Default::default()
    };
    let event = StreamEvent::Usage(usage);
    if let StreamEvent::Usage(u) = event {
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 50);
    } else {
        panic!("expected Usage");
    }
}

// --- StreamHandle Debug impl ---

#[test]
fn stream_handle_debug() {
    let (_, rx) = futures::channel::mpsc::channel::<StreamEvent>(1);
    let handle = StreamHandle {
        receiver: Box::pin(rx),
    };
    let debug = format!("{handle:?}");
    assert!(debug.contains("StreamHandle"));
}

// --- StreamEvent Clone ---

#[test]
fn stream_event_clone() {
    let event = StreamEvent::TextDelta("hello".into());
    let cloned = event.clone();
    if let StreamEvent::TextDelta(text) = cloned {
        assert_eq!(text, "hello");
    } else {
        panic!("expected TextDelta");
    }
}

// --- Multiple tool use streams interleaved ---

#[test]
fn stream_event_multiple_tool_use_interleaved() {
    let events = vec![
        StreamEvent::ToolUseStart {
            id: "t1".into(),
            name: "search".into(),
        },
        StreamEvent::ToolUseStart {
            id: "t2".into(),
            name: "read_file".into(),
        },
        StreamEvent::ToolUseInputDelta {
            id: "t1".into(),
            delta: r#"{"query":"#.into(),
        },
        StreamEvent::ToolUseInputDelta {
            id: "t2".into(),
            delta: r#"{"path":"#.into(),
        },
        StreamEvent::ToolUseInputDelta {
            id: "t1".into(),
            delta: r#""rust"}"#.into(),
        },
        StreamEvent::ToolUseInputDelta {
            id: "t2".into(),
            delta: r#""/tmp"}"#.into(),
        },
        StreamEvent::ToolUseEnd { id: "t1".into() },
        StreamEvent::ToolUseEnd { id: "t2".into() },
    ];

    let t1_input: String = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::ToolUseInputDelta { id, delta } if id == "t1" => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    let t2_input: String = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::ToolUseInputDelta { id, delta } if id == "t2" => Some(delta.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(t1_input, r#"{"query":"rust"}"#);
    assert_eq!(t2_input, r#"{"path":"/tmp"}"#);
}
