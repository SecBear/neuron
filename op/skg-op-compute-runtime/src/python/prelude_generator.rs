use layer0::capability::{
    ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityId, CapabilityKind, ExecutionClass,
    SchedulingFacts,
};

/// Return the static list of core binding capability descriptors.
#[allow(dead_code)]
pub(crate) fn core_binding_descriptors() -> Vec<CapabilityDescriptor> {
    let scheduling = SchedulingFacts::new(ExecutionClass::Shared, false, true, true, None);

    let mk = |id: &str, name: &str, description: &str| {
        CapabilityDescriptor::new(
            CapabilityId::new(id),
            CapabilityKind::Tool,
            name,
            description,
            scheduling.clone(),
            ApprovalFacts::None,
            AuthFacts::Open,
        )
    };

    vec![
        mk(
            "compute.core.final",
            "final",
            "Record the final result for this execution",
        ),
        mk(
            "compute.core.note",
            "note",
            "Record an informational note for this execution",
        ),
        mk(
            "compute.core.capabilities",
            "capabilities",
            "List available prelude bindings as capability descriptors",
        ),
        mk(
            "compute.core.help_bindings",
            "help_bindings",
            "Show help for available prelude bindings",
        ),
    ]
}

/// Render the Python prelude that defines the core bindings.
#[allow(dead_code)]
pub(crate) fn render_core_prelude() -> String {
    let capabilities_json = serde_json::to_string(&core_binding_descriptors())
        .expect("core binding descriptors must serialize");
    format!(
        r#"# Skelegent compute runtime — core prelude (Task 3)

from typing import Any, List
import json

_CORE_CAPABILITIES = json.loads({capabilities_json:?})


def final(value: Any) -> None:
    """Record the final result for this execution."""
    try:
        _store = globals().get("__SKG_RESULT")
        if isinstance(_store, dict):
            _store["final"] = value
    except Exception:
        pass


def note(text: str) -> None:
    """Record a note for this execution."""
    try:
        _store = globals().get("__SKG_RESULT")
        if isinstance(_store, dict):
            _notes = _store.get("notes")
            if not isinstance(_notes, list):
                _notes = []
            _notes.append(str(text))
            _store["notes"] = _notes
    except Exception:
        pass


def capabilities() -> List[dict]:
    """Return capability descriptors for available prelude bindings."""
    return _CORE_CAPABILITIES


def help_bindings(module=None, name=None) -> str:
    """Return human-readable help for prelude bindings."""
    items = _CORE_CAPABILITIES
    if module is not None:
        items = [item for item in items if item.get('id', '').startswith(f'compute.{{module}}.')]
    if name is not None:
        items = [item for item in items if item.get('name') == name]
    return "\n".join(f"{{item['name']}}: {{item['description']}}" for item in items)
"#,
        capabilities_json = capabilities_json,

)
}