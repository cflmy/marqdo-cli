//! Extension selection identities (RFC-EXT-0001 §6, ADR-0056).
//!
//! `marqdo.yaml.extensions` declares activation sources; the lock file records
//! the exact selected identity (manifest digest, artifact digest, target).
//! This module computes those identities from static manifests only:
//!
//! * it never `dlopen`s a library, spawns a worker or runs a script;
//! * it reads the declared artifact SHA-256 (plus the manifest bytes) so
//!   `check`/`sync` can compare against the lock without requiring a built
//!   binary; `run` separately verifies the actual artifact bytes before use.
//!
//! A declaration is visibility/activation, never a grant. Grants stay in
//! `--allow-plugin`/`--allow-provider`/`--allow-foreign` and the capability
//! broker.

use std::collections::BTreeSet;

use marqdo_package::{ExtensionKind, LockedExtension, ResolvedExtension};
use sha2::{Digest, Sha256};

use crate::extension_descriptors::PrepareError;

/// Compute the declared identity of every resolved extension source.
///
/// Duplicate `(kind, name, logical_owner)` identities with different digests
/// are rejected deterministically; the same identity declared twice is
/// idempotent.
pub fn extension_identities(
    resolved: &[ResolvedExtension],
) -> Result<Vec<LockedExtension>, PrepareError> {
    let mut identities = Vec::new();
    let mut seen: BTreeSet<(ExtensionKind, String, String)> = BTreeSet::new();
    for extension in resolved {
        let identity = match extension.kind {
            ExtensionKind::Plugin => plugin_identity(extension)?,
            ExtensionKind::Provider => provider_identity(extension)?,
            ExtensionKind::Kernel => kernel_identity(extension)?,
        };
        let key = (
            identity.kind,
            identity.name.clone(),
            identity.logical_owner.clone(),
        );
        if !seen.insert(key.clone()) {
            let existing = identities
                .iter()
                .find(|item: &&LockedExtension| {
                    item.kind == key.0 && item.name == key.1 && item.logical_owner == key.2
                })
                .expect("identity was inserted with its key");
            if existing == &identity {
                continue;
            }
            return Err(PrepareError::option(
                "ext.duplicate_callable",
                format!(
                    "extension {} `{}` for owner `{}` is declared with two different manifests",
                    key.0, key.1, key.2
                ),
            ));
        }
        identities.push(identity);
    }
    identities.sort_by(|left, right| {
        (&left.kind, &left.name, &left.logical_owner).cmp(&(
            &right.kind,
            &right.name,
            &right.logical_owner,
        ))
    });
    Ok(identities)
}

fn plugin_identity(extension: &ResolvedExtension) -> Result<LockedExtension, PrepareError> {
    let manifest_bytes = std::fs::read(&extension.path).map_err(|error| {
        PrepareError::option(
            "ext.manifest_unreadable",
            format!(
                "cannot read plugin manifest `{}`: {error}",
                extension.path.display()
            ),
        )
    })?;
    let manifest = marqdo_plugin_abi::PluginManifest::load(&extension.path)
        .map_err(|error| PrepareError::option(error.code.clone(), error.message.clone()))?;
    manifest
        .validate()
        .map_err(|error| PrepareError::option(error.code.clone(), error.message.clone()))?;
    let host = marqdo_plugin_abi::HostTarget::current();
    let artifact = manifest
        .resolve_artifact(&host)
        .map_err(|error| PrepareError::option("ext.no_matching_artifact", error.message))?;
    Ok(LockedExtension {
        kind: ExtensionKind::Plugin,
        logical_owner: extension.logical_owner.clone(),
        name: manifest.plugin.name.clone(),
        version: manifest.plugin.version.clone(),
        manifest_digest: hex(&Sha256::digest(&manifest_bytes)),
        artifact_digest: artifact.sha256.clone(),
        selected_target: host.triple().to_string(),
        source: extension.from.clone(),
    })
}

fn provider_identity(extension: &ResolvedExtension) -> Result<LockedExtension, PrepareError> {
    let manifest_bytes = std::fs::read(&extension.path).map_err(|error| {
        PrepareError::option(
            "ext.manifest_unreadable",
            format!(
                "cannot read provider manifest `{}`: {error}",
                extension.path.display()
            ),
        )
    })?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        PrepareError::option(
            "ext.manifest_unreadable",
            format!(
                "invalid provider manifest `{}`: {error}",
                extension.path.display()
            ),
        )
    })?;
    if manifest.get("schema").and_then(serde_json::Value::as_str) != Some("marqdo.provider") {
        return Err(PrepareError::option(
            "ext.manifest_invalid",
            format!(
                "`{}` must declare schema `marqdo.provider`",
                extension.path.display()
            ),
        ));
    }
    if manifest
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
    {
        return Err(PrepareError::option(
            "ext.unsupported_provider_schema",
            format!(
                "`{}` is not a provider schema version 2 manifest; run `marqdo lock` only with v2",
                extension.path.display()
            ),
        ));
    }
    let name = manifest
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            PrepareError::option(
                "ext.manifest_invalid",
                format!("`{}` has no name", extension.path.display()),
            )
        })?;
    let version = manifest
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            PrepareError::option(
                "ext.manifest_invalid",
                format!("`{}` has no version", extension.path.display()),
            )
        })?;
    let target = marqdo_plugin_abi::HostTarget::current()
        .triple()
        .to_string();
    let artifacts = manifest
        .get("artifacts")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            PrepareError::option(
                "ext.manifest_invalid",
                format!("`{}` has no artifacts array", extension.path.display()),
            )
        })?;
    let artifact = artifacts
        .iter()
        .find(|artifact| {
            artifact.get("target").and_then(serde_json::Value::as_str) == Some(&target)
        })
        .ok_or_else(|| {
            PrepareError::option(
                "ext.no_matching_artifact",
                format!("provider `{name}` has no artifact for target `{target}`"),
            )
        })?;
    let sha256 = artifact
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            PrepareError::option(
                "ext.manifest_invalid",
                format!("provider `{name}` artifact has no sha256"),
            )
        })?;
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PrepareError::option(
            "ext.manifest_invalid",
            format!("provider `{name}` artifact sha256 is not 64 hex characters"),
        ));
    }
    Ok(LockedExtension {
        kind: ExtensionKind::Provider,
        logical_owner: extension.logical_owner.clone(),
        name: name.to_string(),
        version: version.to_string(),
        manifest_digest: hex(&Sha256::digest(&manifest_bytes)),
        artifact_digest: sha256.to_ascii_lowercase(),
        selected_target: target,
        source: extension.from.clone(),
    })
}

fn kernel_identity(extension: &ResolvedExtension) -> Result<LockedExtension, PrepareError> {
    let manifest_bytes = std::fs::read(&extension.path).map_err(|error| {
        PrepareError::option(
            "ext.manifest_unreadable",
            format!(
                "cannot read kernel descriptor `{}`: {error}",
                extension.path.display()
            ),
        )
    })?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        PrepareError::option(
            "ext.manifest_unreadable",
            format!(
                "invalid kernel descriptor `{}`: {error}",
                extension.path.display()
            ),
        )
    })?;
    let name = manifest
        .get("provider_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            PrepareError::option(
                "ext.manifest_invalid",
                format!("`{}` has no provider_id", extension.path.display()),
            )
        })?;
    let wire_version = manifest
        .get("wire_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            PrepareError::option(
                "ext.manifest_invalid",
                format!("`{}` has no wire_version", extension.path.display()),
            )
        })?;
    if wire_version != 1 {
        return Err(PrepareError::option(
            "ext.manifest_invalid",
            format!("kernel descriptor `{name}` uses unsupported wire_version {wire_version}"),
        ));
    }
    let digest = manifest
        .get("digest")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            PrepareError::option(
                "ext.manifest_invalid",
                format!("kernel descriptor `{name}` has no digest"),
            )
        })?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PrepareError::option(
            "ext.manifest_invalid",
            format!("kernel descriptor `{name}` digest is not 64 hex characters"),
        ));
    }
    let entrypoint = manifest
        .get("entrypoint")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            PrepareError::option(
                "ext.manifest_invalid",
                format!("kernel descriptor `{name}` has no entrypoint"),
            )
        })?;
    // The descriptor's entrypoint is package/descriptor-relative or absolute
    // (existing foreign.md contract). Reject explicit path escaping here.
    if entrypoint.is_empty() {
        return Err(PrepareError::option(
            "ext.manifest_invalid",
            format!("kernel descriptor `{name}` has an empty entrypoint"),
        ));
    }
    Ok(LockedExtension {
        kind: ExtensionKind::Kernel,
        logical_owner: extension.logical_owner.clone(),
        name: name.to_string(),
        version: format!("wire:{wire_version}"),
        manifest_digest: hex(&Sha256::digest(&manifest_bytes)),
        artifact_digest: digest.to_ascii_lowercase(),
        selected_target: marqdo_plugin_abi::HostTarget::current()
            .triple()
            .to_string(),
        source: extension.from.clone(),
    })
}

/// Validate that the lock still describes the current declared selection.
pub fn validate_against_lock(
    lock: &marqdo_package::LockFile,
    identities: &[LockedExtension],
) -> Result<(), PrepareError> {
    lock.validate_extension_selection(identities)
        .map_err(|message| PrepareError::load("ext.extension_lock_mismatch", message))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
