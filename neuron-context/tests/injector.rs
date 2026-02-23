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

// ---- Additional coverage tests ----

#[test]
fn every_n_turns_zero_never_fires() {
    let mut injector = SystemInjector::new();
    injector.add_rule(
        InjectionTrigger::EveryNTurns(0),
        "Should never fire".to_string(),
    );

    // n=0 is a guard: the condition is `n > 0 && ...`, so nothing fires
    assert!(injector.check(0, 0).is_empty());
    assert!(injector.check(1, 0).is_empty());
    assert!(injector.check(100, 0).is_empty());
}

#[test]
fn every_n_turns_one_fires_every_turn_except_zero() {
    let mut injector = SystemInjector::new();
    injector.add_rule(InjectionTrigger::EveryNTurns(1), "Every turn".to_string());

    // Turn 0 should not fire
    assert!(injector.check(0, 0).is_empty());
    // Every other turn fires
    assert_eq!(injector.check(1, 0).len(), 1);
    assert_eq!(injector.check(2, 0).len(), 1);
    assert_eq!(injector.check(3, 0).len(), 1);
    assert_eq!(injector.check(100, 0).len(), 1);
}

#[test]
fn token_threshold_exact_boundary_fires() {
    let mut injector = SystemInjector::new();
    injector.add_rule(
        InjectionTrigger::OnTokenThreshold(1000),
        "At boundary".to_string(),
    );

    // Exactly at threshold → fires (>= not >)
    assert_eq!(injector.check(1, 1000).len(), 1);
    // One below → does not fire
    assert!(injector.check(1, 999).is_empty());
}

#[test]
fn token_threshold_zero_always_fires() {
    let mut injector = SystemInjector::new();
    injector.add_rule(
        InjectionTrigger::OnTokenThreshold(0),
        "Always fire".to_string(),
    );

    // 0 >= 0 is true
    assert_eq!(injector.check(1, 0).len(), 1);
    assert_eq!(injector.check(1, 100).len(), 1);
}

#[test]
fn multiple_every_n_turns_rules() {
    let mut injector = SystemInjector::new();
    injector.add_rule(InjectionTrigger::EveryNTurns(3), "Every 3".to_string());
    injector.add_rule(InjectionTrigger::EveryNTurns(5), "Every 5".to_string());

    // Turn 3: only every-3 fires
    let r = injector.check(3, 0);
    assert_eq!(r, vec!["Every 3".to_string()]);

    // Turn 5: only every-5 fires
    let r = injector.check(5, 0);
    assert_eq!(r, vec!["Every 5".to_string()]);

    // Turn 15: both fire (15 is multiple of both 3 and 5)
    let r = injector.check(15, 0);
    assert_eq!(r.len(), 2);
    assert!(r.contains(&"Every 3".to_string()));
    assert!(r.contains(&"Every 5".to_string()));

    // Turn 7: neither fires
    assert!(injector.check(7, 0).is_empty());
}

#[test]
fn multiple_token_threshold_rules() {
    let mut injector = SystemInjector::new();
    injector.add_rule(
        InjectionTrigger::OnTokenThreshold(10_000),
        "Low warning".to_string(),
    );
    injector.add_rule(
        InjectionTrigger::OnTokenThreshold(50_000),
        "High warning".to_string(),
    );

    // Below both
    assert!(injector.check(1, 5_000).is_empty());

    // Above low, below high
    let r = injector.check(1, 20_000);
    assert_eq!(r, vec!["Low warning".to_string()]);

    // Above both
    let r = injector.check(1, 60_000);
    assert_eq!(r.len(), 2);
    assert!(r.contains(&"Low warning".to_string()));
    assert!(r.contains(&"High warning".to_string()));
}

#[test]
fn large_turn_number() {
    let mut injector = SystemInjector::new();
    injector.add_rule(
        InjectionTrigger::EveryNTurns(1000),
        "Every 1000".to_string(),
    );

    assert!(injector.check(999, 0).is_empty());
    assert_eq!(injector.check(1000, 0).len(), 1);
    assert!(injector.check(1001, 0).is_empty());
    assert_eq!(injector.check(2000, 0).len(), 1);
}

#[test]
fn rules_fire_in_order_added() {
    let mut injector = SystemInjector::new();
    injector.add_rule(InjectionTrigger::OnTokenThreshold(0), "First".to_string());
    injector.add_rule(InjectionTrigger::OnTokenThreshold(0), "Second".to_string());
    injector.add_rule(InjectionTrigger::OnTokenThreshold(0), "Third".to_string());

    let r = injector.check(1, 100);
    assert_eq!(r, vec!["First", "Second", "Third"]);
}

#[test]
fn default_creates_empty_injector() {
    let injector = SystemInjector::default();
    assert!(injector.check(10, 100_000).is_empty());
}
