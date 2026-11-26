//! Artifact target triples and content-hash verification.
//!
//! Native plugin artifacts are target-specific: the C ABI layout depends on
//! pointer width and endianness, so the host only loads an artifact built for
//! exactly its own target triple. Installed/verified does not mean trusted or
//! authorized; see the header's "A C ABI IS NOT A SANDBOX" section.

use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use marqdo_plugin_abi::PluginError;

/// Length of a lowercase hexadecimal sha256 digest.
pub const SHA256_HEX_LEN: usize = 64;

/// Compute the lowercase hexadecimal sha256 digest of `bytes`.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Compute the lowercase hexadecimal sha256 digest of a file.
pub fn sha256_file(path: &Path) -> Result<String, PluginError> {
    if !path.is_file() {
        return Err(PluginError::artifact(
            "missing_artifact",
            format!("artifact file `{}` does not exist", path.display()),
        ));
    }
    let mut file = fs::File::open(path).map_err(|error| {
        PluginError::artifact(
            "artifact_unreadable",
            format!("artifact file `{}` could not be opened", path.display()),
        )
        .with_detail(error.to_string())
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            PluginError::artifact(
                "artifact_unreadable",
                format!("artifact file `{}` could not be read", path.display()),
            )
            .with_detail(error.to_string())
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Validate that `digest` looks like a sha256 hex digest.
pub fn is_valid_sha256_hex(digest: &str) -> bool {
    digest.len() == SHA256_HEX_LEN && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Verify a file's sha256 digest against `expected` (case-insensitive hex).
/// Manifests conventionally use lowercase hex.
///
/// A missing file yields `missing_artifact`; a digest mismatch yields
/// `hash_mismatch`. Verification is content-only: it proves the artifact is
/// the one described by the manifest, not that the plugin is trustworthy.
pub fn verify_sha256_file(path: &Path, expected: &str) -> Result<(), PluginError> {
    if !is_valid_sha256_hex(expected) {
        return Err(PluginError::artifact(
            "invalid_sha256",
            "manifest sha256 must be a 64-character hexadecimal digest",
        ));
    }
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(PluginError::artifact(
            "hash_mismatch",
            format!(
                "artifact `{}` content hash does not match the manifest",
                path.display()
            ),
        )
        .with_detail(format!("expected {expected}, actual {actual}")));
    }
    Ok(())
}

/// A parsed target triple such as `x86_64-unknown-linux-gnu`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetTriple {
    triple: String,
}

impl TargetTriple {
    /// Parse and validate a target triple.
    pub fn parse(triple: &str) -> Result<Self, PluginError> {
        let parts: Vec<&str> = triple.split('-').collect();
        if parts.len() < 3 || parts.len() > 4 {
            return Err(PluginError::artifact(
                "invalid_target_triple",
                format!("`{triple}` is not a target triple"),
            ));
        }
        if parts.iter().any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        }) {
            return Err(PluginError::artifact(
                "invalid_target_triple",
                format!("`{triple}` contains an invalid target component"),
            ));
        }
        Ok(Self {
            triple: triple.to_ascii_lowercase(),
        })
    }

    /// The canonical triple string.
    pub fn as_str(&self) -> &str {
        &self.triple
    }

    /// The architecture component (`x86_64`, `aarch64`, ...).
    pub fn arch(&self) -> &str {
        self.triple.split('-').next().unwrap_or_default()
    }

    /// Pointer width in bytes implied by the architecture, if known.
    pub fn word_size(&self) -> Option<u32> {
        match self.arch() {
            "x86_64" | "aarch64" | "arm64" | "powerpc64" | "powerpc64le" | "s390x" | "riscv64"
            | "loongarch64" | "mips64" | "mips64el" | "sparc64" => Some(8),
            "i386" | "i486" | "i586" | "i686" | "arm" | "armv5te" | "armv7" | "armv7a"
            | "wasm32" | "mips" | "mipsel" | "riscv32" | "powerpc" | "sparc" => Some(4),
            _ => None,
        }
    }
}

impl std::fmt::Display for TargetTriple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.triple)
    }
}

/// The host's own build target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostTarget {
    triple: TargetTriple,
}

impl HostTarget {
    /// Construct from an explicit triple (used by tests and cross-checking).
    pub fn from_triple(triple: TargetTriple) -> Self {
        Self { triple }
    }

    /// The target this process was compiled for.
    pub fn current() -> Self {
        Self {
            triple: current_target_triple(),
        }
    }

    /// The host target triple.
    pub fn triple(&self) -> &TargetTriple {
        &self.triple
    }

    /// The host pointer width in bytes.
    pub fn word_size(&self) -> u32 {
        usize::BITS / 8
    }
}

/// Compute this process's target triple in the conventional 3/4-component
/// form used by Rust target triples.
pub fn current_target_triple() -> TargetTriple {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    let triple = match os {
        "linux" => {
            let environment = if cfg!(target_env = "musl") {
                "musl"
            } else if cfg!(target_env = "ohos") {
                "ohos"
            } else {
                "gnu"
            };
            format!("{arch}-unknown-linux-{environment}")
        }
        "macos" => format!("{arch}-apple-darwin"),
        "ios" => format!("{arch}-apple-ios"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        "freebsd" => format!("{arch}-unknown-freebsd"),
        "netbsd" => format!("{arch}-unknown-netbsd"),
        "openbsd" => format!("{arch}-unknown-openbsd"),
        "android" => format!("{arch}-linux-android"),
        "wasi" => format!("{arch}-wasi-wasm"),
        other => format!("{arch}-unknown-{other}"),
    };
    // The mapping above always yields a parseable triple.
    TargetTriple::parse(&triple).unwrap_or_else(|_| TargetTriple {
        triple: triple.to_ascii_lowercase(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hex_validation_is_strict() {
        assert!(is_valid_sha256_hex(&"a".repeat(64)));
        assert!(!is_valid_sha256_hex(&"a".repeat(63)));
        assert!(!is_valid_sha256_hex(&"g".repeat(64)));
    }

    #[test]
    fn target_triple_parsing_and_word_size() {
        let triple = TargetTriple::parse("x86_64-unknown-linux-gnu").expect("valid");
        assert_eq!(triple.arch(), "x86_64");
        assert_eq!(triple.word_size(), Some(8));
        assert_eq!(
            TargetTriple::parse("aarch64-apple-darwin")
                .expect("valid")
                .word_size(),
            Some(8)
        );
        assert_eq!(
            TargetTriple::parse("i686-unknown-linux-gnu")
                .expect("valid")
                .word_size(),
            Some(4)
        );
        // Syntactically valid but unknown architecture: word size unknown.
        assert_eq!(
            TargetTriple::parse("not-a-triple")
                .expect("syntactic triple")
                .word_size(),
            None
        );
        assert!(TargetTriple::parse("x86_64").is_err());
        assert!(TargetTriple::parse("x86_64--linux-gnu").is_err());
    }

    #[test]
    fn host_triple_is_parseable_and_matches_word_size() {
        let target = HostTarget::current();
        assert_eq!(target.triple().word_size(), Some(target.word_size()));
    }
}
