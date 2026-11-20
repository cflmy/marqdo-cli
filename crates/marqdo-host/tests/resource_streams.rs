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
fn strict_mode_refuses_every_resource_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(SandboxMode::Strict, Capabilities::pure(), dir.path());
    let error = host
        .resource_open("stdio", "stdout", &["write".to_string()], &context())
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::PermissionDenied);
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
        .invoke(HostOp::Print, vec![Value::Resource(handle)], &ctx)
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
