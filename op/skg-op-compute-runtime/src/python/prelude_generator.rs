use layer0::capability::{
    ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityId, CapabilityKind, ExecutionClass,
    SchedulingFacts,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PythonBindingModule {
    Core,
    Fs,
}

pub(crate) fn default_binding_modules() -> &'static [PythonBindingModule] {
    &[PythonBindingModule::Core, PythonBindingModule::Fs]
}

fn module_scheduling() -> SchedulingFacts {
    SchedulingFacts::new(ExecutionClass::Shared, false, true, true, None)
}

fn mk_descriptor(id: &str, name: &str, description: &str) -> CapabilityDescriptor {
    CapabilityDescriptor::new(
        CapabilityId::new(id),
        CapabilityKind::Tool,
        name,
        description,
        module_scheduling(),
        ApprovalFacts::None,
        AuthFacts::Open,
    )
}

fn module_descriptors(module: PythonBindingModule) -> Vec<CapabilityDescriptor> {
    match module {
        PythonBindingModule::Core => vec![
            mk_descriptor(
                "compute.core.final",
                "final",
                "Record the final result for this execution",
            ),
            mk_descriptor(
                "compute.core.note",
                "note",
                "Record an informational note for this execution",
            ),
            mk_descriptor(
                "compute.core.capabilities",
                "capabilities",
                "List available prelude bindings as capability descriptors",
            ),
            mk_descriptor(
                "compute.core.help_bindings",
                "help_bindings",
                "Show help for available prelude bindings",
            ),
        ],
        PythonBindingModule::Fs => vec![
            mk_descriptor(
                "compute.fs.read",
                "read",
                "Read UTF-8 text from a file with optional 1-indexed offset and limit",
            ),
            mk_descriptor(
                "compute.fs.write",
                "write",
                "Write UTF-8 text to a file, creating parent directories if needed",
            ),
            mk_descriptor(
                "compute.fs.append",
                "append",
                "Append UTF-8 text to a file, creating parent directories if needed",
            ),
            mk_descriptor(
                "compute.fs.find",
                "find",
                "Find paths by glob pattern relative to a base path",
            ),
            mk_descriptor(
                "compute.fs.grep",
                "grep",
                "Search a UTF-8 text file with regex or literal matching and return line hits",
            ),
        ],
    }
}

/// Return the static list of core binding capability descriptors.
#[allow(dead_code)]
pub(crate) fn core_binding_descriptors() -> Vec<CapabilityDescriptor> {
    module_descriptors(PythonBindingModule::Core)
}

/// Return the static list of filesystem binding capability descriptors.
#[allow(dead_code)]
pub(crate) fn fs_binding_descriptors() -> Vec<CapabilityDescriptor> {
    module_descriptors(PythonBindingModule::Fs)
}

#[allow(dead_code)]
pub(crate) fn binding_descriptors(modules: &[PythonBindingModule]) -> Vec<CapabilityDescriptor> {
    let mut out = Vec::new();
    for module in modules {
        out.extend(module_descriptors(*module));
    }
    out
}

fn core_prelude_code() -> &'static str {
    r#"
def _binding_items(module=None, name=None):
    items = _ALL_CAPABILITIES
    if module is not None:
        prefix = "compute." + str(module) + "."
        items = [item for item in items if str(item.get("id", "")).startswith(prefix)]
    if name is not None:
        items = [item for item in items if item.get("name") == name]
    return items


def final(value):
    """Record the final result for this execution."""
    try:
        _store = globals().get("__SKG_RESULT")
        if isinstance(_store, dict):
            _store["final"] = value
    except Exception:
        pass


def note(text):
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


def capabilities():
    """Return capability descriptors for available prelude bindings."""
    return _ALL_CAPABILITIES


def help_bindings(module=None, name=None):
    """Return human-readable help for prelude bindings."""
    items = _binding_items(module, name)
    return "\n".join([item["name"] + ": " + item["description"] for item in items])
"#
}

fn fs_prelude_code() -> &'static str {
    r#"
def read(path, offset=1, limit=None):
    """Read UTF-8 text from a file with 1-indexed line offset/limit."""
    p = Path(path)
    lines = p.read_text(encoding="utf-8").splitlines()
    start = max(int(offset) - 1, 0)
    if limit is None:
        end = len(lines)
    else:
        end = start + max(int(limit), 0)
    return "\n".join(lines[start:end])


def write(path, content):
    """Write UTF-8 text to a file and return its path."""
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(str(content), encoding="utf-8")
    return str(p)


def append(path, content):
    """Append UTF-8 text to a file and return its path."""
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    with p.open("a", encoding="utf-8") as handle:
        handle.write(str(content))
    return str(p)


def _is_hidden_path(path):
    return any(part.startswith(".") for part in Path(path).parts if part not in ("", "."))


def find(pattern, path=".", hidden=False, limit=1000):
    """Find paths matching a glob-like pattern relative to a base path."""
    base = Path(path)
    pattern = str(pattern)
    if "/" in pattern or "**" in pattern:
        iterator = base.glob(pattern)
    else:
        iterator = base.rglob(pattern)
    matches = []
    for match in iterator:
        if not hidden and _is_hidden_path(match):
            continue
        matches.append(str(match))
    matches = sorted(set(matches))
    return matches[: max(int(limit), 0)]


def grep(pattern, path, ignore_case=False, literal=False):
    """Search a UTF-8 file and return [(line_number, text), ...]."""
    flags = re.IGNORECASE if ignore_case else 0
    expr = re.escape(pattern) if literal else pattern
    rx = re.compile(expr, flags)
    hits = []
    for idx, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), start=1):
        if rx.search(line):
            hits.append((idx, line))
    return hits
"#
}

fn module_prelude_code(module: PythonBindingModule) -> &'static str {
    match module {
        PythonBindingModule::Core => core_prelude_code(),
        PythonBindingModule::Fs => fs_prelude_code(),
    }
}

/// Render the Python prelude for the requested binding modules.
#[allow(dead_code)]
pub(crate) fn render_prelude(modules: &[PythonBindingModule]) -> String {
    let capabilities_json = serde_json::to_string(&binding_descriptors(modules))
        .expect("binding descriptors must serialize");

    let mut out = String::from(
        "# Skelegent compute runtime — generated Python prelude\n\nfrom pathlib import Path\nimport json\nimport re\n\n_ALL_CAPABILITIES = json.loads(",
    );
    out.push_str(&format!("{capabilities_json:?}"));
    out.push_str(")\n");

    for module in modules {
        out.push_str(module_prelude_code(*module));
        out.push('\n');
    }

    out
}

/// Render the default Python prelude used by the local backend.
#[allow(dead_code)]
pub(crate) fn render_default_prelude() -> String {
    render_prelude(default_binding_modules())
}
