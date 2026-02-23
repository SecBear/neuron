//! Integration tests for PersistentContext and ContextSection.

use neuron_context::{ContextSection, PersistentContext};

#[test]
fn renders_sections_in_priority_order() {
    let mut ctx = PersistentContext::new();
    // Add in reverse priority order — should render lowest number first
    ctx.add_section(ContextSection {
        label: "Rules".to_string(),
        content: "Be concise.".to_string(),
        priority: 10,
    });
    ctx.add_section(ContextSection {
        label: "Role".to_string(),
        content: "You are helpful.".to_string(),
        priority: 0,
    });
    ctx.add_section(ContextSection {
        label: "Reminder".to_string(),
        content: "Stay on topic.".to_string(),
        priority: 5,
    });

    let rendered = ctx.render();

    // Role (priority 0) should appear before Reminder (5) before Rules (10)
    let role_pos = rendered
        .find("Role")
        .expect("Role section should be present");
    let reminder_pos = rendered
        .find("Reminder")
        .expect("Reminder section should be present");
    let rules_pos = rendered
        .find("Rules")
        .expect("Rules section should be present");

    assert!(role_pos < reminder_pos, "Role should come before Reminder");
    assert!(
        reminder_pos < rules_pos,
        "Reminder should come before Rules"
    );
}

#[test]
fn rendered_output_contains_all_content() {
    let mut ctx = PersistentContext::new();
    ctx.add_section(ContextSection {
        label: "Identity".to_string(),
        content: "You are a code assistant.".to_string(),
        priority: 0,
    });
    ctx.add_section(ContextSection {
        label: "Constraints".to_string(),
        content: "Never run destructive commands.".to_string(),
        priority: 1,
    });

    let rendered = ctx.render();
    assert!(rendered.contains("You are a code assistant."));
    assert!(rendered.contains("Never run destructive commands."));
    assert!(rendered.contains("## Identity"));
    assert!(rendered.contains("## Constraints"));
}

#[test]
fn empty_context_renders_empty_string() {
    let ctx = PersistentContext::new();
    assert_eq!(ctx.render(), "");
}

#[test]
fn single_section_renders_correctly() {
    let mut ctx = PersistentContext::new();
    ctx.add_section(ContextSection {
        label: "Solo".to_string(),
        content: "Only section.".to_string(),
        priority: 0,
    });
    let rendered = ctx.render();
    assert_eq!(rendered, "## Solo\nOnly section.");
}

#[test]
fn equal_priority_sections_are_all_present() {
    let mut ctx = PersistentContext::new();
    ctx.add_section(ContextSection {
        label: "A".to_string(),
        content: "Content A".to_string(),
        priority: 1,
    });
    ctx.add_section(ContextSection {
        label: "B".to_string(),
        content: "Content B".to_string(),
        priority: 1,
    });

    let rendered = ctx.render();
    assert!(rendered.contains("Content A"));
    assert!(rendered.contains("Content B"));
}

// ---- Additional coverage tests ----

#[test]
fn duplicate_labels_both_present() {
    let mut ctx = PersistentContext::new();
    ctx.add_section(ContextSection {
        label: "Rules".to_string(),
        content: "First rules set.".to_string(),
        priority: 0,
    });
    ctx.add_section(ContextSection {
        label: "Rules".to_string(),
        content: "Second rules set.".to_string(),
        priority: 1,
    });

    let rendered = ctx.render();
    // Both sections with the same label should be present
    assert!(rendered.contains("First rules set."));
    assert!(rendered.contains("Second rules set."));
    // Should contain "## Rules" twice
    assert_eq!(rendered.matches("## Rules").count(), 2);
}

#[test]
fn many_sections_ordering() {
    let mut ctx = PersistentContext::new();
    for i in (0..10).rev() {
        ctx.add_section(ContextSection {
            label: format!("Section {i}"),
            content: format!("Content of section {i}"),
            priority: i,
        });
    }

    let rendered = ctx.render();
    // Verify each section appears in priority order (0 first, 9 last)
    let mut last_pos = 0;
    for i in 0..10 {
        let label = format!("Section {i}");
        let pos = rendered
            .find(&label)
            .expect(&format!("{label} should exist"));
        assert!(
            pos >= last_pos,
            "Section {i} at position {pos} should be after position {last_pos}"
        );
        last_pos = pos;
    }
}

#[test]
fn section_with_empty_content() {
    let mut ctx = PersistentContext::new();
    ctx.add_section(ContextSection {
        label: "Empty".to_string(),
        content: String::new(),
        priority: 0,
    });

    let rendered = ctx.render();
    assert_eq!(rendered, "## Empty\n");
}

#[test]
fn section_with_empty_label() {
    let mut ctx = PersistentContext::new();
    ctx.add_section(ContextSection {
        label: String::new(),
        content: "Some content".to_string(),
        priority: 0,
    });

    let rendered = ctx.render();
    assert_eq!(rendered, "## \nSome content");
}

#[test]
fn sections_separated_by_double_newline() {
    let mut ctx = PersistentContext::new();
    ctx.add_section(ContextSection {
        label: "A".to_string(),
        content: "aaa".to_string(),
        priority: 0,
    });
    ctx.add_section(ContextSection {
        label: "B".to_string(),
        content: "bbb".to_string(),
        priority: 1,
    });

    let rendered = ctx.render();
    assert_eq!(rendered, "## A\naaa\n\n## B\nbbb");
}

#[test]
fn section_with_multiline_content() {
    let mut ctx = PersistentContext::new();
    ctx.add_section(ContextSection {
        label: "Multi".to_string(),
        content: "Line 1\nLine 2\nLine 3".to_string(),
        priority: 0,
    });

    let rendered = ctx.render();
    assert_eq!(rendered, "## Multi\nLine 1\nLine 2\nLine 3");
}

#[test]
fn high_priority_values() {
    let mut ctx = PersistentContext::new();
    ctx.add_section(ContextSection {
        label: "High".to_string(),
        content: "High priority value".to_string(),
        priority: usize::MAX,
    });
    ctx.add_section(ContextSection {
        label: "Low".to_string(),
        content: "Low priority value".to_string(),
        priority: 0,
    });

    let rendered = ctx.render();
    let low_pos = rendered.find("Low").expect("Low should exist");
    let high_pos = rendered.find("High").expect("High should exist");
    assert!(
        low_pos < high_pos,
        "Low priority (0) should come before High priority (MAX)"
    );
}

#[test]
fn default_creates_empty_context() {
    let ctx = PersistentContext::default();
    assert_eq!(ctx.render(), "");
}

#[test]
fn context_section_clone() {
    let section = ContextSection {
        label: "Original".to_string(),
        content: "Content".to_string(),
        priority: 5,
    };
    let cloned = section.clone();
    assert_eq!(cloned.label, "Original");
    assert_eq!(cloned.content, "Content");
    assert_eq!(cloned.priority, 5);
}

#[test]
fn context_section_debug() {
    let section = ContextSection {
        label: "Test".to_string(),
        content: "Data".to_string(),
        priority: 1,
    };
    let debug = format!("{section:?}");
    assert!(debug.contains("Test"));
    assert!(debug.contains("Data"));
}

#[test]
fn persistent_context_debug() {
    let ctx = PersistentContext::new();
    let debug = format!("{ctx:?}");
    assert!(debug.contains("PersistentContext"));
}
