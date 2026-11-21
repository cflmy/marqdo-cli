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
fn tcp_requires_network_grant_and_rejects_dns_names() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    let ctx = context();
    let denied = host
        .resource_open(
            "tcp",
            "127.0.0.1:9",
            &["read".to_string(), "write".to_string()],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(denied.category, ErrorCategory::PermissionDenied);
    let dns = host
        .resource_open(
            "tcp",
            "example.test:80",
            &["read".to_string(), "write".to_string()],
            &ctx,
        )
        .unwrap_err();
    assert_eq!(dns.category, ErrorCategory::ResourceUnsupported);

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
