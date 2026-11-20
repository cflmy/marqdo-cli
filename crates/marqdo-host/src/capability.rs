//! Capability model.
//!
//! A project manifest can *request* capabilities, but requests are not grants.
//! Effective permissions are the host default for the sandbox mode intersected
//! with what the operator/CLI explicitly granted. Every host operation checks
//! against this set at the point of execution.

use glob::Pattern;
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError};

/// Effective capability grants for one run.
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    /// Write to program standard output (`打印`).
    pub console: bool,
    /// Use the native math kernel.
    pub math: bool,
    /// Readable project-relative path patterns.
    pub fs_read: Vec<String>,
    /// Writable project-relative path patterns.
    pub fs_write: Vec<String>,
    /// Allowed network hosts (empty = no network).
    pub net_hosts: Vec<String>,
    /// May spawn processes (foreign kernels).
    pub process: bool,
    /// May run foreign kernels (Python/Node).
    pub foreign: bool,
    /// Allowed plugin names.
    pub plugins: Vec<String>,
}

impl Capabilities {
    /// Default grants for pure mode: no external I/O, math and console only.
    pub fn pure() -> Self {
        Self {
            console: true,
            math: true,
            fs_read: Vec::new(),
            fs_write: Vec::new(),
            net_hosts: Vec::new(),
            process: false,
            foreign: false,
            plugins: Vec::new(),
        }
    }

    /// Default grants for project mode: the same baseline; operators add
    /// explicit grants for files, processes or plugins.
    pub fn project() -> Self {
        Self::pure()
    }

    /// Grant read access to a project-relative glob pattern.
    pub fn allow_read(mut self, pattern: impl Into<String>) -> Self {
        self.fs_read.push(pattern.into());
        self
    }

    /// Grant write access to a project-relative glob pattern.
    pub fn allow_write(mut self, pattern: impl Into<String>) -> Self {
        self.fs_write.push(pattern.into());
        self
    }

    /// Grant foreign kernel execution.
    pub fn allow_foreign(mut self) -> Self {
        self.foreign = true;
        self.process = true;
        self
    }

    /// Grant a plugin name.
    pub fn allow_plugin(mut self, name: impl Into<String>) -> Self {
        self.plugins.push(name.into());
        self
    }

    /// Whether a project-relative path matches a granted pattern.
    ///
    /// Patterns and paths are normalised by stripping a leading `./` so that
    /// `--allow-read ./data/**` and the path `data/a.csv` agree.
    fn matches(patterns: &[String], relative: &str) -> bool {
        let relative = relative.trim_start_matches("./");
        patterns.iter().any(|pattern| {
            let pattern = pattern.trim_start_matches("./");
            Pattern::new(pattern)
                .map(|pattern| pattern.matches(relative))
                .unwrap_or(false)
        })
    }

    /// Check read access.
    pub fn check_read(&self, relative: &str, span: Option<Span>) -> Result<(), RuntimeError> {
        if Self::matches(&self.fs_read, relative) {
            Ok(())
        } else {
            Err(denied(
                span,
                format!("read access to `{relative}` was not granted"),
                format!("grant it with --allow-read {relative} or a project capability"),
            ))
        }
    }

    /// Check write access.
    pub fn check_write(&self, relative: &str, span: Option<Span>) -> Result<(), RuntimeError> {
        if Self::matches(&self.fs_write, relative) {
            Ok(())
        } else {
            Err(denied(
                span,
                format!("write access to `{relative}` was not granted"),
                format!("grant it with --allow-write {relative} or a project capability"),
            ))
        }
    }

    /// Check network access for a host.
    pub fn check_net(&self, host: &str, span: Option<Span>) -> Result<(), RuntimeError> {
        if self.net_hosts.iter().any(|allowed| allowed == host) {
            Ok(())
        } else {
            Err(denied(
                span,
                format!("network access to `{host}` was not granted"),
                "network access is denied by default",
            ))
        }
    }

    /// Check foreign kernel execution.
    pub fn check_foreign(&self, span: Option<Span>) -> Result<(), RuntimeError> {
        if self.foreign && self.process {
            Ok(())
        } else {
            Err(denied(
                span,
                "foreign kernel execution was not granted",
                "grant it with --allow-foreign (project/strict policy permitting)",
            ))
        }
    }

    /// Check plugin execution.
    pub fn check_plugin(&self, name: &str, span: Option<Span>) -> Result<(), RuntimeError> {
        if self.plugins.iter().any(|allowed| allowed == name) {
            Ok(())
        } else {
            Err(denied(
                span,
                format!("plugin `{name}` was not granted execution permission"),
                "installing a plugin does not grant permission to run it",
            ))
        }
    }
}

fn denied(span: Option<Span>, message: impl Into<String>, hint: impl Into<String>) -> RuntimeError {
    let mut error = RuntimeError::new(ErrorCategory::PermissionDenied, span, message);
    error.effect_trace.push(format!("hint: {}", hint.into()));
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use marqdo_value::ErrorCategory;

    #[test]
    fn defaults_deny_external_io() {
        let capabilities = Capabilities::pure();
        assert!(capabilities.math);
        assert!(capabilities.console);
        assert_eq!(
            capabilities
                .check_read("data/a.txt", None)
                .unwrap_err()
                .category,
            ErrorCategory::PermissionDenied
        );
        assert!(capabilities.check_foreign(None).is_err());
    }

    #[test]
    fn explicit_grants_are_glob_scoped() {
        let capabilities = Capabilities::project()
            .allow_read("./data/**")
            .allow_write("./output/**");
        assert!(capabilities.check_read("data/a.csv", None).is_ok());
        assert!(capabilities.check_read("secrets/key", None).is_err());
        assert!(capabilities.check_write("output/result.txt", None).is_ok());
        assert!(capabilities.check_write("data/a.csv", None).is_err());
    }
}
