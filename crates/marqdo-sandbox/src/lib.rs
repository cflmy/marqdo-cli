//! Sandbox modes and platform isolation capability detection.
//!
//! Marqdo advertises security guarantees only when they can be enforced. In
//! particular, `strict` mode must **refuse to run** when the build cannot
//! provide OS-level isolation: a cooperative in-process check is not a sandbox
//! (blueprint section 9). This crate is deliberately honest about that.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{marker_present, probe, verify_active, StrictOutcome};

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
    /// Strict OS isolation remains intentionally unavailable: no OS isolation
    /// adapter is implemented, and pretending otherwise would violate the
    /// security contract. The isolated worker transport is implemented, but a
    /// plain child process is not an OS sandbox.
    pub fn detect() -> Self {
        let strict = if cfg!(target_os = "linux") {
            match linux::probe() {
                Ok(mechanism) => StrictIsolation::Available { mechanism },
                Err(reason) => StrictIsolation::Unavailable { reason },
            }
        } else if cfg!(target_os = "macos") {
            StrictIsolation::Unavailable {
                reason: "no verified macOS sandbox adapter is implemented in this build"
                    .to_string(),
            }
        } else if cfg!(target_os = "windows") {
            StrictIsolation::Unavailable {
                reason: "no verified Windows job-object/appcontainer adapter is implemented in this build".to_string(),
            }
        } else {
            StrictIsolation::Unavailable {
                reason: "platform isolation is not implemented for this target".to_string(),
            }
        };
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            strict,
            worker_processes: true,
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
    /// The strict supervisor could not launch, verify or supervise the child.
    LaunchFailed {
        /// Actionable explanation.
        message: String,
    },
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::StrictUnavailable { reason } => write!(
                f,
                "refusing to run in strict mode: {reason}. Strict mode requires verified OS-level isolation and never falls back to an unrestricted process."
            ),
            SandboxError::LaunchFailed { message } => {
                write!(f, "strict isolation failed: {message}")
            }
        }
    }
}

impl std::error::Error for SandboxError {}

/// Validate that the requested mode can be honoured on this platform.
pub fn enforce(mode: SandboxMode, platform: &PlatformCapabilities) -> Result<(), SandboxError> {
    match mode {
        SandboxMode::Strict => {
            #[cfg(target_os = "linux")]
            if verify_active().is_ok() {
                return Ok(());
            }
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

/// A command to execute inside the strict Linux sandbox.
#[derive(Debug, Clone)]
pub struct StrictHostCommand<'a> {
    /// Program bound read-only into the sandbox and executed.
    pub program: &'a Path,
    /// Arguments passed to the program.
    pub args: &'a [std::ffi::OsString],
    /// Project directory mounted read-only at `/project`.
    pub project_root: &'a Path,
    /// Entry document as passed on the command line; rewritten to the sandbox
    /// path. `None` keeps the CLI default (index.mq.md).
    pub document: Option<&'a Path>,
    /// Original document token to replace in `args` (same value as `document`
    /// when present).
    pub original_document: Option<&'a Path>,
    /// Combined per-stream output cap in bytes.
    pub output_limit: usize,
    /// Wall-clock budget for the supervised process tree.
    pub timeout: Duration,
}

/// Launch a command inside the strict sandbox (Linux only).
#[cfg(target_os = "linux")]
pub fn launch_strict(command: &StrictHostCommand<'_>) -> Result<i32, SandboxError> {
    linux::launch(command)
}

/// Launch a command inside the strict sandbox and return its captured outcome.
#[cfg(target_os = "linux")]
pub fn launch_strict_capture(
    command: &StrictHostCommand<'_>,
) -> Result<linux::StrictOutcome, SandboxError> {
    linux::launch_capture(command)
}

/// Launch a command inside the strict sandbox (unsupported elsewhere).
#[cfg(not(target_os = "linux"))]
pub fn launch_strict(_command: &StrictHostCommand<'_>) -> Result<i32, SandboxError> {
    Err(SandboxError::StrictUnavailable {
        reason: "strict OS isolation is only implemented on Linux".to_string(),
    })
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
        let platform = PlatformCapabilities {
            os: "test".to_string(),
            arch: "test".to_string(),
            strict: StrictIsolation::Unavailable {
                reason: "test host has no verified isolation".to_string(),
            },
            worker_processes: true,
        };
        let result = enforce(SandboxMode::Strict, &platform);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("strict"));
        assert!(enforce(SandboxMode::Pure, &platform).is_ok());
        assert!(enforce(SandboxMode::Project, &platform).is_ok());
    }
}
