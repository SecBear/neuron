#[path = "../src/python/prelude_generator.rs"]
mod prelude_generator;

use layer0::capability::CapabilityKind;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

fn unique_tempdir(prefix: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "skelegent-compute-{prefix}-{}-{}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn worker_script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("python")
        .join("worker.py")
}

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
fn python_default_prelude_exposes_core_and_fs_bindings() {
    let prelude = prelude_generator::render_default_prelude();
    for name in [
        "final",
        "note",
        "capabilities",
        "help_bindings",
        "read",
        "write",
        "append",
        "find",
        "grep",
    ] {
        let needle = format!("def {name}(");
        assert!(
            prelude.contains(&needle),
            "prelude missing binding: {name}\n---\n{prelude}\n---"
        );
    }
}

#[test]
fn python_default_capabilities_include_core_and_fs_modules() {
    let mut ids: Vec<_> =
        prelude_generator::binding_descriptors(prelude_generator::default_binding_modules())
            .iter()
            .map(|d| d.id.as_str().to_string())
            .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "compute.core.capabilities".to_string(),
            "compute.core.final".to_string(),
            "compute.core.help_bindings".to_string(),
            "compute.core.note".to_string(),
            "compute.fs.append".to_string(),
            "compute.fs.find".to_string(),
            "compute.fs.grep".to_string(),
            "compute.fs.read".to_string(),
            "compute.fs.write".to_string(),
        ]
    );

    let read_cap =
        prelude_generator::binding_descriptors(prelude_generator::default_binding_modules())
            .into_iter()
            .find(|d| d.id.as_str() == "compute.fs.read")
            .expect("read capability");
    assert_eq!(read_cap.kind, CapabilityKind::Tool);
    assert_eq!(read_cap.name, "read");
    assert!(read_cap.description.contains("Read UTF-8 text"));
}

#[test]
fn python_capabilities_payload_matches_default_descriptor_projection() {
    let prelude = prelude_generator::render_default_prelude();
    for id in [
        "compute.core.final",
        "compute.core.note",
        "compute.core.capabilities",
        "compute.core.help_bindings",
        "compute.fs.read",
        "compute.fs.write",
        "compute.fs.append",
        "compute.fs.find",
        "compute.fs.grep",
    ] {
        assert!(
            prelude.contains(id),
            "prelude capabilities() payload must include {id}"
        );
    }
}

#[test]
fn python_worker_protocol_round_trips_persists_and_exposes_fs_helpers() {
    let cwd = unique_tempdir("worker-protocol");
    fs::write(cwd.join("alpha.txt"), "line1\nline2\nline3\n").expect("seed file");

    let mut child = Command::new("python3")
        .arg("-u")
        .arg(worker_script_path())
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn worker");

    let prelude = prelude_generator::render_default_prelude();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    write_msg(
        &mut stdin,
        &serde_json::json!({"op":"init","prelude": prelude}),
    );
    let init_resp = read_msg(&mut stdout);
    assert_eq!(init_resp.get("ok").and_then(|v| v.as_bool()), Some(true));

    write_msg(
        &mut stdin,
        &serde_json::json!({"op":"exec","code": r#"x = 41
print(read('alpha.txt', offset=2, limit=1))
print(find('*.txt'))
print(grep('line3', 'alpha.txt'))
write('beta.txt', 'hello')
append('beta.txt', ' world')
note(help_bindings('fs'))
final({'answer': x + 1, 'beta': read('beta.txt')})"# }),
    );
    let resp1 = read_msg(&mut stdout);
    if resp1.get("exit_code").and_then(|v| v.as_i64()) != Some(0) {
        panic!(
            "exec1 stderr: {}",
            resp1.get("stderr").and_then(|v| v.as_str()).unwrap_or("")
        );
    }
    let stdout_text = resp1.get("stdout").and_then(|v| v.as_str()).unwrap();
    assert!(stdout_text.contains("line2"));
    assert!(stdout_text.contains("alpha.txt"));
    assert!(stdout_text.contains("line3"));
    assert_eq!(
        resp1.get("final_result").unwrap()["answer"].as_i64(),
        Some(42)
    );
    assert_eq!(
        resp1.get("final_result").unwrap()["beta"].as_str(),
        Some("hello world")
    );
    let notes = resp1.get("notes").unwrap().as_array().unwrap();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].as_str().unwrap().contains("read:"));

    write_msg(
        &mut stdin,
        &serde_json::json!({"op":"exec","code": "print(x)"}),
    );
    let resp2 = read_msg(&mut stdout);
    assert_eq!(resp2.get("exit_code").and_then(|v| v.as_i64()), Some(0));
    assert_eq!(
        resp2.get("stdout").and_then(|v| v.as_str()).unwrap().trim(),
        "41"
    );

    write_msg(&mut stdin, &serde_json::json!({"op":"reset"}));
    let _ = read_msg(&mut stdout);
    write_msg(
        &mut stdin,
        &serde_json::json!({"op":"exec","code": "print('x' in globals())"}),
    );
    let resp3 = read_msg(&mut stdout);
    assert_eq!(
        resp3.get("stdout").and_then(|v| v.as_str()).unwrap().trim(),
        "False"
    );

    write_msg(&mut stdin, &serde_json::json!({"op":"close"}));
    drop(stdin);
    drop(stdout);
    let _ = child.wait();

    let _ = fs::remove_dir_all(cwd);
}

#[tokio::test]
async fn python_backend_exec_respects_working_dir_and_fs_bindings() {
    use skg_op_compute_runtime::backend::ComputeBackend;
    use skg_op_compute_runtime::profile::ExecutionProfile;

    let cwd = unique_tempdir("backend-fs");
    fs::write(cwd.join("hello.txt"), "alpha\nbeta\n").expect("seed file");

    let backend = skg_op_compute_runtime::python::LocalPythonBackend;
    let profile = ExecutionProfile {
        working_dir: Some(cwd.clone()),
        ..ExecutionProfile::default()
    };
    let handle = backend.start(&profile).await.expect("start");

    let result = backend
        .exec(
            &handle,
            skg_op_compute_runtime::backend::BackendExecRequest {
                code: "print(read('hello.txt', offset=2, limit=1))\nprint(find('*.txt'))\nprint(grep('alpha', 'hello.txt'))\nwrite('out.txt', 'ok')".into(),
            },
        )
        .await
        .expect("exec");
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("beta"));
    assert!(result.stdout.contains("hello.txt"));
    assert!(result.stdout.contains("alpha"));
    assert_eq!(fs::read_to_string(cwd.join("out.txt")).unwrap(), "ok");

    backend.stop(handle).await.expect("stop");
    let _ = fs::remove_dir_all(cwd);
}
