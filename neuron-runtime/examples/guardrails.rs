//! Guardrails: input and output safety checks with tripwire support.
//!
//! No API key needed — guardrails are pure functions on strings.
//!
//! Run with: cargo run --example guardrails -p neuron-runtime

use std::future::Future;

use neuron_runtime::{
    ErasedInputGuardrail, ErasedOutputGuardrail, GuardrailResult, InputGuardrail,
    OutputGuardrail, run_input_guardrails, run_output_guardrails,
};

// --- Input guardrail: reject messages containing passwords or secrets ---

struct NoPasswords;

impl InputGuardrail for NoPasswords {
    fn check(&self, input: &str) -> impl Future<Output = GuardrailResult> + Send {
        let lower = input.to_lowercase();
        async move {
            if lower.contains("password") || lower.contains("secret") {
                GuardrailResult::Tripwire(
                    "Input contains sensitive keywords (password/secret)".to_string(),
                )
            } else {
                GuardrailResult::Pass
            }
        }
    }
}

// --- A second input guardrail: reject SQL injection attempts ---

struct NoSqlInjection;

impl InputGuardrail for NoSqlInjection {
    fn check(&self, input: &str) -> impl Future<Output = GuardrailResult> + Send {
        let lower = input.to_lowercase();
        async move {
            if lower.contains("drop table") || lower.contains("'; --") {
                GuardrailResult::Tripwire(
                    "Input contains potential SQL injection".to_string(),
                )
            } else {
                GuardrailResult::Pass
            }
        }
    }
}

// --- Output guardrail: reject profanity in model output ---

struct NoProfanity;

impl OutputGuardrail for NoProfanity {
    fn check(&self, output: &str) -> impl Future<Output = GuardrailResult> + Send {
        let lower = output.to_lowercase();
        async move {
            let bad_words = ["damn", "hell", "crap"];
            for word in &bad_words {
                if lower.contains(word) {
                    return GuardrailResult::Tripwire(format!(
                        "Output contains profanity: '{word}'"
                    ));
                }
            }
            GuardrailResult::Pass
        }
    }
}

#[tokio::main]
async fn main() {
    // --- Input guardrail tests ---

    println!("=== Input Guardrail: NoPasswords ===");

    let guardrail = NoPasswords;

    let clean = "What is the weather in Tokyo?";
    let result = guardrail.check(clean).await;
    println!("  Input: {clean:?}");
    println!("  Result: {result:?}");
    assert!(result.is_pass());

    let dirty = "My password is hunter2";
    let result = guardrail.check(dirty).await;
    println!("  Input: {dirty:?}");
    println!("  Result: {result:?}");
    assert!(result.is_tripwire());

    // --- Output guardrail tests ---

    println!("\n=== Output Guardrail: NoProfanity ===");

    let guardrail = NoProfanity;

    let clean = "The result is 42.";
    let result = guardrail.check(clean).await;
    println!("  Output: {clean:?}");
    println!("  Result: {result:?}");
    assert!(result.is_pass());

    let dirty = "What the hell happened?";
    let result = guardrail.check(dirty).await;
    println!("  Output: {dirty:?}");
    println!("  Result: {result:?}");
    assert!(result.is_tripwire());

    // --- Running multiple input guardrails together ---

    println!("\n=== Multiple Input Guardrails ===");

    let no_passwords = NoPasswords;
    let no_sql = NoSqlInjection;
    let guardrails: Vec<&dyn ErasedInputGuardrail> = vec![&no_passwords, &no_sql];

    let clean = "Tell me about Rust programming";
    let result = run_input_guardrails(&guardrails, clean).await;
    println!("  Input: {clean:?}");
    println!("  Result: {result:?}  (expected: Pass)");
    assert!(result.is_pass());

    let has_password = "Store my secret key somewhere safe";
    let result = run_input_guardrails(&guardrails, has_password).await;
    println!("  Input: {has_password:?}");
    println!("  Result: {result:?}  (expected: Tripwire from NoPasswords)");
    assert!(result.is_tripwire());

    let has_sql = "Run this: DROP TABLE users; --";
    let result = run_input_guardrails(&guardrails, has_sql).await;
    println!("  Input: {has_sql:?}");
    println!("  Result: {result:?}  (expected: Tripwire from NoSqlInjection)");
    assert!(result.is_tripwire());

    // --- Running multiple output guardrails ---

    println!("\n=== Multiple Output Guardrails ===");

    let no_profanity = NoProfanity;
    let output_guardrails: Vec<&dyn ErasedOutputGuardrail> = vec![&no_profanity];

    let clean = "Here is a helpful summary of the data.";
    let result = run_output_guardrails(&output_guardrails, clean).await;
    println!("  Output: {clean:?}");
    println!("  Result: {result:?}  (expected: Pass)");
    assert!(result.is_pass());

    let dirty = "This is a damn mess.";
    let result = run_output_guardrails(&output_guardrails, dirty).await;
    println!("  Output: {dirty:?}");
    println!("  Result: {result:?}  (expected: Tripwire)");
    assert!(result.is_tripwire());

    println!("\nAll guardrail checks passed as expected.");
}
