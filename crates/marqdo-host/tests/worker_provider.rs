use marqdo_host::{Capabilities, Host, StdHost};
use marqdo_sandbox::SandboxMode;
use marqdo_value::ErrorCategory;

fn context() -> marqdo_host::ExecutionContext {
    marqdo_host::ExecutionContext::new("test".to_string(), None)
}

fn handle(value: marqdo_value::Value) -> marqdo_value::ResourceHandle {
    match value {
        marqdo_value::Value::Resource(handle) => handle,
        other => panic!("expected resource, got {other:?}"),
    }
}

fn helper() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("provider.py");
    std::fs::write(
        &script,
        r#"
import sys, json, struct

def read_frame():
    header = sys.stdin.buffer.read(4)
    if len(header) < 4:
        return None
    size = struct.unpack(">I", header)[0]
    return json.loads(sys.stdin.buffer.read(size))

def write_frame(value):
    payload = json.dumps(value).encode()
    sys.stdout.buffer.write(struct.pack(">I", len(payload)) + payload)
    sys.stdout.buffer.flush()

data = b"hello world"
position = 0
while True:
    request = read_frame()
    if request is None:
        break
    op = request["op"]
    if op == "ping":
        write_frame({"ok": True})
    elif op == "open":
        write_frame({"ok": True, "token": 7})
    elif op == "read":
        count = request["max_bytes"]
        chunk = data[position:position + count]
        position += len(chunk)
        write_frame({"ok": True, "bytes": list(chunk), "eof": position >= len(data)})
    elif op == "write":
        write_frame({"ok": True, "count": len(request["bytes"])})
    elif op == "close":
        write_frame({"ok": True})
    elif op == "shutdown":
        break
    else:
        write_frame({"ok": False, "code": "resource.unsupported", "message": op})
"#,
    )
    .unwrap();
    (dir, script)
}

#[test]
fn worker_provider_round_trips_through_framed_json() {
    let (dir, script) = helper();
    let project = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("worker"),
        project.path(),
    );
    host.register_worker_resource_provider(
        "worker",
        &["worker.sensor"],
        &std::path::PathBuf::from("python3"),
        &[script.to_string_lossy().to_string()],
    )
    .unwrap();
    let ctx = context();
    let handle = handle(
        host.resource_open("worker.sensor", "sensor-1", &["read".to_string()], &ctx)
            .unwrap(),
    );
    let first = host.resource_read(handle, 5, &ctx).unwrap();
    assert_eq!(first.bytes, b"hello");
    assert!(!first.eof);
    let rest = host.resource_read(handle, 64, &ctx).unwrap();
    assert_eq!(rest.bytes, b" world");
    assert!(rest.eof);
    assert_eq!(host.resource_write(handle, b"abc", &ctx).unwrap(), 3);
    host.resource_close(handle, &ctx).unwrap();
    assert_eq!(
        host.resource_close(handle, &ctx).unwrap_err().code(),
        "resource.closed"
    );
    drop(dir);
}

#[test]
fn worker_provider_registration_requires_a_grant_and_project_mode() {
    let (_dir, script) = helper();
    let project = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut denied = StdHost::new(
        SandboxMode::Project,
        Capabilities::project(),
        project.path(),
    );
    denied
        .register_worker_resource_provider(
            "worker",
            &["worker.sensor"],
            &std::path::PathBuf::from("python3"),
            &[script.to_string_lossy().to_string()],
        )
        .unwrap();
    let error = denied
        .resource_open("worker.sensor", "s", &["read".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);

    for mode in [SandboxMode::Pure, SandboxMode::Strict] {
        let mut host = StdHost::new(
            mode,
            Capabilities::project().allow_resource_provider("worker"),
            project.path(),
        );
        host.register_worker_resource_provider(
            "worker",
            &["worker.sensor"],
            &std::path::PathBuf::from("python3"),
            &[script.to_string_lossy().to_string()],
        )
        .unwrap();
        let error = host
            .resource_open("worker.sensor", "s", &["read".to_string()], &ctx)
            .unwrap_err();
        assert_eq!(error.category, ErrorCategory::PermissionDenied);
    }
}

#[test]
fn missing_worker_program_is_a_structured_error() {
    let project = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("worker"),
        project.path(),
    );
    let error = host
        .register_worker_resource_provider(
            "worker",
            &["worker.sensor"],
            &std::path::PathBuf::from("/definitely/missing/marqdo-provider"),
            &[],
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::KernelUnavailable);
}
