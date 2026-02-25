//! Configuration for NeuronTurn.

/// Static configuration for a NeuronTurn instance.
///
/// Per-request overrides come from `TurnInput.config` (layer0's `TurnConfig`).
/// This struct holds the defaults.
pub struct NeuronTurnConfig {
    /// Base system prompt for this turn implementation.
    pub system_prompt: String,

    /// Default model identifier.
    pub default_model: String,

    /// Default maximum output tokens per provider call.
    pub default_max_tokens: u32,

    /// Default maximum ReAct loop iterations.
    pub default_max_turns: u32,
}

impl Default for NeuronTurnConfig {
    fn default() -> Self {
        Self {
            system_prompt: "You are a helpful assistant.".into(),
            default_model: String::new(),
            default_max_tokens: 4096,
            default_max_turns: 25,
        }
    }
}
