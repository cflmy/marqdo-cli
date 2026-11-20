//! Sandbox modes and platform isolation capability detection.
//!
//! Marqdo advertises security guarantees only when they can be enforced. In
//! particular, `strict` mode must **refuse to run** when the build cannot
//! provide OS-level isolation: a cooperative in-process check is not a sandbox
//! (blueprint section 9). This crate is deliberately honest about that.

use serde::{Deserialize, Serialize};

/// Execution mode requested on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    /// Pure Marqdo and native math only; no external I/O.
    Pure,
    /// Project resources explicitly granted by the host/web caller.
    Project,
    /// Untrusted documents/code/plugins; requires real OS isolation.
    Strict,
}

impl SandboxMode {
    /// Parse a mode name.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pure" => Some(SandboxMode::Pure),
            "project" => Some(SandboxMode::Project),
            "strict" => Some(SandboxMode::Strict),
            _ => None,
        }
    }

    /// Stable lowercase name.
    pub const fn as_str(self) -> &'static str {
        match self {
            SandboxMode::Pure => "pure",
            SandboxMode::Project => "project",
            SandboxMode::Strict => "strict",
        }
    }
}

impl std::fmt::Display for SandboxMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether real OS-level isolation is available for strict mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StrictIsolation {
    /// Isolation is implemented and verified on this platform.
    Available {
        /// Mechanism, e.g. `linux-namespaces-seccomp`.
        mechanism: String,
    },
    /// Isolation is not available; strict mode must refuse to run.
    Unavailable {
        /// Human readable reason.
        reason: String,
    },
}

impl StrictIsolation {
    /// Whether strict execution can be offered.
    pub fn is_available(&self) -> bool {
        matches!(self, StrictIsolation::Available { .. })
    }

    /// Human readable explanation.
    pub fn describe(&self) -> String {
        match self {
            StrictIsolation::Available { mechanism } => {
                format!("strict isolation available via {mechanism}")
            }
            StrictIsolation::Unavailable { reason } => {
                format!("strict isolation unavailable: {reason}")
            }
        }
    }
}

/// Platform capability report, shown by `marqdo inspect --platform` and used to
/// gate strict mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCapabilities {
    /// Operating system family.
    pub os: String,
    /// Architecture.
    pub arch: String,
    /// Strict-mode isolation status.
    pub strict: StrictIsolation,
    /// Whether spawning isolated worker processes is implemented.
    pub worker_processes: bool,
}

impl PlatformCapabilities {
    /// Detect capabilities for the current build and platform.
    ///
    /// M0.5 intentionally reports strict isolation as unavailable: no OS
    /// isolation adapter is implemented yet, and pretending otherwise would
    /// violate the security contract. The worker process transport is likewise
    /// not implemented in this build.
    pub fn detect() -> Self {
        let reason = if cfg!(target_os = "linux") {
            "no OS isolation adapter (namespaces/seccomp/cgroups) is implemented in this build"
        } else if cfg!(target_os = "macos") {
            "no verified macOS sandbox adapter is implemented in this build"
        } else if cfg!(target_os = "windows") {
            "no verified Windows job-object/appcontainer adapter is implemented in this build"
        } else {
            "platform isolation is not implemented for this target"
        };
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            strict: StrictIsolation::Unavailable {
                reason: reason.to_string(),
            },
            worker_processes: false,
        }
    }
}

/// Failure to apply the requested sandbox mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxError {
    /// Strict mode was requested but the platform cannot provide isolation.
    StrictUnavailable {
        /// Explanation from [`PlatformCapabilities`].
        reason: String,
    },
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::StrictUnavailable { reason } => write!(
                f,
                "refusing to run in strict mode: {reason}. Strict mode requires verified OS-level isolation and never falls back to an unrestricted process."
            ),
        }
    }
}

impl std::error::Error for SandboxError {}

/// Validate that the requested mode can be honoured on this platform.
pub fn enforce(mode: SandboxMode, platform: &PlatformCapabilities) -> Result<(), SandboxError> {
    match mode {
        SandboxMode::Strict => {
            if platform.strict.is_available() {
                Ok(())
            } else {
                let reason = match &platform.strict {
                    StrictIsolation::Available { .. } => String::new(),
                    StrictIsolation::Unavailable { reason } => reason.clone(),
                };
                Err(SandboxError::StrictUnavailable { reason })
            }
        }
        SandboxMode::Pure | SandboxMode::Project => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_round_trip() {
        for mode in [SandboxMode::Pure, SandboxMode::Project, SandboxMode::Strict] {
            assert_eq!(SandboxMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(SandboxMode::parse("nope"), None);
    }

    #[test]
    fn strict_refuses_when_isolation_is_unavailable() {
        let platform = PlatformCapabilities::detect();
        let result = enforce(SandboxMode::Strict, &platform);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("strict"));
        assert!(enforce(SandboxMode::Pure, &platform).is_ok());
        assert!(enforce(SandboxMode::Project, &platform).is_ok());
    }
}
