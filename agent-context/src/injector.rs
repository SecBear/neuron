//! System prompt injection based on turn count and token thresholds.

/// Trigger condition for a [`SystemInjector`] rule.
#[derive(Debug, Clone)]
pub enum InjectionTrigger {
    /// Fire every N turns (turn % n == 0, excluding turn 0).
    EveryNTurns(usize),
    /// Fire when the token count meets or exceeds the threshold.
    OnTokenThreshold(usize),
}

struct InjectionRule {
    trigger: InjectionTrigger,
    content: String,
}

/// Injects system prompt content based on turn or token thresholds.
///
/// Add rules with [`SystemInjector::add_rule`], then call [`SystemInjector::check`]
/// each turn to get any content that should be injected.
///
/// # Example
///
/// ```
/// use agent_context::{SystemInjector, InjectionTrigger};
///
/// let mut injector = SystemInjector::new();
/// injector.add_rule(InjectionTrigger::EveryNTurns(5), "Reminder: be concise.".into());
/// injector.add_rule(InjectionTrigger::OnTokenThreshold(50_000), "Context is getting long.".into());
///
/// // Turn 5, under token threshold
/// let injected = injector.check(5, 10_000);
/// assert!(injected.contains(&"Reminder: be concise.".to_string()));
///
/// // Turn 1, over token threshold
/// let injected = injector.check(1, 60_000);
/// assert!(injected.contains(&"Context is getting long.".to_string()));
/// ```
#[derive(Default)]
pub struct SystemInjector {
    rules: Vec<InjectionRule>,
}

impl SystemInjector {
    /// Creates a new `SystemInjector` with no rules.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an injection rule.
    ///
    /// # Arguments
    /// * `trigger` — when this rule fires
    /// * `content` — the text to inject when triggered
    pub fn add_rule(&mut self, trigger: InjectionTrigger, content: String) {
        self.rules.push(InjectionRule { trigger, content });
    }

    /// Returns all content strings whose triggers are satisfied by the given state.
    ///
    /// # Arguments
    /// * `turn` — the current turn number (1-indexed for "every N" checks)
    /// * `token_count` — the current estimated token count
    #[must_use]
    pub fn check(&self, turn: usize, token_count: usize) -> Vec<String> {
        self.rules
            .iter()
            .filter(|rule| match rule.trigger {
                InjectionTrigger::EveryNTurns(n) => n > 0 && turn > 0 && turn.is_multiple_of(n),
                InjectionTrigger::OnTokenThreshold(threshold) => token_count >= threshold,
            })
            .map(|rule| rule.content.clone())
            .collect()
    }
}
