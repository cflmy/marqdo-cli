#![cfg(unix)]

use std::path::Path;
use std::process::Command;

use marqdo_host::{Capabilities, Host, StdHost};
use marqdo_sandbox::SandboxMode;
use marqdo_value::{ErrorCategory, Value};

fn context() -> marqdo_host::ExecutionContext {
    marqdo_host::ExecutionContext::new("test".to_string(), None)
}

fn handle(value: Value) -> marqdo_value::ResourceHandle {
    match value {
        Value::Resource(handle) => handle,
        other => panic!("expected resource, got {other:?}"),
    }
}

fn cc_available() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn build_provider(dir: &Path) -> std::path::PathBuf {
    let source = dir.join("provider.c");
    std::fs::write(
        &source,
        r#"
#include <stdint.h>
#include <stddef.h>
#include <string.h>
static const char DATA[] = "worker-abi";
static size_t position = 0;
uint32_t marqdo_resource_provider_abi_version(void) { return 1u; }
int32_t marqdo_resource_open(const char *kind, const char *descriptor,
                             const char *rights_csv, uint64_t *out_token) {
    (void)kind; (void)descriptor; (void)rights_csv;
    position = 0; *out_token = 42; return 0;
}
int32_t marqdo_resource_read(uint64_t token, uint8_t *buffer,
                             uint32_t capacity, uint32_t *out_read,
                             int32_t *out_eof) {
    (void)token;
    size_t remaining = sizeof(DATA) - 1 - position;
    size_t count = remaining < capacity ? remaining : capacity;
    memcpy(buffer, DATA + position, count);
    position += count;
    *out_read = (uint32_t)count;
    *out_eof = position >= sizeof(DATA) - 1 ? 1 : 0;
    return 0;
}
int32_t marqdo_resource_write(uint64_t token, const uint8_t *bytes,
                              uint32_t length, uint32_t *out_written) {
    (void)token; (void)bytes; *out_written = length; return 0;
}
int32_t marqdo_resource_close(uint64_t token) { (void)token; return 0; }
int32_t marqdo_resource_last_error(char *buffer, uint32_t capacity) {
    (void)capacity; buffer[0] = 0; return 0;
}
"#,
    )
    .unwrap();
    let extension = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let library = dir.join(format!("libworker_provider.{extension}"));
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-O2", "-o"])
        .arg(&library)
        .arg(&source)
        .status()
        .expect("run cc");
    assert!(status.success());
    library
}

#[test]
fn c_abi_provider_runs_inside_the_worker_helper() {
    if !cc_available() {
        eprintln!("skipping worker binding test: cc is unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let library = build_provider(dir.path());
    let project = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("worker-cabi"),
        project.path(),
    );
    host.register_worker_resource_provider(
        "worker-cabi",
        &["worker.sensor"],
        &std::path::PathBuf::from(env!("CARGO_BIN_EXE_marqdo-provider-worker")),
        &[library.to_string_lossy().to_string()],
    )
    .unwrap();
    let ctx = context();
    let handle = handle(
        host.resource_open("worker.sensor", "s", &["read".to_string()], &ctx)
            .unwrap(),
    );
    let first = host.resource_read(handle, 6, &ctx).unwrap();
    assert_eq!(first.bytes, b"worker");
    let rest = host.resource_read(handle, 64, &ctx).unwrap();
    assert_eq!(rest.bytes, b"-abi");
    assert!(rest.eof);
    assert_eq!(host.resource_write(handle, b"x", &ctx).unwrap(), 1);
    host.resource_close(handle, &ctx).unwrap();
}

#[test]
fn worker_helper_without_c_provider_fails_closed() {
    let project = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("worker-cabi"),
        project.path(),
    );
    let error = host
        .register_worker_resource_provider(
            "worker-cabi",
            &["worker.sensor"],
            &std::path::PathBuf::from(env!("CARGO_BIN_EXE_marqdo-provider-worker")),
            &["/definitely/missing/provider.so".to_string()],
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::HostError);
}
