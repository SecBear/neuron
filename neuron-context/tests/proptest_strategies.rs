//! Property-based tests: context strategy invariants.

use proptest::prelude::*;
use neuron_types::*;
use neuron_context::TokenCounter;

fn arb_text_message() -> impl Strategy<Value = Message> {
    ("[a-zA-Z ]{1,200}", prop_oneof![Just(Role::User), Just(Role::Assistant)])
        .prop_map(|(text, role)| Message {
            role,
            content: vec![ContentBlock::Text(text)],
        })
}

proptest! {
    #[test]
    fn token_count_monotonic(
        messages in proptest::collection::vec(arb_text_message(), 1..20),
    ) {
        let counter = TokenCounter::default();
        let mut prev_count = 0;
        for i in 1..=messages.len() {
            let count = counter.estimate_messages(&messages[..i]);
            prop_assert!(count >= prev_count,
                "Token count decreased: {} -> {} at message {}",
                prev_count, count, i);
            prev_count = count;
        }
    }

    #[test]
    fn token_count_non_negative(msg in arb_text_message()) {
        let counter = TokenCounter::default();
        let count = counter.estimate_messages(&[msg]);
        prop_assert!(count > 0, "Token count should be positive for non-empty message");
    }
}
