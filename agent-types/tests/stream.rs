use agent_types::*;

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
        StreamEvent::ToolUseStart { id: "t1".into(), name: "read_file".into() },
        StreamEvent::ToolUseInputDelta { id: "t1".into(), delta: r#"{"path""#.into() },
        StreamEvent::ToolUseInputDelta { id: "t1".into(), delta: r#": "/tmp"}"#.into() },
        StreamEvent::ToolUseEnd { id: "t1".into() },
    ];
    // Verify we can match on id for parallel tool call demux
    let t1_deltas: Vec<&str> = events.iter().filter_map(|e| match e {
        StreamEvent::ToolUseInputDelta { id, delta } if id == "t1" => Some(delta.as_str()),
        _ => None,
    }).collect();
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
