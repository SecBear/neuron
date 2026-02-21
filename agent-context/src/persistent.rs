//! Persistent context sections for structured system prompt construction.

/// A single named section within a [`PersistentContext`].
///
/// Sections are rendered in ascending `priority` order (lower number = higher
/// priority = rendered first).
#[derive(Debug, Clone)]
pub struct ContextSection {
    /// Human-readable label for this section (used as a heading).
    pub label: String,
    /// The text content of this section.
    pub content: String,
    /// Render order — lower values appear first.
    pub priority: usize,
}

/// Aggregates named context sections and renders them into a single string.
///
/// Sections are sorted by `priority` (ascending) before rendering. Use this to
/// build structured system prompts from multiple independent sources.
///
/// # Example
///
/// ```
/// use agent_context::{PersistentContext, ContextSection};
///
/// let mut ctx = PersistentContext::new();
/// ctx.add_section(ContextSection { label: "Role".into(), content: "You are helpful.".into(), priority: 0 });
/// ctx.add_section(ContextSection { label: "Rules".into(), content: "Be concise.".into(), priority: 10 });
/// let rendered = ctx.render();
/// assert!(rendered.contains("Role"));
/// assert!(rendered.contains("Rules"));
/// ```
#[derive(Debug, Default)]
pub struct PersistentContext {
    sections: Vec<ContextSection>,
}

impl PersistentContext {
    /// Creates an empty `PersistentContext`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a section to this context.
    pub fn add_section(&mut self, section: ContextSection) {
        self.sections.push(section);
    }

    /// Renders all sections into a single string, sorted by `priority`.
    ///
    /// Each section is formatted as:
    /// ```text
    /// ## <label>
    /// <content>
    /// ```
    #[must_use]
    pub fn render(&self) -> String {
        let mut sorted = self.sections.clone();
        sorted.sort_by_key(|s| s.priority);

        sorted
            .into_iter()
            .map(|s| format!("## {}\n{}", s.label, s.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}
