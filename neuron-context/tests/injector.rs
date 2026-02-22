//! Integration tests for SystemInjector.

use neuron_context::{InjectionTrigger, SystemInjector};

#[test]
fn fires_on_every_n_turns() {
    let mut injector = SystemInjector::new();
    injector.add_rule(
        InjectionTrigger::EveryNTurns(5),
        "Reminder: be concise.".to_string(),
    );

    // Should fire on multiples of 5
    assert!(
        injector
            .check(5, 0)
            .contains(&"Reminder: be concise.".to_string())
    );
    assert!(
        injector
            .check(10, 0)
            .contains(&"Reminder: be concise.".to_string())
    );
    assert!(
        injector
            .check(15, 0)
            .contains(&"Reminder: be concise.".to_string())
    );

    // Should not fire on non-multiples
    assert!(injector.check(1, 0).is_empty());
    assert!(injector.check(4, 0).is_empty());
    assert!(injector.check(6, 0).is_empty());
}

#[test]
fn does_not_fire_on_turn_zero() {
    let mut injector = SystemInjector::new();
    injector.add_rule(InjectionTrigger::EveryNTurns(5), "content".to_string());

    // Turn 0 should not fire even though 0 % 5 == 0
    assert!(injector.check(0, 0).is_empty());
}

#[test]
fn fires_on_token_threshold() {
    let mut injector = SystemInjector::new();
    injector.add_rule(
        InjectionTrigger::OnTokenThreshold(50_000),
        "Context is getting long.".to_string(),
    );

    // At or above threshold → fire
    assert!(
        injector
            .check(1, 50_000)
            .contains(&"Context is getting long.".to_string())
    );
    assert!(
        injector
            .check(1, 60_000)
            .contains(&"Context is getting long.".to_string())
    );

    // Below threshold → no fire
    assert!(injector.check(1, 49_999).is_empty());
}

#[test]
fn multiple_rules_can_fire_simultaneously() {
    let mut injector = SystemInjector::new();
    injector.add_rule(
        InjectionTrigger::EveryNTurns(5),
        "Turn reminder".to_string(),
    );
    injector.add_rule(
        InjectionTrigger::OnTokenThreshold(10_000),
        "Token warning".to_string(),
    );

    let injected = injector.check(5, 15_000);
    assert!(injected.contains(&"Turn reminder".to_string()));
    assert!(injected.contains(&"Token warning".to_string()));
    assert_eq!(injected.len(), 2);
}

#[test]
fn no_rules_returns_empty() {
    let injector = SystemInjector::new();
    assert!(injector.check(10, 100_000).is_empty());
}

#[test]
fn only_matching_rules_fire() {
    let mut injector = SystemInjector::new();
    injector.add_rule(InjectionTrigger::EveryNTurns(10), "Turn 10".to_string());
    injector.add_rule(
        InjectionTrigger::OnTokenThreshold(100_000),
        "High tokens".to_string(),
    );

    // Turn 5, 50k tokens — neither rule fires
    assert!(injector.check(5, 50_000).is_empty());

    // Turn 10, 50k tokens — only turn rule fires
    let injected = injector.check(10, 50_000);
    assert_eq!(injected, vec!["Turn 10".to_string()]);
}
