//! Integration tests for PersistentContext and ContextSection.

use agent_context::{ContextSection, PersistentContext};

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
    let role_pos = rendered.find("Role").expect("Role section should be present");
    let reminder_pos = rendered.find("Reminder").expect("Reminder section should be present");
    let rules_pos = rendered.find("Rules").expect("Rules section should be present");

    assert!(role_pos < reminder_pos, "Role should come before Reminder");
    assert!(reminder_pos < rules_pos, "Reminder should come before Rules");
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
