//! Resource/stream host boundary tests (ADR-0025).

use std::path::Path;

use marqdo_host::{Capabilities, ExecutionContext, Host, HostOp, StdHost};
use marqdo_sandbox::SandboxMode;
use marqdo_value::{ErrorCategory, Value};

fn context() -> ExecutionContext {
    ExecutionContext::new("index.mq.md".to_string(), None)
}

fn handle(value: Value) -> marqdo_value::ResourceHandle {
    match value {
        Value::Resource(handle) => handle,
        other => panic!("expected a Resource handle, got {other:?}"),
    }
}

fn write_project_file(root: &Path, name: &str, contents: &[u8]) {
    std::fs::write(root.join(name), contents).unwrap();
}

#[test]
fn pure_mode_writes_stdout_through_a_resource() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Pure, Capabilities::pure(), dir.path());
    let ctx = context();
    let handle = handle(
        host.resource_open("stdio", "stdout", &["write".to_string()], &ctx)
            .unwrap(),
    );
    assert_eq!(host.resource_write(handle, b"hello", &ctx).unwrap(), 5);
    assert_eq!(host.take_output(), b"hello");
    host.resource_close(handle, &ctx).unwrap();
    let error = host.resource_close(handle, &ctx).unwrap_err();
    assert_eq!(error.category, ErrorCategory::ResourceClosed);
    assert_eq!(error.code(), "resource.closed");
}

#[test]
fn stdout_write_obeys_the_output_budget_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Pure, Capabilities::pure(), dir.path());
    host.budget.max_output_bytes = 3;
    let ctx = context();
    let handle = handle(
        host.resource_open("stdio", "stdout", &["write".to_string()], &ctx)
            .unwrap(),
    );
    let error = host.resource_write(handle, b"hello", &ctx).unwrap_err();
    assert_eq!(error.category, ErrorCategory::ResourceExhausted);
    assert!(host.take_output().is_empty());
}

#[test]
fn pure_mode_reads_stdin_through_a_resource() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Pure, Capabilities::pure(), dir.path())
        .with_stdin(b"abcd".to_vec());
    let ctx = context();
    let handle = handle(
        host.resource_open("stdio", "stdin", &["read".to_string()], &ctx)
            .unwrap(),
    );
    let first = host.resource_read(handle, 2, &ctx).unwrap();
    assert_eq!(first.bytes, b"ab");
    assert!(!first.eof);
    let rest = host.resource_read(handle, 8, &ctx).unwrap();
    assert_eq!(rest.bytes, b"cd");
    assert!(rest.eof);
    assert_eq!(host.resource_read(handle, 1, &ctx).unwrap().bytes, b"");

    let write = host.resource_write(handle, b"x", &ctx).unwrap_err();
    assert_eq!(write.code(), "resource.unsupported");
}

#[test]
fn stdout_and_stderr_are_captured_separately() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Pure, Capabilities::pure(), dir.path());
    let ctx = context();
    let stdout = handle(
        host.resource_open("stdio", "stdout", &["write".to_string()], &ctx)
            .unwrap(),
    );
    let stderr = handle(
        host.resource_open("stdio", "stderr", &["write".to_string()], &ctx)
            .unwrap(),
    );
    assert_eq!(host.resource_write(stdout, b"out", &ctx).unwrap(), 3);
    assert_eq!(host.resource_write(stderr, b"err", &ctx).unwrap(), 3);
    assert_eq!(host.output(), b"out");
    assert_eq!(host.error(), b"err");
    assert_eq!(host.take_output(), b"out");
    assert_eq!(host.take_error(), b"err");
}

#[test]
fn strict_mode_refuses_stdin_and_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Strict, Capabilities::pure(), dir.path())
        .with_stdin(b"x".to_vec());
    let ctx = context();
    for (descriptor, rights) in [("stdin", "read"), ("stderr", "write")] {
        let error = host
            .resource_open("stdio", descriptor, &[rights.to_string()], &ctx)
            .unwrap_err();
        assert_eq!(error.category, ErrorCategory::PermissionDenied);
    }
}

#[test]
fn strict_mode_allows_only_the_existing_stdio_console_channel() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Strict, Capabilities::pure(), dir.path());
    let ctx = context();
    let handle = handle(
        host.resource_open("stdio", "stdout", &["write".to_string()], &ctx)
            .unwrap(),
    );
    assert_eq!(host.resource_write(handle, b"strict", &ctx).unwrap(), 6);
    assert_eq!(host.take_output(), b"strict");

    let file = host
        .resource_open("file", "a.txt", &["read".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(file.category, ErrorCategory::PermissionDenied);
    let socket = host
        .resource_open("socket", "example.test", &["read".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(socket.category, ErrorCategory::PermissionDenied);
}

#[test]
fn resource_handles_cannot_cross_the_generic_host_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Pure, Capabilities::pure(), dir.path());
    let ctx = context();
    let handle = handle(
        host.resource_open("stdio", "stdout", &["write".to_string()], &ctx)
            .unwrap(),
    );
    let error = host
        .invoke(HostOp::FsRead, vec![Value::Resource(handle)], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::Type);
    assert!(error.message.contains("Resource handles"));
}

#[test]
fn pure_mode_refuses_file_resources_even_with_a_grant() {
    let dir = tempfile::tempdir().unwrap();
    write_project_file(dir.path(), "a.txt", b"abcdef");
    let mut host = StdHost::new(
        SandboxMode::Pure,
        Capabilities::pure().allow_read("a.txt"),
        dir.path(),
    );
    let error = host
        .resource_open("file", "a.txt", &["read".to_string()], &context())
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
}

#[test]
fn project_file_writes_buffer_until_close_and_require_a_grant() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_write("out.bin"),
        dir.path(),
    );
    let ctx = context();
    let handle = handle(
        host.resource_open("file", "out.bin", &["write".to_string()], &ctx)
            .unwrap(),
    );
    assert_eq!(host.resource_write(handle, b"abc", &ctx).unwrap(), 3);
    assert_eq!(host.resource_write(handle, b"def", &ctx).unwrap(), 3);
    assert!(
        !dir.path().join("out.bin").exists(),
        "write must not flush early"
    );
    host.resource_close(handle, &ctx).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("out.bin")).unwrap(),
        b"abcdef"
    );

    let error = host.resource_read(handle, 1, &ctx).unwrap_err();
    assert_eq!(error.code(), "resource.closed");
    let second = host.resource_close(handle, &ctx).unwrap_err();
    assert_eq!(second.code(), "resource.closed");

    let mut denied = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let error = denied
        .resource_open("file", "out.bin", &["write".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
}

#[test]
fn file_write_buffer_has_a_hard_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_write("big.bin"),
        dir.path(),
    );
    let ctx = context();
    let handle = handle(
        host.resource_open("file", "big.bin", &["write".to_string()], &ctx)
            .unwrap(),
    );
    let oversized = vec![0u8; marqdo_host::fs::MAX_READ_BYTES as usize + 1];
    let error = host.resource_write(handle, &oversized, &ctx).unwrap_err();
    assert_eq!(error.category, ErrorCategory::ResourceExhausted);
    assert_eq!(host.resource_write(handle, b"ok", &ctx).unwrap(), 2);
    host.resource_close(handle, &ctx).unwrap();
    assert_eq!(std::fs::read(dir.path().join("big.bin")).unwrap(), b"ok");
}

#[test]
fn project_file_reads_are_chunked_and_require_a_grant() {
    let dir = tempfile::tempdir().unwrap();
    write_project_file(dir.path(), "a.txt", b"abcdef");
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_read("a.txt"),
        dir.path(),
    );
    let ctx = context();
    let handle = handle(
        host.resource_open("file", "a.txt", &["read".to_string()], &ctx)
            .unwrap(),
    );
    let first = host.resource_read(handle, 2, &ctx).unwrap();
    assert_eq!(first.bytes, b"ab");
    assert!(!first.eof);
    let second = host.resource_read(handle, 8, &ctx).unwrap();
    assert_eq!(second.bytes, b"cdef");
    assert!(second.eof);
    assert!(host.resource_read(handle, 1, &ctx).unwrap().eof);

    let mut denied = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let error = denied
        .resource_open("file", "a.txt", &["read".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
}

#[test]
fn traversal_and_unsupported_rights_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_read("**"),
        dir.path(),
    );
    let ctx = context();
    let traversal = host
        .resource_open("file", "../secret", &["read".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(traversal.category, ErrorCategory::PermissionDenied);
    let write_rights = host
        .resource_open("stdio", "stdout", &["read".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(write_rights.category, ErrorCategory::ResourceUnsupported);
    let unknown_kind = host
        .resource_open("socket", "example.test", &["read".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(unknown_kind.category, ErrorCategory::ResourceUnsupported);
}

struct MockProvider {
    data: Vec<u8>,
    position: usize,
    written: Vec<u8>,
    version: u32,
    kinds: Vec<&'static str>,
}

impl MockProvider {
    fn new(data: &[u8], version: u32, kinds: Vec<&'static str>) -> Self {
        Self {
            data: data.to_vec(),
            position: 0,
            written: Vec::new(),
            version,
            kinds,
        }
    }
}

impl marqdo_host::ResourceProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }
    fn protocol_version(&self) -> u32 {
        self.version
    }
    fn kinds(&self) -> &[&str] {
        &self.kinds
    }
    fn open(
        &mut self,
        _kind: &str,
        _descriptor: &str,
        _rights: &[String],
        _span: Option<marqdo_span::Span>,
    ) -> Result<u64, marqdo_value::RuntimeError> {
        self.position = 0;
        Ok(7)
    }
    fn read(
        &mut self,
        token: u64,
        max_bytes: usize,
        _span: Option<marqdo_span::Span>,
    ) -> Result<marqdo_host::ResourceRead, marqdo_value::RuntimeError> {
        assert_eq!(token, 7);
        let start = self.position;
        let end = start.saturating_add(max_bytes).min(self.data.len());
        self.position = end;
        Ok(marqdo_host::ResourceRead {
            bytes: self.data[start..end].to_vec(),
            eof: end >= self.data.len(),
        })
    }
    fn write(
        &mut self,
        token: u64,
        bytes: &[u8],
        _span: Option<marqdo_span::Span>,
    ) -> Result<usize, marqdo_value::RuntimeError> {
        assert_eq!(token, 7);
        self.written.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn close(
        &mut self,
        token: u64,
        _span: Option<marqdo_span::Span>,
    ) -> Result<(), marqdo_value::RuntimeError> {
        assert_eq!(token, 7);
        Ok(())
    }
}

#[test]
fn registered_provider_streams_work_with_a_grant_in_project_mode() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("mock"),
        dir.path(),
    );
    host.register_resource_provider(Box::new(MockProvider::new(
        b"abcdef",
        1,
        vec!["mock.sensor"],
    )))
    .unwrap();
    let ctx = context();
    let handle = handle(
        host.resource_open("mock.sensor", "sensor-1", &["read".to_string()], &ctx)
            .unwrap(),
    );
    let first = host.resource_read(handle, 2, &ctx).unwrap();
    assert_eq!(first.bytes, b"ab");
    assert!(!first.eof);
    let rest = host.resource_read(handle, 16, &ctx).unwrap();
    assert_eq!(rest.bytes, b"cdef");
    assert!(rest.eof);
    assert_eq!(host.resource_write(handle, b"x", &ctx).unwrap(), 1);
    host.resource_close(handle, &ctx).unwrap();
    assert_eq!(
        host.resource_close(handle, &ctx).unwrap_err().code(),
        "resource.closed"
    );
}

#[test]
fn provider_requires_a_name_grant_and_project_mode() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut denied = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    denied
        .register_resource_provider(Box::new(MockProvider::new(b"x", 1, vec!["mock.sensor"])))
        .unwrap();
    let error = denied
        .resource_open("mock.sensor", "s", &["read".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);

    for mode in [SandboxMode::Pure, SandboxMode::Strict] {
        let mut host = StdHost::new(
            mode,
            Capabilities::pure().allow_resource_provider("mock"),
            dir.path(),
        );
        host.register_resource_provider(Box::new(MockProvider::new(b"x", 1, vec!["mock.sensor"])))
            .unwrap();
        let error = host
            .resource_open("mock.sensor", "s", &["read".to_string()], &ctx)
            .unwrap_err();
        assert_eq!(error.category, ErrorCategory::PermissionDenied);
    }
}

#[test]
fn provider_registration_rejects_versions_reserved_names_and_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("mock"),
        dir.path(),
    );
    let bad_version = host
        .register_resource_provider(Box::new(MockProvider::new(b"x", 2, vec!["mock.sensor"])))
        .unwrap_err();
    assert_eq!(bad_version.category, ErrorCategory::ResourceUnsupported);
    let reserved = host
        .register_resource_provider(Box::new(MockProvider::new(b"x", 1, vec!["stdio"])))
        .unwrap_err();
    assert_eq!(reserved.category, ErrorCategory::ResourceUnsupported);
    host.register_resource_provider(Box::new(MockProvider::new(b"x", 1, vec!["mock.sensor"])))
        .unwrap();
    let conflict = host
        .register_resource_provider(Box::new(MockProvider::new(b"y", 1, vec!["mock.sensor"])))
        .unwrap_err();
    assert_eq!(conflict.category, ErrorCategory::ResourceUnsupported);
}

#[test]
fn tcp_resource_round_trips_with_a_network_grant() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0u8; 4];
        stream.read_exact(&mut buffer).unwrap();
        stream.write_all(&buffer).unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_net("127.0.0.1"),
        dir.path(),
    );
    let ctx = context();
    let handle = handle(
        host.resource_open(
            "tcp",
            &address.to_string(),
            &["read".to_string(), "write".to_string()],
            &ctx,
        )
        .unwrap(),
    );
    assert_eq!(host.resource_write(handle, b"ping", &ctx).unwrap(), 4);
    let read = host.resource_read(handle, 4, &ctx).unwrap();
    assert_eq!(read.bytes, b"ping");
    host.resource_close(handle, &ctx).unwrap();
    server.join().unwrap();
}

#[test]
fn tcp_requires_a_host_grant_in_project_mode() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let ctx = context();
    for descriptor in ["127.0.0.1:9", "example.test:80"] {
        let denied = host
            .resource_open(
                "tcp",
                descriptor,
                &["read".to_string(), "write".to_string()],
                &ctx,
            )
            .unwrap_err();
        assert_eq!(denied.category, ErrorCategory::PermissionDenied);
    }

    for mode in [SandboxMode::Pure, SandboxMode::Strict] {
        let mut host = StdHost::new(
            mode,
            Capabilities::project().allow_net("127.0.0.1"),
            dir.path(),
        );
        let error = host
            .resource_open(
                "tcp",
                "127.0.0.1:9",
                &["read".to_string(), "write".to_string()],
                &ctx,
            )
            .unwrap_err();
        assert_eq!(error.category, ErrorCategory::PermissionDenied);
    }
}

#[test]
fn tcp_dns_name_connects_with_a_host_grant() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0u8; 4];
        stream.read_exact(&mut buffer).unwrap();
        stream.write_all(&buffer).unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_net("localhost"),
        dir.path(),
    );
    let ctx = context();
    let handle = handle(
        host.resource_open(
            "tcp",
            &format!("localhost:{port}"),
            &["read".to_string(), "write".to_string()],
            &ctx,
        )
        .unwrap(),
    );
    assert_eq!(host.resource_write(handle, b"ping", &ctx).unwrap(), 4);
    assert_eq!(host.resource_read(handle, 4, &ctx).unwrap().bytes, b"ping");
    host.resource_close(handle, &ctx).unwrap();
    server.join().unwrap();
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    match value {
        Value::Map(map) => map.get(name).unwrap_or_else(|| panic!("missing `{name}`")),
        other => panic!("expected a Map, got {other:?}"),
    }
}

#[test]
fn resource_requests_are_project_only_and_grant_checked() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut pure = StdHost::new(SandboxMode::Pure, Capabilities::pure(), dir.path());
    let error = pure
        .resource_request("fs.exists", vec![Value::text("out")], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);

    let mut project = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let error = project
        .resource_request("fs.mkdir", vec![Value::text("out")], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
}

#[test]
fn fs_requests_create_list_rename_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    let capabilities = Capabilities::project()
        .allow_read("out")
        .allow_read("out/**")
        .allow_write("out")
        .allow_write("out/**");
    let mut host = StdHost::new(SandboxMode::Project, capabilities, dir.path());
    let ctx = context();

    let missing = host
        .resource_request("fs.exists", vec![Value::text("out")], &ctx)
        .unwrap();
    assert_eq!(field(&missing, "value"), &Value::Bool(false));

    let created = host
        .resource_request("fs.mkdir", vec![Value::text("out")], &ctx)
        .unwrap();
    assert_eq!(field(&created, "ok"), &Value::Bool(true));

    std::fs::write(dir.path().join("out/a.txt"), b"hello").unwrap();
    let metadata = host
        .resource_request("fs.metadata", vec![Value::text("out/a.txt")], &ctx)
        .unwrap();
    assert_eq!(
        field(&metadata, "value"),
        &Value::Map({
            let mut map = std::collections::BTreeMap::new();
            map.insert("kind".to_string(), Value::text("file"));
            map.insert("size".to_string(), Value::int(5));
            map
        })
    );

    let listed = host
        .resource_request("fs.list", vec![Value::text("out")], &ctx)
        .unwrap();
    assert_eq!(
        field(&listed, "value"),
        &Value::List(vec![Value::text("a.txt")])
    );

    let renamed = host
        .resource_request(
            "fs.rename",
            vec![Value::text("out/a.txt"), Value::text("out/b.txt")],
            &ctx,
        )
        .unwrap();
    assert_eq!(field(&renamed, "ok"), &Value::Bool(true));
    assert!(!dir.path().join("out/a.txt").exists());
    assert!(dir.path().join("out/b.txt").exists());

    let removed = host
        .resource_request("fs.remove", vec![Value::text("out/b.txt")], &ctx)
        .unwrap();
    assert_eq!(field(&removed, "ok"), &Value::Bool(true));
    let removed_dir = host
        .resource_request("fs.remove", vec![Value::text("out")], &ctx)
        .unwrap();
    assert_eq!(field(&removed_dir, "ok"), &Value::Bool(true));
    assert!(!dir.path().join("out").exists());
}

#[test]
fn fs_requests_reject_traversal_before_touching_the_filesystem() {
    let dir = tempfile::tempdir().unwrap();
    let capabilities = Capabilities::project()
        .allow_write("../**")
        .allow_read("../**");
    let mut host = StdHost::new(SandboxMode::Project, capabilities, dir.path());
    let ctx = context();
    let error = host
        .resource_request("fs.mkdir", vec![Value::text("../escape")], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
}

#[test]
fn clock_now_is_project_only_and_returns_unix_millis() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut pure = StdHost::new(SandboxMode::Pure, Capabilities::pure(), dir.path());
    let error = pure
        .resource_request("clock.now", Vec::new(), &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);

    let mut project = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let value = project
        .resource_request("clock.now", Vec::new(), &ctx)
        .unwrap();
    assert_eq!(field(&value, "ok"), &Value::Bool(true));
    let Value::Int(millis) = field(&value, "value") else {
        panic!("clock.now did not return an Int");
    };
    let millis = millis.to_string().parse::<i64>().expect("i64 millis");
    assert!(millis > 1_600_000_000_000);

    let error = project
        .resource_request("clock.now", vec![Value::text("x")], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::Arity);
}

#[test]
fn env_get_requires_an_exact_grant_and_reports_missing_names() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut denied = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let error = denied
        .resource_request("env.get", vec![Value::text("PATH")], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);

    let mut pure = StdHost::new(SandboxMode::Pure, Capabilities::pure(), dir.path());
    let error = pure
        .resource_request("env.get", vec![Value::text("PATH")], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);

    let capabilities = Capabilities::project()
        .allow_env("PATH")
        .allow_env("MARQDO_TEST_MISSING_ENV");
    let mut host = StdHost::new(SandboxMode::Project, capabilities, dir.path());
    let present = host
        .resource_request("env.get", vec![Value::text("PATH")], &ctx)
        .unwrap();
    assert_eq!(field(&present, "ok"), &Value::Bool(true));

    let missing = host
        .resource_request(
            "env.get",
            vec![Value::text("MARQDO_TEST_MISSING_ENV")],
            &ctx,
        )
        .unwrap();
    assert_eq!(field(&missing, "ok"), &Value::Bool(false));
    assert_eq!(field(&missing, "code"), &Value::text("env.not_found"));

    let error = host
        .resource_request("env.get", Vec::new(), &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::Arity);
}

#[cfg(unix)]
#[test]
fn process_run_requires_an_exact_program_grant() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut denied = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let error = denied
        .resource_request(
            "process.run",
            vec![
                Value::text("/bin/echo"),
                Value::List(vec![Value::text("x")]),
            ],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);

    let mut pure = StdHost::new(SandboxMode::Pure, Capabilities::pure(), dir.path());
    let error = pure
        .resource_request(
            "process.run",
            vec![
                Value::text("/bin/echo"),
                Value::List(vec![Value::text("x")]),
            ],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);

    let capabilities = Capabilities::project().allow_process("/bin/echo");
    let mut host = StdHost::new(SandboxMode::Project, capabilities, dir.path());
    let result = host
        .resource_request(
            "process.run",
            vec![
                Value::text("/bin/echo"),
                Value::List(vec![Value::text("hello")]),
            ],
            &ctx,
        )
        .unwrap();
    assert_eq!(field(&result, "ok"), &Value::Bool(true));
    let value = field(&result, "value");
    assert_eq!(field(value, "status"), &Value::int(0));
    assert_eq!(field(value, "stdout"), &Value::text("hello\n"));
    assert_eq!(field(value, "stderr"), &Value::text(""));

    let missing = host
        .resource_request(
            "process.run",
            vec![
                Value::text("/bin/definitely-not-a-program"),
                Value::List(Vec::new()),
            ],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(missing.category, ErrorCategory::PermissionDenied);
}

#[test]
fn fs_append_read_at_and_walk_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let capabilities = Capabilities::project()
        .allow_read("out")
        .allow_read("out/**")
        .allow_write("out")
        .allow_write("out/**");
    let mut host = StdHost::new(SandboxMode::Project, capabilities, dir.path());
    let ctx = context();
    host.resource_request("fs.mkdir", vec![Value::text("out")], &ctx)
        .unwrap();
    let appended = host
        .resource_request(
            "fs.append",
            vec![Value::text("out/data.txt"), Value::text("hello")],
            &ctx,
        )
        .unwrap();
    assert_eq!(field(&appended, "value"), &Value::int(5));
    host.resource_request(
        "fs.append",
        vec![Value::text("out/data.txt"), Value::text(" world")],
        &ctx,
    )
    .unwrap();
    let slice = host
        .resource_request(
            "fs.read_at",
            vec![Value::text("out/data.txt"), Value::int(6), Value::int(5)],
            &ctx,
        )
        .unwrap();
    assert_eq!(field(&slice, "value"), &Value::text("world"));
    let walked = host
        .resource_request("fs.walk", vec![Value::text("out")], &ctx)
        .unwrap();
    assert_eq!(
        field(&walked, "value"),
        &Value::List(vec![Value::text("data.txt")])
    );
    let error = host
        .resource_request(
            "fs.read_at",
            vec![Value::text("out/data.txt"), Value::int(-1), Value::int(1)],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::Type);
}

#[test]
fn random_bytes_require_the_entropy_grant() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut denied = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let error = denied
        .resource_request("random.bytes", vec![Value::int(8)], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);

    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_random(),
        dir.path(),
    );
    let value = host
        .resource_request("random.bytes", vec![Value::int(16)], &ctx)
        .unwrap();
    assert_eq!(field(&value, "ok"), &Value::Bool(true));
    let bytes = field(&value, "value").clone();
    let Value::List(bytes) = bytes else {
        panic!("random.bytes did not return a List");
    };
    assert_eq!(bytes.len(), 16);
    for byte in bytes {
        let Value::Int(byte) = byte else {
            panic!("random byte is not an Int");
        };
        let byte = byte.to_string().parse::<i64>().unwrap();
        assert!((0..=255).contains(&byte));
    }

    let error = host
        .resource_request("random.bytes", vec![Value::int(0)], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::Type);

    let mut pure = StdHost::new(
        SandboxMode::Pure,
        Capabilities::pure().allow_random(),
        dir.path(),
    );
    let error = pure
        .resource_request("random.bytes", vec![Value::int(8)], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
}

#[test]
fn udp_resource_round_trips_a_datagram_with_a_network_grant() {
    use std::net::UdpSocket;

    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let mut buffer = [0u8; 16];
        let (count, peer) = server.recv_from(&mut buffer).unwrap();
        server.send_to(&buffer[..count], peer).unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_net("127.0.0.1"),
        dir.path(),
    );
    let ctx = context();
    let handle = handle(
        host.resource_open(
            "udp",
            &format!("127.0.0.1:{port}"),
            &["read".to_string(), "write".to_string()],
            &ctx,
        )
        .unwrap(),
    );
    assert_eq!(host.resource_write(handle, b"ping", &ctx).unwrap(), 4);
    let read = host.resource_read(handle, 16, &ctx).unwrap();
    assert_eq!(read.bytes, b"ping");
    assert!(!read.eof);
    host.resource_close(handle, &ctx).unwrap();
    server.join().unwrap();
}

#[test]
fn udp_is_project_only_and_grant_checked() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    for mode in [SandboxMode::Pure, SandboxMode::Strict] {
        let mut host = StdHost::new(
            mode,
            Capabilities::project().allow_net("127.0.0.1"),
            dir.path(),
        );
        let error = host
            .resource_open(
                "udp",
                "127.0.0.1:9",
                &["read".to_string(), "write".to_string()],
                &ctx,
            )
            .unwrap_err();
        assert_eq!(error.category, ErrorCategory::PermissionDenied);
    }
    let mut project = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let error = project
        .resource_open(
            "udp",
            "127.0.0.1:9",
            &["read".to_string(), "write".to_string()],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
}

#[test]
fn tcp_listener_accepts_an_inbound_connection() {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_net("127.0.0.1"),
        dir.path(),
    );
    let ctx = context();
    let listener = handle(
        host.resource_open(
            "tcp-listener",
            &format!("127.0.0.1:{port}"),
            &["accept".to_string()],
            &ctx,
        )
        .unwrap(),
    );
    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
        stream.write_all(b"ping").unwrap();
        let mut buffer = [0u8; 4];
        stream.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"ping");
    });

    let accepted = host.resource_accept(listener, &ctx).unwrap();
    let value = field(&accepted, "value");
    let Value::Resource(connection) = value else {
        panic!("resource.accept did not return a Resource");
    };
    assert_eq!(
        host.resource_read(*connection, 4, &ctx).unwrap().bytes,
        b"ping"
    );
    assert_eq!(host.resource_write(*connection, b"ping", &ctx).unwrap(), 4);
    host.resource_close(*connection, &ctx).unwrap();
    host.resource_close(listener, &ctx).unwrap();
    client.join().unwrap();
}

#[test]
fn tcp_listener_requires_accept_rights_and_project_mode() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut project = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_net("127.0.0.1"),
        dir.path(),
    );
    let error = project
        .resource_open("tcp-listener", "127.0.0.1:9", &["read".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::ResourceUnsupported);
    let mut strict = StdHost::new(
        SandboxMode::Strict,
        Capabilities::project().allow_net("127.0.0.1"),
        dir.path(),
    );
    let error = strict
        .resource_open("tcp-listener", "127.0.0.1:9", &["accept".to_string()], &ctx)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
}

#[test]
fn tls_resource_round_trips_with_an_explicit_root() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let certificate = certified.cert.der().clone();
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
        certified.key_pair.serialize_der(),
    ));
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate.clone()], key)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let connection = rustls::ServerConnection::new(Arc::new(config)).unwrap();
        let mut stream = rustls::StreamOwned::new(connection, stream);
        let mut buffer = [0u8; 4];
        stream.read_exact(&mut buffer).unwrap();
        stream.write_all(&buffer).unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_net("localhost"),
        dir.path(),
    )
    .with_tls_ca(certificate);
    let ctx = context();
    let handle = handle(
        host.resource_open(
            "tls",
            &format!("localhost:{port}"),
            &["read".to_string(), "write".to_string()],
            &ctx,
        )
        .unwrap(),
    );
    assert_eq!(host.resource_write(handle, b"ping", &ctx).unwrap(), 4);
    assert_eq!(host.resource_read(handle, 4, &ctx).unwrap().bytes, b"ping");
    host.resource_close(handle, &ctx).unwrap();
    server.join().unwrap();
}

#[test]
fn tls_requires_project_mode_and_a_host_grant() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context();
    let mut project = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let error = project
        .resource_open(
            "tls",
            "localhost:9",
            &["read".to_string(), "write".to_string()],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
    let mut strict = StdHost::new(
        SandboxMode::Strict,
        Capabilities::project().allow_net("localhost"),
        dir.path(),
    );
    let error = strict
        .resource_open(
            "tls",
            "localhost:9",
            &["read".to_string(), "write".to_string()],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
}

#[test]
fn compression_round_trips_and_rejects_invalid_gzip() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let ctx = context();
    let compressed = host
        .resource_request("compress.gzip", vec![Value::text("hello 世界")], &ctx)
        .unwrap();
    assert_eq!(field(&compressed, "ok"), &Value::Bool(true));
    let bytes = field(&compressed, "value").clone();
    let decompressed = host
        .resource_request("compress.gunzip", vec![bytes], &ctx)
        .unwrap();
    assert_eq!(field(&decompressed, "value"), &Value::text("hello 世界"));

    let invalid = host
        .resource_request(
            "compress.gunzip",
            vec![Value::List(vec![Value::int(1), Value::int(2)])],
            &ctx,
        )
        .unwrap();
    assert_eq!(field(&invalid, "ok"), &Value::Bool(false));
    assert_eq!(field(&invalid, "code"), &Value::text("compress.invalid"));
}

#[test]
fn regex_operations_support_groups_alternation_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let ctx = context();
    let matched = host
        .resource_request(
            "regex.match",
            vec![Value::text("^(a+)(b|c)$"), Value::text("aab")],
            &ctx,
        )
        .unwrap();
    assert_eq!(field(&matched, "value"), &Value::Bool(true));
    let captures = host
        .resource_request(
            "regex.captures",
            vec![Value::text("^(a+)(b|c)$"), Value::text("aab")],
            &ctx,
        )
        .unwrap();
    let value = field(&captures, "value");
    assert_eq!(field(value, "matched"), &Value::Bool(true));
    assert_eq!(
        field(value, "groups"),
        &Value::List(vec![
            Value::text("aab"),
            Value::text("aa"),
            Value::text("b")
        ])
    );
    let invalid = host
        .resource_request(
            "regex.match",
            vec![Value::text("["), Value::text("a")],
            &ctx,
        )
        .unwrap();
    assert_eq!(field(&invalid, "ok"), &Value::Bool(false));
    assert_eq!(field(&invalid, "code"), &Value::text("regex.invalid"));
}
