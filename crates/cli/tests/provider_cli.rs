//! PR-03 black-box CLI tests for third-party Resource Provider v2
//! (RFC-EXT-0001 §5, ADR-0055).
//!
//! These tests drive the real `marqdo` binary (`CARGO_BIN_EXE_marqdo`) against
//! a real `provider.json` v2 bundle, a real `c-abi-1` C sensor library compiled
//! with `cc`, and the real `marqdo-provider-worker` helper. No host API is
//! called directly: this is the same path a user gets from the public CLI.
//!
//! The helper is a separate release artifact that cargo does not build for
//! `-p marqdo-cli`; [`worker_helper`] builds it with cargo if it is missing and
//! copies it next to the CLI executable, which is exactly where production
//! resolution expects it (a sibling of the running `marqdo`, never PATH).
//!
//! Linux x86_64 is the mandatory provider-worker lane (RFC-EXT-0001 §5.6).
//! Other platforms must keep this file compiling but run no provider
//! assertions; the CLI itself fails closed with `ext.platform_unsupported`.
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use marqdo_plugin_abi::HostTarget;
use sha2::{Digest, Sha256};

const MARKER_ENV: &str = "MARQDO_TEST_PROVIDER_MARKER";

/// C `c-abi-1` provider: `demo.sensor` yields `hello` (5 bytes),
/// `demo.actuator` yields `xy` (2 bytes). It appends `worker-started` when the
/// worker loads the library, so tests can prove a worker was (or was not)
/// spawned for an unauthorized/invalid provider.
const SENSOR_C: &str = r#"
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    uint64_t token;
    size_t pos;
    const char *data;
    size_t len;
    int closed;
} Stream;

static Stream streams[64];
static int stream_count = 0;

static void mark(const char *what) {
    const char *path = getenv("MARQDO_TEST_PROVIDER_MARKER");
    if (!path || !*path) return;
    FILE *file = fopen(path, "a");
    if (!file) return;
    fprintf(file, "%s\n", what);
    fclose(file);
}

uint32_t marqdo_resource_provider_abi_version(void) {
    mark("worker-started");
    return 1u;
}

int32_t marqdo_resource_open(const char *kind, const char *descriptor,
                             const char *rights_csv, uint64_t *out_token) {
    (void)descriptor;
    (void)rights_csv;
    if (stream_count >= 64) return 1;
    Stream *stream = &streams[stream_count];
    memset(stream, 0, sizeof(*stream));
    if (strcmp(kind, "demo.sensor") == 0) {
        stream->data = "hello";
        stream->len = 5;
    } else if (strcmp(kind, "demo.actuator") == 0) {
        stream->data = "xy";
        stream->len = 2;
    } else {
        return 1;
    }
    stream->token = (uint64_t)(stream_count + 1);
    stream_count += 1;
    mark("open");
    *out_token = stream->token;
    return 0;
}

int32_t marqdo_resource_read(uint64_t token, uint8_t *buffer, uint32_t capacity,
                             uint32_t *out_read, int32_t *out_eof) {
    for (int i = 0; i < stream_count; i++) {
        Stream *stream = &streams[i];
        if (stream->token != token || stream->closed) continue;
        size_t remaining = stream->len - stream->pos;
        size_t count = remaining < capacity ? remaining : capacity;
        memcpy(buffer, stream->data + stream->pos, count);
        stream->pos += count;
        *out_read = (uint32_t)count;
        *out_eof = stream->pos >= stream->len ? 1 : 0;
        return 0;
    }
    return 1;
}

int32_t marqdo_resource_write(uint64_t token, const uint8_t *bytes,
                              uint32_t length, uint32_t *out_written) {
    (void)token;
    (void)bytes;
    *out_written = length;
    return 0;
}

int32_t marqdo_resource_close(uint64_t token) {
    for (int i = 0; i < stream_count; i++) {
        if (streams[i].token == token) {
            streams[i].closed = 1;
            mark("close");
            return 0;
        }
    }
    return 1;
}

int32_t marqdo_resource_last_error(char *buffer, uint32_t capacity) {
    if (buffer && capacity) buffer[0] = 0;
    return 0;
}
"#;

const READ_DOC: &str = r#"## @exec main

`内存 = memory.new(1, 1)`

`流 = resource.open("demo.sensor", "unit-1", ["read"])`

`读取 = resource.read(流, 内存, 0, 5)`

`关闭 = resource.close(流)`

`return {count: 读取.value.count, eof: 读取.value.eof, closed: 关闭.ok, data: 内存[0]}`
"#;

const TWO_KINDS_DOC: &str = r#"## @exec main

`内存 = memory.new(1, 1)`

`传感器 = resource.open("demo.sensor", "unit-1", ["read"])`

`执行器 = resource.open("demo.actuator", "unit-2", ["read"])`

`第一次 = resource.read(传感器, 内存, 0, 5)`

`第二次 = resource.read(执行器, 内存, 0, 2)`

`关闭传感器 = resource.close(传感器)`

`关闭执行器 = resource.close(执行器)`

`return [第一次.value.count, 第二次.value.count, 关闭传感器.ok, 关闭执行器.ok]`
"#;

const TRIVIAL_DOC: &str = "## @exec main\n\n`return 1`\n";

fn cli_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_marqdo"))
}

fn profile_dir() -> PathBuf {
    cli_binary()
        .parent()
        .expect("the CLI binary has a parent directory")
        .to_path_buf()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/cli has a parent")
        .parent()
        .expect("crates has a parent")
        .to_path_buf()
}

fn worker_file_name() -> String {
    format!("marqdo-provider-worker{}", std::env::consts::EXE_SUFFIX)
}

/// Build `marqdo-provider-worker` if needed and install it next to the CLI.
///
/// Production resolution is only "a sibling of the current `marqdo` binary";
/// the test therefore performs the same installation a release tarball would
/// and never weakens the resolver with an environment override or PATH lookup.
fn worker_helper() -> PathBuf {
    static INSTALLED: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    match INSTALLED.get_or_init(install_worker_helper) {
        Ok(path) => path.clone(),
        Err(error) => panic!("cannot build the provider worker helper: {error}"),
    }
}

fn install_worker_helper() -> Result<PathBuf, String> {
    let sibling = profile_dir().join(worker_file_name());
    if sibling.is_file() {
        return Ok(sibling);
    }
    let repo = repo_root();
    let target_dir = repo.join("target").join("provider-worker-helper");
    let (profile, release_flag) = match profile_dir().file_name().and_then(|name| name.to_str()) {
        Some("release") => ("release", Some("--release")),
        _ => ("debug", None),
    };
    let mut last_error = String::new();
    // A dedicated target directory avoids contending with the cargo
    // invocation that is already running this test binary.
    for offline in [true, false] {
        let mut command = Command::new(
            std::env::var_os("CARGO").unwrap_or_else(|| std::ffi::OsString::from("cargo")),
        );
        command
            .current_dir(&repo)
            .args([
                "build",
                "--package",
                "marqdo-provider-worker",
                "--bin",
                "marqdo-provider-worker",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(&target_dir);
        if let Some(flag) = release_flag {
            command.arg(flag);
        }
        if offline {
            command.arg("--offline");
        }
        let output = command
            .output()
            .map_err(|error| format!("cannot run cargo: {error}"))?;
        if !output.status.success() {
            last_error = String::from_utf8_lossy(&output.stderr)
                .lines()
                .rev()
                .take(6)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            continue;
        }
        let mut executable: Option<PathBuf> = None;
        for line in output.stdout.split(|byte| *byte == b'\n') {
            let Ok(message) = serde_json::from_slice::<serde_json::Value>(line) else {
                continue;
            };
            let Some(path) = message.get("executable").and_then(|value| value.as_str()) else {
                continue;
            };
            if Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy() == worker_file_name())
                .unwrap_or(false)
            {
                executable = Some(PathBuf::from(path));
            }
        }
        let built = executable.unwrap_or_else(|| target_dir.join(profile).join(worker_file_name()));
        if !built.is_file() {
            last_error = format!("cargo succeeded but `{}` is missing", built.display());
            continue;
        }
        fs::copy(&built, &sibling).map_err(|error| {
            format!(
                "cannot install `{}` as the CLI sibling `{}`: {error}",
                built.display(),
                sibling.display()
            )
        })?;
        return Ok(sibling);
    }
    Err(last_error)
}

/// One provider bundle under a throw-away directory.
struct Fixture {
    dir: PathBuf,
    lib_relative: String,
    lib_sha256: String,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "marqdo-provider-cli-{}-{}-{tag}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create fixture directory");
        let lib_relative = "lib/libdemo_sensor.so".to_string();
        let source = path.join("sensor.c");
        fs::write(&source, SENSOR_C).expect("write sensor.c");
        let library = path.join(&lib_relative);
        fs::create_dir_all(library.parent().expect("lib dir")).expect("create lib dir");
        let status = Command::new("cc")
            .args(["-shared", "-fPIC", "-O2", "-o"])
            .arg(&library)
            .arg(&source)
            .status()
            .expect("run cc");
        assert!(status.success(), "cc could not build the test provider");
        let sha256 = sha256_hex(&fs::read(&library).expect("read provider library"));
        Fixture {
            dir: path,
            lib_relative,
            lib_sha256: sha256,
        }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directory");
        }
        fs::write(&path, contents).expect("write fixture file");
        path
    }

    fn project(&self, document: &str) -> PathBuf {
        self.write("index.mq.md", document)
    }

    fn manifest(&self, name: &str, kinds: &[&str]) -> String {
        self.manifest_with(
            name,
            kinds,
            &self.lib_relative,
            &self.lib_sha256,
            &HostTarget::current().triple().to_string(),
        )
    }

    fn manifest_with(
        &self,
        name: &str,
        kinds: &[&str],
        artifact_relative: &str,
        sha256: &str,
        target: &str,
    ) -> String {
        serde_json::json!({
            "schema": "marqdo.provider",
            "schema_version": 2,
            "name": name,
            "version": "0.1.0",
            "abi": "c-abi-1",
            "transport": "worker",
            "kinds": kinds,
            "artifacts": [{
                "target": target,
                "path": artifact_relative,
                "sha256": sha256,
            }],
            "permissions_requested": [],
        })
        .to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn marker_path(dir: &Path) -> PathBuf {
    dir.join("provider-marker.txt")
}

fn marker_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn run_cli(dir: &Path, args: &[&str], marker: &Path) -> Output {
    Command::new(cli_binary())
        .args(args)
        .current_dir(dir)
        .env(MARKER_ENV, marker)
        .output()
        .unwrap_or_else(|error| panic!("cannot run marqdo {args:?}: {error}"))
}

fn json_of(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "expected one JSON document, got {:?} (stderr {:?}): {error}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn error_code(report: &serde_json::Value) -> String {
    report
        .get("error")
        .and_then(|error| error.get("code"))
        .or_else(|| report.get("code"))
        .and_then(|code| code.as_str())
        .unwrap_or("<missing error code>")
        .to_string()
}

fn map_field<'a>(report: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    &report["value"]["entries"][name]
}

fn cc_available() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Every fixture test needs `cc`; this mirrors the existing CLI/worker tests.
macro_rules! require_cc {
    () => {
        if !cc_available() {
            eprintln!("UNSUPPORTED: `cc` is unavailable, cannot build the c-abi-1 test provider");
            return;
        }
    };
}

#[test]
fn prov_01_authorized_v2_worker_reads_five_bytes() {
    require_cc!();
    let helper = worker_helper();
    assert!(helper.is_file(), "worker helper must be a real executable");
    let fixture = Fixture::new("prov-01");
    fixture.project(READ_DOC);
    let manifest = fixture.write(
        "provider.json",
        &fixture.manifest("demo.sensor", &["demo.sensor"]),
    );
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            manifest.to_str().expect("utf-8 manifest path"),
            "--allow-provider",
            "demo.sensor",
            "--json",
        ],
        &marker,
    );
    let report = json_of(&output);
    assert_eq!(
        output.status.code(),
        Some(0),
        "run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(report["success"].as_bool().unwrap_or(false));
    assert_eq!(map_field(&report, "count")["decimal"], "5");
    assert!(map_field(&report, "eof")["value"]
        .as_bool()
        .unwrap_or(false));
    assert!(map_field(&report, "closed")["value"]
        .as_bool()
        .unwrap_or(false));
    // `hello` is in the Memory region the provider filled on the first byte.
    assert_eq!(map_field(&report, "data")["decimal"], "104");
    assert_eq!(report["stdout"], "");
    // The Host recorded the real resource effect (not a synthetic success).
    let trace = report["trace"].as_array().expect("trace array");
    assert!(
        trace
            .iter()
            .any(|entry| entry["op"] == "resource.open" && entry["result"] == "ok"),
        "missing resource.open trace: {trace:?}"
    );
    let marks = marker_text(&marker);
    assert_eq!(
        marks
            .lines()
            .filter(|line| *line == "worker-started")
            .count(),
        1,
        "exactly one worker must start: {marks:?}"
    );
    assert!(marks.lines().any(|line| line == "open"), "{marks:?}");
    assert_eq!(
        marks.lines().filter(|line| *line == "close").count(),
        1,
        "close must happen exactly once: {marks:?}"
    );
}

#[test]
fn prov_02_without_grant_is_permission_denied_and_never_starts_the_worker() {
    require_cc!();
    let _ = worker_helper();
    let fixture = Fixture::new("prov-02");
    fixture.project(READ_DOC);
    let manifest = fixture.write(
        "provider.json",
        &fixture.manifest("demo.sensor", &["demo.sensor"]),
    );
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            manifest.to_str().expect("utf-8 manifest path"),
            "--json",
        ],
        &marker,
    );
    assert_eq!(output.status.code(), Some(1));
    let report = json_of(&output);
    assert!(!report["success"].as_bool().unwrap_or(true));
    assert_eq!(error_code(&report), "permission.denied");
    assert!(
        !marker.exists(),
        "an unauthorized provider worker was started: {:?}",
        marker_text(&marker)
    );
}

#[test]
fn prov_03_grant_without_manifest_is_never_discovered() {
    require_cc!();
    let fixture = Fixture::new("prov-03");
    fixture.project(TRIVIAL_DOC);
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--allow-provider",
            "demo.sensor",
            "--json",
        ],
        &marker,
    );
    assert_eq!(output.status.code(), Some(2));
    let report = json_of(&output);
    assert_eq!(error_code(&report), "ext.provider_grant_unmatched");
    assert!(
        !marker.exists(),
        "a provider was discovered without --provider"
    );
}

#[test]
fn prov_04_duplicate_kind_across_providers_is_rejected_before_spawn() {
    require_cc!();
    let _ = worker_helper();
    let fixture = Fixture::new("prov-04");
    fixture.project(TRIVIAL_DOC);
    let first = fixture.write(
        "first.json",
        &fixture.manifest("demo.one", &["demo.sensor"]),
    );
    let second = fixture.write(
        "second.json",
        &fixture.manifest("demo.two", &["demo.sensor"]),
    );
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            first.to_str().expect("utf-8 path"),
            "--provider",
            second.to_str().expect("utf-8 path"),
            "--allow-provider",
            "demo.one",
            "--allow-provider",
            "demo.two",
            "--json",
        ],
        &marker,
    );
    assert_eq!(output.status.code(), Some(2));
    let report = json_of(&output);
    assert_eq!(error_code(&report), "ext.duplicate_provider_kind");
    assert!(
        !marker.exists(),
        "a duplicate kind must fail before any worker starts"
    );
}

#[test]
fn prov_05_one_provider_serves_two_non_reserved_kinds() {
    require_cc!();
    let _ = worker_helper();
    let fixture = Fixture::new("prov-05");
    fixture.project(TWO_KINDS_DOC);
    let manifest = fixture.write(
        "provider.json",
        &fixture.manifest("demo.multi", &["demo.sensor", "demo.actuator"]),
    );
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            manifest.to_str().expect("utf-8 manifest path"),
            "--allow-provider",
            "demo.multi",
            "--json",
        ],
        &marker,
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json_of(&output);
    let items = report["value"]["items"].as_array().expect("list result");
    assert_eq!(items[0]["decimal"], "5");
    assert_eq!(items[1]["decimal"], "2");
    assert!(items[2]["value"].as_bool().unwrap_or(false));
    assert!(items[3]["value"].as_bool().unwrap_or(false));
    let marks = marker_text(&marker);
    assert_eq!(
        marks
            .lines()
            .filter(|line| *line == "worker-started")
            .count(),
        1,
        "one provider (one worker) serves both kinds: {marks:?}"
    );
    assert_eq!(
        marks.lines().filter(|line| *line == "open").count(),
        2,
        "{marks:?}"
    );
    assert_eq!(
        marks.lines().filter(|line| *line == "close").count(),
        2,
        "{marks:?}"
    );
}

#[test]
fn prov_06_reserved_core_kinds_are_rejected() {
    require_cc!();
    let _ = worker_helper();
    for kind in ["stdio", "file", "tcp", "tls", "udp", "tcp-listener"] {
        let fixture = Fixture::new("prov-06");
        fixture.project(TRIVIAL_DOC);
        let manifest = fixture.write("provider.json", &fixture.manifest("demo.reserved", &[kind]));
        let marker = marker_path(fixture.path());
        let output = run_cli(
            fixture.path(),
            &[
                "run",
                "index.mq.md",
                "--provider",
                manifest.to_str().expect("utf-8 manifest path"),
                "--allow-provider",
                "demo.reserved",
                "--json",
            ],
            &marker,
        );
        assert_eq!(output.status.code(), Some(2), "kind {kind}");
        let report = json_of(&output);
        assert_eq!(error_code(&report), "ext.manifest_invalid", "kind {kind}");
        assert!(
            !marker.exists(),
            "reserved kind {kind} must not start a worker"
        );
    }
}

#[test]
fn prov_07_digest_symlink_and_traversal_fail_before_spawn() {
    require_cc!();
    let _ = worker_helper();

    // (a) content digest mismatch.
    let fixture = Fixture::new("prov-07-digest");
    fixture.project(TRIVIAL_DOC);
    let wrong_sha = "0".repeat(64);
    let digest_manifest = fixture.write(
        "provider.json",
        &fixture.manifest_with(
            "demo.sensor",
            &["demo.sensor"],
            &fixture.lib_relative,
            &wrong_sha,
            &HostTarget::current().triple().to_string(),
        ),
    );
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            digest_manifest.to_str().expect("utf-8 path"),
            "--allow-provider",
            "demo.sensor",
            "--json",
        ],
        &marker,
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(error_code(&json_of(&output)), "ext.artifact_integrity");
    assert!(!marker.exists(), "digest mismatch must not start a worker");

    // (b) artifact symlink escaping the *bundle root* (the manifest parent):
    // the hash matches the real library, so only the bundle-containment check
    // can reject it.
    let fixture = Fixture::new("prov-07-symlink");
    fixture.project(TRIVIAL_DOC);
    let bundle = fixture.path().join("bundle");
    fs::create_dir_all(bundle.join("lib")).expect("create bundle dir");
    let outside = fixture.path().join("outside");
    fs::create_dir_all(&outside).expect("create outside dir");
    let outside_library = outside.join("libdemo_sensor.so");
    fs::copy(fixture.path().join(&fixture.lib_relative), &outside_library)
        .expect("copy library outside the bundle");
    std::os::unix::fs::symlink(&outside_library, bundle.join(&fixture.lib_relative))
        .expect("create escaping symlink");
    let symlink_manifest = bundle.join("provider.json");
    fs::write(
        &symlink_manifest,
        fixture.manifest("demo.sensor", &["demo.sensor"]),
    )
    .expect("write bundle manifest");
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            symlink_manifest.to_str().expect("utf-8 path"),
            "--allow-provider",
            "demo.sensor",
            "--json",
        ],
        &marker,
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(error_code(&json_of(&output)), "ext.artifact_integrity");
    assert!(!marker.exists(), "symlink escape must not start a worker");

    // (c) `..` path traversal declared in the manifest.
    let fixture = Fixture::new("prov-07-traversal");
    fixture.project(TRIVIAL_DOC);
    let traversal_manifest = fixture.write(
        "provider.json",
        &fixture.manifest_with(
            "demo.sensor",
            &["demo.sensor"],
            "../lib/libdemo_sensor.so",
            &fixture.lib_sha256,
            &HostTarget::current().triple().to_string(),
        ),
    );
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            traversal_manifest.to_str().expect("utf-8 path"),
            "--allow-provider",
            "demo.sensor",
            "--json",
        ],
        &marker,
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(error_code(&json_of(&output)), "ext.artifact_integrity");
    assert!(!marker.exists(), "path traversal must not start a worker");
}

#[test]
fn prov_08_missing_current_target_is_no_matching_artifact() {
    require_cc!();
    let _ = worker_helper();
    let fixture = Fixture::new("prov-08");
    fixture.project(TRIVIAL_DOC);
    let manifest = fixture.write(
        "provider.json",
        &fixture.manifest_with(
            "demo.sensor",
            &["demo.sensor"],
            &fixture.lib_relative,
            &fixture.lib_sha256,
            "aarch64-unknown-linux-gnu",
        ),
    );
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            manifest.to_str().expect("utf-8 path"),
            "--allow-provider",
            "demo.sensor",
            "--json",
        ],
        &marker,
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(error_code(&json_of(&output)), "ext.no_matching_artifact");
    assert!(!marker.exists());
}

#[test]
fn prov_09_v1_manifest_is_unsupported_without_migration() {
    require_cc!();
    let _ = worker_helper();
    let fixture = Fixture::new("prov-09");
    fixture.project(TRIVIAL_DOC);
    let v1 = serde_json::json!({
        "schema": "marqdo.provider",
        "schema_version": 1,
        "name": "demo.sensor",
        "kinds": ["demo.sensor"],
        "abi": "c-abi-1",
        "library": { "linux": fixture.lib_relative },
    })
    .to_string();
    let manifest = fixture.write("provider.json", &v1);
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            manifest.to_str().expect("utf-8 path"),
            "--allow-provider",
            "demo.sensor",
            "--json",
        ],
        &marker,
    );
    assert_eq!(output.status.code(), Some(2));
    let report = json_of(&output);
    assert_eq!(error_code(&report), "ext.unsupported_provider_schema");
    assert!(
        !marker.exists(),
        "the v1 host-embedding manifest path is not reachable from the CLI"
    );
}

#[test]
fn prov_10_pure_and_strict_preflight_refuse_provider_flags() {
    require_cc!();
    let _ = worker_helper();
    let fixture = Fixture::new("prov-10");
    fixture.project(TRIVIAL_DOC);
    let manifest = fixture.write(
        "provider.json",
        &fixture.manifest("demo.sensor", &["demo.sensor"]),
    );
    let marker = marker_path(fixture.path());
    let manifest_arg = manifest.to_str().expect("utf-8 manifest path");

    let pure = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--mode",
            "pure",
            "--provider",
            manifest_arg,
            "--allow-provider",
            "demo.sensor",
            "--json",
        ],
        &marker,
    );
    assert_eq!(pure.status.code(), Some(2));
    assert_eq!(error_code(&json_of(&pure)), "cli.invalid_grants");
    assert!(
        !marker.exists(),
        "pure mode must not start a provider worker"
    );

    let strict = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--mode",
            "strict",
            "--provider",
            manifest_arg,
            "--allow-provider",
            "demo.sensor",
            "--json",
        ],
        &marker,
    );
    assert_eq!(strict.status.code(), Some(2));
    assert_eq!(error_code(&json_of(&strict)), "strict.unsupported_grant");
    assert!(
        !marker.exists(),
        "strict L1 must not start a provider worker"
    );

    // The wildcard grant is rejected too: grants are exact names.
    let wildcard = run_cli(
        fixture.path(),
        &[
            "run",
            "index.mq.md",
            "--provider",
            manifest_arg,
            "--allow-provider",
            "*",
            "--json",
        ],
        &marker,
    );
    assert_eq!(wildcard.status.code(), Some(2));
    assert_eq!(error_code(&json_of(&wildcard)), "cli.invalid_grants");
}

#[test]
fn prov_13_missing_sibling_helper_is_worker_unavailable_without_path_fallback() {
    require_cc!();
    // Install the helper next to the real CLI binary and put that directory on
    // PATH: a PATH-based resolver would now succeed, which must not happen.
    let helper = worker_helper();
    let helper_dir = helper.parent().expect("helper directory").to_path_buf();
    let fixture = Fixture::new("prov-13");
    fixture.project(READ_DOC);
    let manifest = fixture.write(
        "provider.json",
        &fixture.manifest("demo.sensor", &["demo.sensor"]),
    );
    let copied_dir = fixture.path().join("release");
    fs::create_dir_all(&copied_dir).expect("create release dir");
    let copied_cli = copied_dir.join("marqdo");
    fs::copy(cli_binary(), &copied_cli).expect("copy the CLI binary");
    let marker = marker_path(fixture.path());
    let path = format!(
        "{}:{}",
        helper_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(&copied_cli)
        .args([
            "run",
            "index.mq.md",
            "--provider",
            manifest.to_str().expect("utf-8 path"),
            "--allow-provider",
            "demo.sensor",
            "--json",
        ])
        .current_dir(fixture.path())
        .env("PATH", path)
        .env(MARKER_ENV, &marker)
        .output()
        .expect("run the relocated CLI");
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json_of(&output);
    assert_eq!(error_code(&report), "ext.worker_unavailable");
    let message = report["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("marqdo-provider-worker") && message.contains("missing"),
        "{message}"
    );
    assert!(
        !marker.exists(),
        "the helper on PATH must never be used instead of the sibling"
    );
}

#[test]
fn check_never_accepts_or_loads_provider_manifests() {
    require_cc!();
    let fixture = Fixture::new("check");
    fixture.project(TRIVIAL_DOC);
    let manifest = fixture.write(
        "provider.json",
        &fixture.manifest("demo.sensor", &["demo.sensor"]),
    );
    let marker = marker_path(fixture.path());
    let output = run_cli(
        fixture.path(),
        &[
            "check",
            "index.mq.md",
            "--provider",
            manifest.to_str().expect("utf-8 path"),
        ],
        &marker,
    );
    // `check` has no --provider option (clap usage error, exit 2) and never
    // starts a provider worker.
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unexpected argument"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.exists());
}
