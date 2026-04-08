#[path = "../src/python/prelude_generator.rs"]
mod prelude_generator;

use layer0::capability::CapabilityKind;

#[test]
fn python_core_prelude_exposes_bindings() {
    let prelude = prelude_generator::render_core_prelude();
    for name in ["final", "note", "capabilities", "help_bindings"] {
        let needle = format!("def {name}(");
        assert!(
            prelude.contains(&needle),
            "prelude missing binding: {name}\n---\n{prelude}\n---"
        );
    }
}

#[test]
fn python_core_capabilities_are_projected() {
    let caps = prelude_generator::core_binding_descriptors();
    let mut ids: Vec<_> = caps.iter().map(|d| d.id.as_str().to_string()).collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "compute.core.capabilities".to_string(),
            "compute.core.final".to_string(),
            "compute.core.help_bindings".to_string(),
            "compute.core.note".to_string(),
        ]
    );

    let final_cap = caps
        .iter()
        .find(|d| d.id.as_str() == "compute.core.final")
        .expect("final");
    assert_eq!(final_cap.kind, CapabilityKind::Tool);
    assert_eq!(final_cap.name, "final");
    assert!(final_cap.accepts.is_empty());
    assert!(final_cap.produces.is_empty());
}

#[test]
fn python_capabilities_prelude_matches_descriptor_projection() {
    let prelude = prelude_generator::render_core_prelude();
    for id in [
        "compute.core.final",
        "compute.core.note",
        "compute.core.capabilities",
        "compute.core.help_bindings",
    ] {
        assert!(
            prelude.contains(id),
            "prelude capabilities() payload must include {id}"
        );
    }
}

use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn write_msg<W: Write>(w: &mut W, value: &serde_json::Value) {
    let bytes = serde_json::to_vec(value).expect("json");
    let len = (bytes.len() as u32).to_be_bytes();
    w.write_all(&len).unwrap();
    w.write_all(&bytes).unwrap();
    w.flush().unwrap();
}

fn read_msg<R: Read>(r: &mut R) -> serde_json::Value {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).unwrap();
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).unwrap();
    serde_json::from_slice(&buf).unwrap()
}

#[test]
fn python_worker_protocol_round_trips_and_persists() {
    // Spawn the worker directly to exercise protocol, persistence, and reset.
    let mut child = Command::new("python3")
        .arg("-u")
        .arg("src/python/worker.py")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn worker");

    let prelude = prelude_generator::render_core_prelude();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    // init
    write_msg(&mut stdin, &serde_json::json!({"op":"init","prelude": prelude}));
    let init_resp = read_msg(&mut stdout);
    assert_eq!(init_resp.get("ok").and_then(|v| v.as_bool()), Some(true));

    // exec #1: set a var and emit final + note + print
    write_msg(&mut stdin, &serde_json::json!({"op":"exec","code": r#"x = 41
print('hi')
note('n1')
final({'answer': x + 1})"# }));
    let resp1 = read_msg(&mut stdout);
    if resp1.get("exit_code").and_then(|v| v.as_i64()) != Some(0) {
        panic!("exec1 stderr: {}", resp1.get("stderr").and_then(|v| v.as_str()).unwrap_or(""));
    }
    assert_eq!(resp1.get("stdout").and_then(|v| v.as_str()).unwrap().trim(), "hi");
    assert_eq!(resp1.get("final_result").unwrap()["answer"].as_i64(), Some(42));
    assert_eq!(resp1.get("notes").unwrap().as_array().unwrap().len(), 1);

    // exec #2: confirm persistence of x
    write_msg(&mut stdin, &serde_json::json!({"op":"exec","code": "print(x)"}));
    let resp2 = read_msg(&mut stdout);
    assert_eq!(resp2.get("exit_code").and_then(|v| v.as_i64()), Some(0));
    assert_eq!(resp2.get("stdout").and_then(|v| v.as_str()).unwrap().trim(), "41");

    // reset clears namespace and reinstalls prelude
    write_msg(&mut stdin, &serde_json::json!({"op":"reset"}));
    let _ = read_msg(&mut stdout);
    write_msg(&mut stdin, &serde_json::json!({"op":"exec","code": "print('x' in globals())"}));
    let resp3 = read_msg(&mut stdout);
    assert_eq!(resp3.get("stdout").and_then(|v| v.as_str()).unwrap().trim(), "False");

    // help/capabilities round-trip via prelude
    write_msg(&mut stdin, &serde_json::json!({"op":"exec","code": "print('compute.core.final' in json.dumps(capabilities()))"}));
    let resp4 = read_msg(&mut stdout);
    assert_eq!(resp4.get("stdout").and_then(|v| v.as_str()).unwrap().trim(), "True");

    // close
    write_msg(&mut stdin, &serde_json::json!({"op":"close"}));
}

#[tokio::test]
async fn python_backend_exec_persists_namespace() {
    use skg_op_compute_runtime::backend::ComputeBackend;
    use skg_op_compute_runtime::profile::ExecutionProfile;
    // Use the real backend to verify init+exec plumbing.
    let backend = skg_op_compute_runtime::python::LocalPythonBackend::default();
    let profile = ExecutionProfile::default();
    let handle = backend.start(&profile).await.expect("start");

    let r1 = backend
        .exec(&handle, skg_op_compute_runtime::backend::BackendExecRequest {
            code: "x = 7".into(),
        })
        .await
        .expect("exec1");
    assert_eq!(r1.exit_code, 0);

    let r2 = backend
        .exec(&handle, skg_op_compute_runtime::backend::BackendExecRequest {
            code: "print(x)".into(),
        })
        .await
        .expect("exec2");
    assert_eq!(r2.exit_code, 0);
    assert_eq!(r2.stdout.trim(), "7");

    backend.stop(handle).await.expect("stop");
}
