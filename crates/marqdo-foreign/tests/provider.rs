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

#[cfg(unix)]
fn environment_digest(root: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.unwrap();
        if entry.file_type().is_dir() {
            continue;
        }
        let relative = entry.path().strip_prefix(root).unwrap();
        let path = relative
            .components()
            .map(|component| component.as_os_str().to_str().unwrap())
            .collect::<Vec<_>>()
            .join("/");
        files.push((path, std::fs::read(entry.path()).unwrap()));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    hasher.update(b"marqdo.kernel-environment.v1\0");
    hasher.update((files.len() as u64).to_le_bytes());
    for (path, bytes) in &files {
        hasher.update((path.len() as u64).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    hex::encode(hasher.finalize())
}

/// Write a provider executable inside `env` and return its path.
#[cfg(unix)]
fn provider_inside_environment(env: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(env.join("bin")).unwrap();
    let script = env.join("bin").join("mocklang.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'mocklang 1.0.0'; exit 0; fi\ncat >/dev/null\nprintf '%s' '{\"type\":\"int\",\"decimal\":\"42\"}'\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script, permissions).unwrap();
    script
}

#[cfg(unix)]
#[test]
fn pinned_environment_binds_the_provider_and_is_verified_before_execution() {
    let dir = tempfile::tempdir().unwrap();
    let env = dir.path().join("venv");
    let script = provider_inside_environment(&env);
    let digest = environment_digest(&env);

    let mut registry = KernelRegistry::builtin();
    let mut descriptor = KernelDescriptor::external("mocklang", &["mocklang"], &script);
    descriptor.environment_id = Some("env-test".to_string());
    descriptor.environment_digest = Some(digest);
    descriptor.environment_dir = Some(env.clone());
    registry.register(descriptor).unwrap();
    let result = call_with_registry(&request("mocklang", "ignored"), &registry).unwrap();
    assert_eq!(result.status, RunStatus::Success);

    std::fs::write(env.join("marker.txt"), "tampered").unwrap();
    let error = call_with_registry(&request("mocklang", "ignored"), &registry).unwrap_err();
    assert!(
        matches!(error, ForeignError::EnvironmentIntegrity { .. }),
        "{error:?}"
    );
}

/// A provider outside its pinned environment is a manifest error: the
/// environment would otherwise be verified while a different program runs.
#[cfg(unix)]
#[test]
fn provider_outside_pinned_environment_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let env = dir.path().join("venv");
    std::fs::create_dir_all(&env).unwrap();
    let script = mock_provider(dir.path());

    let mut registry = KernelRegistry::builtin();
    let mut descriptor = KernelDescriptor::external("mocklang", &["mocklang"], &script);
    descriptor.environment_digest = Some("0".repeat(64));
    descriptor.environment_dir = Some(env);
    let error = registry.register(descriptor).unwrap_err();
    assert!(matches!(error, ForeignError::Protocol { .. }), "{error:?}");
    assert!(error.to_string().contains("outside its pinned environment"));
}

/// A built-in interpreter cannot be silently rebound to a verified environment;
/// a pinned interpreter must be an explicit external provider.
#[cfg(unix)]
#[test]
fn builtin_provider_cannot_be_bound_to_an_environment() {
    let dir = tempfile::tempdir().unwrap();
    let env = dir.path().join("venv");
    std::fs::create_dir_all(&env).unwrap();
    let mut registry = KernelRegistry::builtin();
    let mut descriptor = KernelDescriptor::builtin("mockpython", &["mockpython"], "python3");
    descriptor.environment_digest = Some("0".repeat(64));
    descriptor.environment_dir = Some(env);
    let error = registry.register(descriptor).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot be bound to environment_dir"),
        "{error}"
    );
}

/// A declared interpreter version must match the program that actually runs;
/// environment pinning that ignores version drift is not reproducible.
#[cfg(unix)]
#[test]
fn declared_interpreter_version_must_match_the_resolved_program() {
    let dir = tempfile::tempdir().unwrap();
    let env = dir.path().join("venv");
    let script = provider_inside_environment(&env);
    let digest = environment_digest(&env);
    let mut registry = KernelRegistry::builtin();
    let mut descriptor = KernelDescriptor::external("mocklang", &["mocklang"], &script);
    descriptor.environment_digest = Some(digest);
    descriptor.environment_dir = Some(env);
    descriptor.interpreter_version = Some("9.9.9".to_string());
    registry.register(descriptor).unwrap();

    let error = call_with_registry(&request("mocklang", "ignored"), &registry).unwrap_err();
    assert!(matches!(error, ForeignError::Protocol { .. }), "{error:?}");
    assert!(error.to_string().contains("interpreter version mismatch"));

    // A matching prefix is accepted.
    let dir = tempfile::tempdir().unwrap();
    let env = dir.path().join("venv");
    let script = provider_inside_environment(&env);
    let digest = environment_digest(&env);
    let mut registry = KernelRegistry::builtin();
    let mut descriptor = KernelDescriptor::external("mocklang", &["mocklang"], &script);
    descriptor.environment_digest = Some(digest);
    descriptor.environment_dir = Some(env);
    descriptor.interpreter_version = Some("mocklang 1.0".to_string());
    registry.register(descriptor).unwrap();
    let result = call_with_registry(&request("mocklang", "ignored"), &registry).unwrap();
    assert_eq!(result.status, RunStatus::Success);
}
