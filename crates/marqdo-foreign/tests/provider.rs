//! External kernel-provider registration (ADR-0012).

use std::time::Duration;

use marqdo_foreign::{
    call, call_with_registry, ForeignError, KernelDescriptor, KernelRegistry, RunRequest, RunStatus,
};
use marqdo_value::Value;

fn request(language: &str, source: &str) -> RunRequest {
    RunRequest {
        language: language.to_string(),
        source: source.to_string(),
        input: None,
        timeout: Duration::from_secs(5),
        max_output_bytes: 64 * 1024,
    }
}

#[cfg(unix)]
fn mock_provider(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("mocklang.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"type\":\"int\",\"decimal\":\"42\"}'\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script, permissions).unwrap();
    script
}

#[cfg(unix)]
#[test]
fn external_provider_is_registered_without_core_changes() {
    let dir = tempfile::tempdir().unwrap();
    let script = mock_provider(dir.path());
    let mut registry = KernelRegistry::builtin();
    registry
        .register(KernelDescriptor::external(
            "mocklang",
            &["mocklang", "mock"],
            &script,
        ))
        .unwrap();
    assert_eq!(registry.descriptor("MOCK").unwrap().provider_id, "mocklang");
    assert!(registry.detect("mocklang").is_some());

    let result = call_with_registry(&request("mocklang", "ignored source"), &registry).unwrap();
    assert_eq!(result.status, RunStatus::Success);
    assert_eq!(result.value, Some(Value::Int(42.into())));

    let unavailable = call(&request("mocklang", "ignored source")).unwrap_err();
    assert!(matches!(
        unavailable,
        ForeignError::KernelUnavailable { .. }
    ));
}

#[cfg(unix)]
#[test]
fn duplicate_language_labels_and_wire_versions_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let script = mock_provider(dir.path());
    let mut registry = KernelRegistry::builtin();
    let mut descriptor = KernelDescriptor::external("mocklang", &["python"], &script);
    assert!(registry.register(descriptor.clone()).is_err());

    descriptor.languages = vec!["mocklang".into()];
    descriptor.wire_version = 99;
    assert!(registry.register(descriptor).is_err());
}

#[cfg(unix)]
#[test]
fn external_provider_timeout_is_enforced() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("slow.sh");
    std::fs::write(&script, "#!/bin/sh\nsleep 5\n").unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script, permissions).unwrap();

    let mut registry = KernelRegistry::builtin();
    registry
        .register(KernelDescriptor::external(
            "slowlang",
            &["slowlang"],
            &script,
        ))
        .unwrap();
    let mut request = request("slowlang", "");
    request.timeout = Duration::from_millis(50);
    let error = call_with_registry(&request, &registry).unwrap_err();
    assert!(matches!(error, ForeignError::Timeout { .. }));
    // The built-in path remains unchanged.
}

#[cfg(unix)]
#[test]
fn provider_digest_is_verified_at_registration_and_execution() {
    use sha2::{Digest, Sha256};

    let dir = tempfile::tempdir().unwrap();
    let script = mock_provider(dir.path());
    let digest = {
        let mut hasher = Sha256::new();
        hasher.update(std::fs::read(&script).unwrap());
        hex::encode(hasher.finalize())
    };
    let mut registry = KernelRegistry::builtin();
    let mut descriptor = KernelDescriptor::external("mocklang", &["mocklang", "mock"], &script);
    descriptor.digest = Some(digest.clone());
    descriptor.environment_id = Some("env-test".to_string());
    registry.register(descriptor).unwrap();
    assert_eq!(
        registry
            .descriptor("mocklang")
            .unwrap()
            .environment_id
            .as_deref(),
        Some("env-test")
    );
    let result = call_with_registry(&request("mocklang", "ignored"), &registry).unwrap();
    assert_eq!(result.status, RunStatus::Success);

    // Tampering after registration must be caught before spawn.
    std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
    let error = call_with_registry(&request("mocklang", "ignored"), &registry).unwrap_err();
    assert!(matches!(error, ForeignError::Integrity { .. }), "{error:?}");

    // Malformed digests and relative external entrypoints are rejected.
    let mut bad = KernelDescriptor::external("bad", &["bad"], "relative.sh");
    bad.digest = Some("not-a-digest".to_string());
    assert!(registry.register(bad).is_err());
}
