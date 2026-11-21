//! Trusted capability-handler registry (RFC-EXT-0001 §7, ADR-0057).
//!
//! These are end-to-end tests over the public `marqdo-host` API. They cover the
//! CAP-01..CAP-12 checks listed in RFC-EXT-0001 §7.5: policy vs handler,
//! package binding, registration validation, real `fs.read` scope enforcement,
//! reply budgets, forged/replayed handles, audit hygiene, resource-stream
//! regression, project-vs-strict labeling and the "no handler is never a
//! silent OK" rule.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use marqdo_host::{
    contains_plugin_handle, Budget, Capabilities, CapabilityBroker, CapabilityHandler,
    CapabilityRequest, PluginHandleTable, ProjectFs, ResourceTable, SandboxMode, StdHost,
    CORE_CAPABILITIES,
};
use marqdo_plugin_abi::PluginError;
use marqdo_value::{RuntimeError, Value};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "marqdo-capability-handlers-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A trusted test handler that returns canned bytes.
struct FixedHandler {
    name: &'static str,
    reply: Vec<u8>,
}

impl CapabilityHandler for FixedHandler {
    fn name(&self) -> &str {
        self.name
    }

    fn execute(
        &mut self,
        _request: &CapabilityRequest<'_>,
        _capabilities: &Capabilities,
        _budget: &mut Budget,
    ) -> Result<Vec<u8>, PluginError> {
        Ok(self.reply.clone())
    }
}

/// A trusted test handler that proves the request identity and budget reach it.
struct EchoHandler {
    name: &'static str,
    fuel: u64,
}

impl CapabilityHandler for EchoHandler {
    fn name(&self) -> &str {
        self.name
    }

    fn execute(
        &mut self,
        request: &CapabilityRequest<'_>,
        _capabilities: &Capabilities,
        budget: &mut Budget,
    ) -> Result<Vec<u8>, PluginError> {
        budget
            .consume_fuel(self.fuel, None)
            .map_err(map_runtime_error)?;
        Ok(format!(
            "{}|{}|{}",
            request.package_instance, request.capability, request.trace_id
        )
        .into_bytes())
    }
}

fn map_runtime_error(error: RuntimeError) -> PluginError {
    PluginError::capability(error.code(), error.message)
}

fn audit_entries(broker: &CapabilityBroker) -> Vec<marqdo_host::CapabilityAudit> {
    let audit = broker.audit();
    let guard = audit.lock().expect("audit lock");
    guard.clone()
}

/// CAP-01: a declared vendor capability without a trusted policy entry is
/// denied, even when a handler happens to be installed.
#[test]
fn cap_01_vendor_capability_without_policy_is_denied() {
    let mut broker = CapabilityBroker::new(Capabilities::project());
    broker
        .register_handler(FixedHandler {
            name: "vendor.device.read",
            reply: b"device".to_vec(),
        })
        .unwrap();

    let error = broker
        .execute("demo.solver", "vendor.device.read", b"{}")
        .unwrap_err();
    assert_eq!(error.code, "permission.denied", "{error:?}");
    assert!(broker
        .authorize("demo.solver", "vendor.device.read", b"{}")
        .is_err());

    let audit = audit_entries(&broker);
    assert_eq!(audit.len(), 2);
    assert!(audit.iter().all(|entry| entry.decision == "denied"));
    assert!(audit
        .iter()
        .all(|entry| entry.capability == "vendor.device.read"));
}

/// CAP-02: authorized but no handler installed is `capability.unavailable`;
/// `authorize` alone does not execute and cannot fake an empty success.
#[test]
fn cap_02_authorized_capability_without_handler_is_unavailable() {
    let mut broker = CapabilityBroker::new(Capabilities::project().allow_write("out/**"));
    broker.allow_vendor("vendor.device.read");

    let unavailable = broker
        .execute("demo.solver", "vendor.device.read", b"{}")
        .unwrap_err();
    assert_eq!(
        unavailable.code, "capability.unavailable",
        "{unavailable:?}"
    );

    // Policy is a grant, not an executor: authorize() succeeds, execute() still
    // refuses because no trusted handler was registered.
    assert!(broker
        .authorize(
            "demo.solver",
            "fs.write",
            br#"{"path":"out/result.txt","contents":"x"}"#,
        )
        .is_ok());
    let write = broker
        .execute(
            "demo.solver",
            "fs.write",
            br#"{"path":"out/result.txt","contents":"x"}"#,
        )
        .unwrap_err();
    assert_eq!(write.code, "capability.unavailable", "{write:?}");

    let audit = audit_entries(&broker);
    assert!(audit
        .iter()
        .any(|entry| entry.decision == "unavailable" && entry.capability == "fs.write"));
    assert!(audit
        .iter()
        .any(|entry| entry.decision == "granted" && entry.capability == "fs.write"));
}

/// CAP-03: a handler bound to one trusted package rejects cross-package
/// impersonation, while the bound package still executes and the request
/// identity/trace id reach the handler.
#[test]
fn cap_03_package_bound_handler_rejects_impersonation() {
    let mut broker = CapabilityBroker::new(Capabilities::project()).with_trace_id("trace-9");
    broker.allow_vendor("vendor.device.read");
    broker
        .register_package_handler(
            "demo.solver",
            EchoHandler {
                name: "vendor.device.read",
                fuel: 0,
            },
        )
        .unwrap();

    let spoofed = broker
        .execute("attacker.solver", "vendor.device.read", b"{}")
        .unwrap_err();
    assert_eq!(spoofed.code, "permission.denied", "{spoofed:?}");

    let reply = broker
        .execute("demo.solver", "vendor.device.read", b"{}")
        .unwrap();
    assert_eq!(
        String::from_utf8(reply).unwrap(),
        "demo.solver|vendor.device.read|trace-9"
    );

    let audit = audit_entries(&broker);
    assert!(audit
        .iter()
        .any(|entry| entry.decision == "denied" && entry.package == "attacker.solver"));
    assert!(audit
        .iter()
        .any(|entry| entry.decision == "granted" && entry.package == "demo.solver"));
}

/// CAP-04: duplicate, reserved, empty and malformed handler names are rejected
/// and never installed.
#[test]
fn cap_04_registration_rejects_duplicate_reserved_and_empty_names() {
    let mut broker = CapabilityBroker::new(Capabilities::project());
    let rejected = |broker: &mut CapabilityBroker, name: &'static str, expected: &str| {
        let error = broker
            .register_handler(FixedHandler {
                name,
                reply: Vec::new(),
            })
            .unwrap_err();
        assert_eq!(error.code, expected, "name `{name}`: {error:?}");
        assert!(!broker.has_handler(name));
    };

    rejected(&mut broker, "", "capability.handler_invalid");
    rejected(&mut broker, "host.terminal", "capability.handler_reserved");
    rejected(
        &mut broker,
        "marqdo.internal.read",
        "capability.handler_reserved",
    );
    rejected(
        &mut broker,
        "not a capability",
        "capability.handler_invalid",
    );
    rejected(&mut broker, "vendor..read", "capability.handler_invalid");

    broker
        .register_handler(FixedHandler {
            name: "vendor.device.read",
            reply: b"device".to_vec(),
        })
        .unwrap();
    let duplicate = broker
        .register_handler(FixedHandler {
            name: "vendor.device.read",
            reply: b"other".to_vec(),
        })
        .unwrap_err();
    assert_eq!(
        duplicate.code, "capability.handler_conflict",
        "{duplicate:?}"
    );

    // The trusted built-in fs.read handler cannot be replaced either.
    let dir = TempDir::new();
    let mut with_fs =
        CapabilityBroker::new(Capabilities::project()).with_fs(ProjectFs::new(dir.path()));
    assert!(with_fs.has_handler("fs.read"));
    let conflict = with_fs
        .register_handler(FixedHandler {
            name: "fs.read",
            reply: Vec::new(),
        })
        .unwrap_err();
    assert_eq!(conflict.code, "capability.handler_conflict", "{conflict:?}");
}

/// CAP-05: the real `fs.read` handler enforces ProjectFs root/scope limits; a
/// permissive glob grant cannot be used to read outside the project root or to
/// traverse with `..`.
#[test]
fn cap_05_fs_read_handler_enforces_scope() {
    let dir = TempDir::new();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(dir.path().join("data/allowed.txt"), "allowed").unwrap();
    std::fs::create_dir_all(dir.path().join("private")).unwrap();
    std::fs::write(dir.path().join("private/secret.txt"), "secret").unwrap();

    let mut broker = CapabilityBroker::new(Capabilities::project().allow_read("data/**"))
        .with_fs(ProjectFs::new(dir.path()));
    let allowed = broker
        .execute("demo.solver", "fs.read", br#"{"path":"data/allowed.txt"}"#)
        .unwrap();
    assert_eq!(String::from_utf8(allowed).unwrap(), "allowed");

    for out_of_scope in [
        br#"{"path":"private/secret.txt"}"#.as_slice(),
        br#"{"path":"../escape.txt"}"#.as_slice(),
        br#"{"path":"/etc/passwd"}"#.as_slice(),
    ] {
        let error = broker
            .execute("demo.solver", "fs.read", out_of_scope)
            .unwrap_err();
        assert_eq!(
            error.code, "permission.denied",
            "{out_of_scope:?}: {error:?}"
        );
    }

    // Even a `**` grant only authorizes project-relative paths: traversal and
    // absolute paths are rejected by ProjectFs::resolve at the handler.
    let mut permissive = CapabilityBroker::new(Capabilities::project().allow_read("**"))
        .with_fs(ProjectFs::new(dir.path()));
    for rejected in [
        br#"{"path":"data/../private/secret.txt"}"#.as_slice(),
        br#"{"path":"/etc/passwd"}"#.as_slice(),
    ] {
        let error = permissive
            .execute("demo.solver", "fs.read", rejected)
            .unwrap_err();
        assert_eq!(error.code, "permission.denied", "{rejected:?}: {error:?}");
    }
}

/// CAP-06: a handler reply over the broker budget is a resource limit, and the
/// handler receives (and charges) the broker budget.
#[test]
fn cap_06_handler_reply_over_budget_is_resource_limit() {
    let mut broker = CapabilityBroker::new(Capabilities::project())
        .with_reply_limit(64)
        .with_budget(Budget {
            fuel: 10,
            ..Budget::default()
        });
    broker.allow_vendor("vendor.device.big");
    broker.allow_vendor("vendor.device.fuel");
    broker
        .register_handler(FixedHandler {
            name: "vendor.device.big",
            reply: vec![b'x'; 65],
        })
        .unwrap();
    broker
        .register_handler(EchoHandler {
            name: "vendor.device.fuel",
            fuel: 4,
        })
        .unwrap();

    let oversized = broker
        .execute("demo.solver", "vendor.device.big", b"{}")
        .unwrap_err();
    assert_eq!(oversized.code, "resource.exhausted", "{oversized:?}");

    let reply = broker
        .execute("demo.solver", "vendor.device.fuel", b"{}")
        .unwrap();
    assert_eq!(
        String::from_utf8(reply).unwrap(),
        "demo.solver|vendor.device.fuel|"
    );
    assert_eq!(broker.budget().remaining_fuel(), 6);

    let audit = audit_entries(&broker);
    assert!(audit.iter().all(|entry| entry.duration_micros <= 5_000_000));
    assert!(audit
        .iter()
        .any(|entry| entry.capability == "vendor.device.big" && entry.decision == "denied"));
}

/// CAP-07: forged plugin handles are rejected by the host table, and a handle
/// hidden inside an untrusted extension value is detected.
#[test]
fn cap_07_forged_handles_are_rejected() {
    let mut table = PluginHandleTable::default();
    let handle = table.mint("demo.capability", "capability.read", "demo.capability", 1);
    table
        .validate(&handle, "demo.capability", 1)
        .expect("host-minted handle is valid");

    let mut forged = handle.clone();
    forged.id = handle.id.wrapping_add(1000);
    let error = table
        .validate(&forged, "demo.capability", 1)
        .expect_err("forged id must be rejected");
    assert!(
        error.message.contains("plugin_handle.untrusted"),
        "{error:?}"
    );

    let wrong_owner = table
        .validate(&handle, "other.package", 1)
        .expect_err("cross-owner handle must be rejected");
    assert!(
        wrong_owner.message.contains("plugin_handle.untrusted"),
        "{wrong_owner:?}"
    );

    let untrusted_value = Value::Map(
        [("handle".to_string(), Value::PluginHandle(handle.clone()))]
            .into_iter()
            .collect(),
    );
    assert!(contains_plugin_handle(&untrusted_value));
}

/// CAP-08: a handle from an old run or a cleared (unloaded) run is rejected on
/// replay.
#[test]
fn cap_08_replayed_handle_after_run_end_is_rejected() {
    let mut table = PluginHandleTable::default();
    let handle = table.mint("demo.capability", "capability.read", "demo.capability", 7);
    table
        .validate(&handle, "demo.capability", 7)
        .expect("current run accepts its own handle");

    let new_run = table
        .validate(&handle, "demo.capability", 8)
        .expect_err("a new run must not accept the old handle");
    assert!(
        new_run.message.contains("plugin_handle.untrusted"),
        "{new_run:?}"
    );

    table.clear_run(7);
    assert!(table.is_empty());
    let replayed = table
        .validate(&handle, "demo.capability", 7)
        .expect_err("a cleared run must reject the replayed handle");
    assert!(
        replayed.message.contains("plugin_handle.untrusted"),
        "{replayed:?}"
    );
}

/// CAP-09: the capability audit records decisions and durations but never file
/// contents or request secrets.
#[test]
fn cap_09_audit_contains_no_file_contents_or_secrets() {
    const SECRET: &str = "TOP-SECRET-PAYLOAD";
    const API_KEY: &str = "sk-live-not-audited";
    let dir = TempDir::new();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(dir.path().join("data/secret.txt"), SECRET).unwrap();

    let mut broker = CapabilityBroker::new(Capabilities::project().allow_read("data/**"))
        .with_fs(ProjectFs::new(dir.path()));
    let request = format!(r#"{{"path":"data/secret.txt","api_key":"{API_KEY}"}}"#);
    let contents = broker
        .execute("demo.solver", "fs.read", request.as_bytes())
        .unwrap();
    assert_eq!(String::from_utf8(contents).unwrap(), SECRET);

    let denied = broker
        .execute("demo.solver", "fs.read", br#"{"path":"private/key.txt"}"#)
        .unwrap_err();
    assert_eq!(denied.code, "permission.denied");

    let audit_debug = format!("{:?}", audit_entries(&broker));
    assert!(!audit_debug.contains(SECRET), "{audit_debug}");
    assert!(!audit_debug.contains(API_KEY), "{audit_debug}");

    let audit = audit_entries(&broker);
    assert!(audit
        .iter()
        .any(|entry| entry.decision == "granted" && entry.capability == "fs.read"));
    assert!(audit
        .iter()
        .any(|entry| entry.decision == "denied" && entry.capability == "fs.read"));
    assert!(audit.iter().all(|entry| entry.duration_micros <= 5_000_000));
}

/// CAP-10: low-level resource open/read/write/close stays on a sane performance
/// path. This is a catastrophic-regression guard with a deliberately generous
/// bound, not a benchmark (CI timeouts remain authoritative).
#[test]
fn cap_10_resource_stream_operations_do_not_regress_catastrophically() {
    const CHUNK: usize = 256 * 1024;
    const ITERATIONS: usize = 32;
    let payload = vec![b'x'; CHUNK];
    let mut table = ResourceTable::default();

    let started = Instant::now();
    for _ in 0..ITERATIONS {
        let reader = table.open_file("data/blob.bin", payload.clone());
        let read = table.read(reader, payload.len(), None).unwrap();
        assert_eq!(read.bytes.len(), CHUNK);
        assert!(read.eof);
        table.close(reader, None).unwrap();

        let writer = table.open_file_writer("out/blob.bin");
        assert_eq!(table.write(writer, &payload, None).unwrap(), CHUNK);
        table.close(writer, None).unwrap();
    }
    let elapsed = started.elapsed();
    // 32 x (256 KiB read + 256 KiB write) is ~16 MiB of in-memory copying.
    assert!(
        elapsed < Duration::from_secs(5),
        "resource open/read/write/close took {elapsed:?} for {ITERATIONS} iterations"
    );
}

/// CAP-11: the default `project` mode is not an OS-strict mode and grants
/// nothing beyond `pure` by default.
#[test]
fn cap_11_default_project_mode_is_not_os_strict() {
    let dir = TempDir::new();
    let host = StdHost::new(SandboxMode::Project, Capabilities::project(), dir.path());
    assert_eq!(host.mode, SandboxMode::Project);
    assert_ne!(host.mode, SandboxMode::Strict);
    assert_eq!(host.mode.as_str(), "project");

    // Project defaults equal pure defaults: no implicit fs/process/plugin grant.
    assert!(host.capabilities.fs_read.is_empty());
    assert!(host.capabilities.fs_write.is_empty());
    assert!(!host.capabilities.process);
    assert!(!host.capabilities.foreign);
    assert!(host.capabilities.plugins.is_empty());

    // Strict isolation availability is a property of `strict`; project mode is
    // cooperative in-process checks, whether or not the platform could isolate.
    assert_eq!(
        SandboxMode::parse(host.mode.as_str()),
        Some(SandboxMode::Project)
    );
}

/// CAP-12: core capabilities without an executing handler are always
/// `capability.unavailable`, never a silent empty success.
#[test]
fn cap_12_core_capabilities_without_handler_are_unavailable() {
    let dir = TempDir::new();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(dir.path().join("data/ok.txt"), "ok").unwrap();

    let mut capabilities = Capabilities::project()
        .allow_read("data/**")
        .allow_write("data/**")
        .allow_net("127.0.0.1")
        .allow_process("/bin/true");
    capabilities.process = true;
    let mut broker = CapabilityBroker::new(capabilities).with_fs(ProjectFs::new(dir.path()));

    // The one migrated capability has a real handler.
    let read = broker
        .execute("demo.solver", "fs.read", br#"{"path":"data/ok.txt"}"#)
        .unwrap();
    assert_eq!(String::from_utf8(read).unwrap(), "ok");

    // Every other core capability is authorized by its grant but has no
    // executor in this build.
    for name in CORE_CAPABILITIES {
        if name == "fs.read" {
            continue;
        }
        let payload: &[u8] = match name {
            "fs.write" => br#"{"path":"data/x","contents":"x"}"#,
            "net.request" => br#"{"host":"127.0.0.1"}"#,
            _ => b"{}",
        };
        let error = broker.execute("demo.solver", name, payload).unwrap_err();
        assert_eq!(
            error.code, "capability.unavailable",
            "core capability `{name}` must not silently succeed: {error:?}"
        );
        assert!(!broker.has_handler(name));
    }

    let unknown = broker
        .execute("demo.solver", "teleport", b"{}")
        .unwrap_err();
    assert_eq!(unknown.code, "capability.unknown", "{unknown:?}");

    let audit = audit_entries(&broker);
    assert!(
        audit
            .iter()
            .filter(|entry| entry.decision == "unavailable")
            .count()
            >= CORE_CAPABILITIES.len() - 1,
        "{audit:?}"
    );
}
