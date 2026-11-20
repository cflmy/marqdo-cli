#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

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

fn cc_available() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn build_provider_with_version(dir: &Path, version: u32) -> PathBuf {
    let source = dir.join("provider.c");
    std::fs::write(
        &source,
        r#"
#include <stdint.h>
#include <stddef.h>
#include <string.h>

static const char DATA[] = "c-abi";
static size_t position = 0;
static uint64_t next_token = 1;

#ifndef PROVIDER_VERSION
#define PROVIDER_VERSION 1u
#endif
uint32_t marqdo_resource_provider_abi_version(void) { return PROVIDER_VERSION; }

int32_t marqdo_resource_open(const char *kind, const char *descriptor,
                             const char *rights_csv, uint64_t *out_token) {
    (void)kind; (void)descriptor; (void)rights_csv;
    position = 0;
    *out_token = next_token++;
    return 0;
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
    (void)token; (void)bytes;
    *out_written = length;
    return 0;
}

int32_t marqdo_resource_close(uint64_t token) { (void)token; return 0; }

int32_t marqdo_resource_last_error(char *buffer, uint32_t capacity) {
    if (capacity > 0) buffer[0] = 0;
    return 0;
}
"#,
    )
    .unwrap();
    let extension = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let library = dir.join(format!("libecho_provider.{extension}"));
    let status = Command::new("cc")
        .args([
            "-shared",
            "-fPIC",
            "-O2",
            &format!("-DPROVIDER_VERSION={version}"),
            "-o",
        ])
        .arg(&library)
        .arg(&source)
        .status()
        .expect("run cc");
    assert!(status.success(), "C provider build failed");
    library
}

fn build_provider(dir: &Path) -> PathBuf {
    build_provider_with_version(dir, 1)
}

#[test]
fn c_abi_provider_round_trips() {
    if !cc_available() {
        eprintln!("skipping C provider test: cc is unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let library = build_provider(dir.path());
    let project = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("cabi"),
        project.path(),
    );
    host.register_c_resource_provider("cabi", &["cabi.sensor"], &library)
        .unwrap();
    let ctx = context();
    let handle = handle(
        host.resource_open("cabi.sensor", "sensor-1", &["read".to_string()], &ctx)
            .unwrap(),
    );
    let first = host.resource_read(handle, 2, &ctx).unwrap();
    assert_eq!(first.bytes, b"c-");
    assert!(!first.eof);
    let rest = host.resource_read(handle, 16, &ctx).unwrap();
    assert_eq!(rest.bytes, b"abi");
    assert!(rest.eof);
    assert_eq!(host.resource_write(handle, b"xyz", &ctx).unwrap(), 3);
    host.resource_close(handle, &ctx).unwrap();
    assert_eq!(
        host.resource_close(handle, &ctx).unwrap_err().code(),
        "resource.closed"
    );
}

#[test]
fn c_abi_provider_loading_fails_closed() {
    let project = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("cabi"),
        project.path(),
    );
    let error = host
        .register_c_resource_provider(
            "cabi",
            &["cabi.sensor"],
            Path::new("/definitely/missing/marqdo-provider.so"),
        )
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::KernelUnavailable);
}

#[test]
fn c_abi_version_mismatch_and_modes_fail_closed() {
    if !cc_available() {
        eprintln!("skipping C provider test: cc is unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let wrong = build_provider_with_version(dir.path(), 2);
    let project = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("cabi"),
        project.path(),
    );
    let error = host
        .register_c_resource_provider("cabi", &["cabi.sensor"], &wrong)
        .unwrap_err();
    assert_eq!(error.category, ErrorCategory::ResourceUnsupported);

    let good = build_provider(dir.path());
    for mode in [SandboxMode::Pure, SandboxMode::Strict] {
        let mut host = StdHost::new(
            mode,
            Capabilities::project().allow_resource_provider("cabi"),
            project.path(),
        );
        host.register_c_resource_provider("cabi", &["cabi.sensor"], &good)
            .unwrap();
        let error = host
            .resource_open("cabi.sensor", "s", &["read".to_string()], &context())
            .unwrap_err();
        assert_eq!(error.category, ErrorCategory::PermissionDenied);
    }
}

#[test]
fn provider_package_manifest_installs_a_c_abi_library() {
    if !cc_available() {
        eprintln!("skipping C provider test: cc is unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let library = build_provider(dir.path());
    let manifest = dir.path().join("provider.json");
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "default"
    };
    std::fs::write(
        &manifest,
        serde_json::json!({
            "schema": "marqdo.provider",
            "schema_version": 1,
            "name": "cabi",
            "kinds": ["cabi.sensor"],
            "abi": "c-abi-1",
            "library": { platform: library.file_name().unwrap().to_string_lossy() }
        })
        .to_string(),
    )
    .unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::project().allow_resource_provider("cabi"),
        project.path(),
    );
    host.register_provider_package(&manifest).unwrap();
    let ctx = context();
    let handle = handle(
        host.resource_open("cabi.sensor", "s", &["read".to_string()], &ctx)
            .unwrap(),
    );
    let read = host.resource_read(handle, 2, &ctx).unwrap();
    assert_eq!(read.bytes, b"c-");
}
