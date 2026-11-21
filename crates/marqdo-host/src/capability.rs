//! Capability model.
//!
//! A project manifest can *request* capabilities, but requests are not grants.
//! Effective permissions are the host default for the sandbox mode intersected
//! with what the operator/CLI explicitly granted. Every host operation checks
//! against this set at the point of execution.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use glob::Pattern;
use marqdo_plugin_abi::abi::MAX_CAPABILITY_REPLY_BYTES;
use marqdo_plugin_abi::PluginError;
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError};

use crate::fs::ProjectFs;

/// One audited capability request from a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityAudit {
    /// Plugin package that asked.
    pub package: String,
    /// Capability name.
    pub capability: String,
    /// `granted`, `denied` or `unavailable`.
    pub decision: &'static str,
    /// Human-readable reason.
    pub reason: String,
}

/// Broker for plugin host-capability requests (ADR-0011).
///
/// Core capabilities are checked against the effective grant set. Vendor
/// capabilities are accepted only when a trusted host policy registered the
/// exact name; a declaration in a manifest is never enough.
#[derive(Debug, Clone)]
pub struct CapabilityBroker {
    capabilities: Capabilities,
    vendor_policy: BTreeSet<String>,
    audit: Arc<Mutex<Vec<CapabilityAudit>>>,
    fs: Option<ProjectFs>,
}

impl CapabilityBroker {
    /// Construct a broker over an effective grant set.
    pub fn new(capabilities: Capabilities) -> Self {
        Self::with_audit(capabilities, Arc::new(Mutex::new(Vec::new())))
    }

    /// Construct a broker sharing an existing audit log.
    pub fn with_audit(capabilities: Capabilities, audit: Arc<Mutex<Vec<CapabilityAudit>>>) -> Self {
        Self {
            capabilities,
            vendor_policy: BTreeSet::new(),
            audit,
            fs: None,
        }
    }

    /// Attach the project filesystem used to execute granted `fs.read`
    /// requests. Without it, `fs.read` is authorized but unavailable.
    pub fn with_fs(mut self, fs: ProjectFs) -> Self {
        self.fs = Some(fs);
        self
    }

    /// Register a trusted vendor capability policy entry.
    pub fn allow_vendor(&mut self, capability: impl Into<String>) {
        self.vendor_policy.insert(capability.into());
    }

    /// Shared audit log.
    pub fn audit(&self) -> Arc<Mutex<Vec<CapabilityAudit>>> {
        Arc::clone(&self.audit)
    }

    /// Authorize one request, returning reply bytes on success.
    pub fn authorize(
        &mut self,
        package: &str,
        capability: &str,
        request: &[u8],
    ) -> Result<Vec<u8>, PluginError> {
        let result = self.decide(package, capability, request);
        let (decision, reason) = match &result {
            Ok(()) => ("granted", String::new()),
            Err(error) => ("denied", format!("{}: {}", error.code, error.message)),
        };
        if let Ok(mut audit) = self.audit.lock() {
            audit.push(CapabilityAudit {
                package: package.to_string(),
                capability: capability.to_string(),
                decision,
                reason,
            });
        }
        result.map(|()| Vec::new())
    }

    /// Authorize and execute one capability request.
    ///
    /// Only `fs.read` has a real implementation in this build. Other
    /// capabilities return `capability.unavailable` instead of pretending an
    /// empty reply means success (ADR-0011, spec §12.2).
    pub fn execute(
        &mut self,
        package: &str,
        capability: &str,
        request: &[u8],
    ) -> Result<Vec<u8>, PluginError> {
        if let Err(error) = self.decide(package, capability, request) {
            let reason = format!("{}: {}", error.code, error.message);
            self.record(package, capability, "denied", &reason);
            return Err(error);
        }
        let result = match capability {
            "fs.read" => {
                let path = request_path(request)?;
                let Some(fs) = &self.fs else {
                    return self.unavailable(
                        package,
                        capability,
                        "no project filesystem is attached",
                    );
                };
                match fs.read(&path, None) {
                    Ok(contents) => {
                        if contents.len() > MAX_CAPABILITY_REPLY_BYTES {
                            return self.unavailable(
                                package,
                                capability,
                                "the file exceeds the capability reply limit",
                            );
                        }
                        Ok(contents.into_bytes())
                    }
                    Err(error) => {
                        let error = map_runtime(error);
                        let reason = format!("{}: {}", error.code, error.message);
                        self.record(package, capability, "denied", &reason);
                        Err(error)
                    }
                }
            }
            _ => self.unavailable(
                package,
                capability,
                "this capability has no executing handler in this build",
            ),
        };
        if result.is_ok() {
            self.record(package, capability, "granted", "");
        }
        result
    }

    fn unavailable<T>(
        &mut self,
        package: &str,
        capability: &str,
        message: &str,
    ) -> Result<T, PluginError> {
        let error = PluginError::capability("capability.unavailable", message.to_string());
        self.record(package, capability, "unavailable", message);
        Err(error)
    }

    fn record(&mut self, package: &str, capability: &str, decision: &'static str, reason: &str) {
        if let Ok(mut audit) = self.audit.lock() {
            audit.push(CapabilityAudit {
                package: package.to_string(),
                capability: capability.to_string(),
                decision,
                reason: reason.to_string(),
            });
        }
    }

    fn decide(&self, package: &str, capability: &str, request: &[u8]) -> Result<(), PluginError> {
        match capability {
            "console" => self
                .capabilities
                .console
                .then_some(())
                .ok_or_else(|| plugin_denied("console capability was not granted")),
            "math.compute" => self
                .capabilities
                .math
                .then_some(())
                .ok_or_else(|| plugin_denied("math capability was not granted")),
            "log" | "clock" | "random" | "artifact.record" => Ok(()),
            "fs.read" | "fs.write" => {
                let path = request_path(request)?;
                if capability == "fs.read" {
                    self.capabilities
                        .check_read(&path, None)
                        .map_err(map_runtime)
                } else {
                    self.capabilities
                        .check_write(&path, None)
                        .map_err(map_runtime)
                }
            }
            "net.request" => {
                let host = request_host(request)?;
                self.capabilities
                    .check_net(&host, None)
                    .map_err(map_runtime)
            }
            "process" => self
                .capabilities
                .process
                .then_some(())
                .ok_or_else(|| plugin_denied("process capability was not granted")),
            other => {
                if is_vendor_capability(other)
                    && (self.vendor_policy.contains(other) || self.vendor_policy.contains("*"))
                {
                    Ok(())
                } else if is_vendor_capability(other) {
                    Err(PluginError::capability(
                        "capability.unavailable",
                        format!(
                            "vendor capability `{other}` has no trusted handler for package `{package}`"
                        ),
                    ))
                } else {
                    Err(PluginError::capability(
                        "capability.unknown",
                        format!("unknown core capability `{other}`"),
                    ))
                }
            }
        }
    }
}

fn plugin_denied(message: &str) -> PluginError {
    PluginError::capability("permission.denied", message.to_string())
}

fn map_runtime(error: RuntimeError) -> PluginError {
    PluginError::capability(error.code(), error.message)
}

fn request_json(request: &[u8]) -> Result<serde_json::Value, PluginError> {
    if request.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_slice(request).map_err(|error| {
        PluginError::capability(
            "capability.request_invalid",
            format!("capability request is not valid JSON: {error}"),
        )
    })
}

fn request_path(request: &[u8]) -> Result<String, PluginError> {
    let value = request_json(request)?;
    value
        .get("path")
        .and_then(|path| path.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            PluginError::capability(
                "capability.request_invalid",
                "fs capability requests must include a `path` string",
            )
        })
}

fn request_host(request: &[u8]) -> Result<String, PluginError> {
    let value = request_json(request)?;
    value
        .get("host")
        .and_then(|host| host.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            PluginError::capability(
                "capability.request_invalid",
                "net capability requests must include a `host` string",
            )
        })
}

/// Whether `name` is a syntactically valid vendor-namespaced capability.
pub fn is_vendor_capability(name: &str) -> bool {
    let segments: Vec<&str> = name.split('.').collect();
    if segments.len() < 3 {
        return false;
    }
    if matches!(
        segments[0],
        "console" | "math" | "fs" | "net" | "process" | "log" | "clock" | "random"
    ) {
        return false;
    }
    segments.iter().all(|segment| {
        !segment.is_empty()
            && segment.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
            })
    })
}

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
    /// Allowed environment variable names (empty = no environment).
    pub env: Vec<String>,
    /// Allowed process program paths (empty = no process execution).
    pub process_programs: Vec<String>,
    /// May spawn processes (foreign kernels).
    pub process: bool,
    /// May run foreign kernels (Python/Node).
    pub foreign: bool,
    /// Allowed plugin names.
    pub plugins: Vec<String>,
    /// Allowed resource-provider names (ADR-0033).
    pub resource_providers: Vec<String>,
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
            env: Vec::new(),
            process_programs: Vec::new(),
            process: false,
            foreign: false,
            plugins: Vec::new(),
            resource_providers: Vec::new(),
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

    /// Grant read access to one environment variable name (ADR-0043).
    pub fn allow_env(mut self, name: impl Into<String>) -> Self {
        self.env.push(name.into());
        self
    }

    /// Check an environment variable grant. Names are exact, not globs.
    pub fn check_env(&self, name: &str, span: Option<Span>) -> Result<(), RuntimeError> {
        if self.env.iter().any(|allowed| allowed == name) {
            Ok(())
        } else {
            Err(denied(
                span,
                format!("environment variable `{name}` was not granted"),
                format!("grant it with --allow-env {name}"),
            ))
        }
    }

    /// Grant execution of one exact program path (ADR-0044).
    pub fn allow_process(mut self, program: impl Into<String>) -> Self {
        self.process_programs.push(program.into());
        self
    }

    /// Check an exact process-program grant.
    pub fn check_process(&self, program: &str, span: Option<Span>) -> Result<(), RuntimeError> {
        if self
            .process_programs
            .iter()
            .any(|allowed| allowed == program)
        {
            Ok(())
        } else {
            Err(denied(
                span,
                format!("process `{program}` was not granted"),
                format!("grant it with --allow-process {program}"),
            ))
        }
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

    /// Grant network access to one IP literal host (ADR-0034).
    pub fn allow_net(mut self, host: impl Into<String>) -> Self {
        self.net_hosts.push(host.into());
        self
    }

    /// Grant a registered resource provider by its stable name (ADR-0033).
    pub fn allow_resource_provider(mut self, name: impl Into<String>) -> Self {
        self.resource_providers.push(name.into());
        self
    }

    /// Check a resource-provider grant.
    pub fn check_resource_provider(
        &self,
        name: &str,
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        if self
            .resource_providers
            .iter()
            .any(|allowed| allowed == name)
        {
            Ok(())
        } else {
            Err(denied(
                span,
                format!("resource provider `{name}` was not granted"),
                format!("grant it with Capabilities::allow_resource_provider({name:?})"),
            ))
        }
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
    fn broker_enforces_core_grants_and_vendor_policy() {
        let mut broker = CapabilityBroker::new(Capabilities::project().allow_read("data/**"));
        assert!(broker
            .authorize("demo.solver", "fs.read", br#"{"path":"data/a.csv"}"#)
            .is_ok());
        let denied = broker
            .authorize("demo.solver", "fs.read", br#"{"path":"secrets/key"}"#)
            .unwrap_err();
        assert_eq!(denied.code, "permission.denied");
        assert!(broker.authorize("demo.solver", "fs.read", b"{}").is_err());
        let unavailable = broker
            .authorize("demo.solver", "vendor.device.read", b"{}")
            .unwrap_err();
        assert_eq!(unavailable.code, "capability.unavailable");
        broker.allow_vendor("vendor.device.read");
        assert!(broker
            .authorize("demo.solver", "vendor.device.read", b"{}")
            .is_ok());
        assert!(!is_vendor_capability("vendor"));
        assert!(!is_vendor_capability("fs.read.extra"));
        let audit = broker.audit();
        let audit = audit.lock().unwrap();
        assert_eq!(audit.len(), 5);
        assert!(audit.iter().any(|entry| entry.decision == "denied"));
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

#[cfg(test)]
mod execute_tests {
    use super::*;

    #[test]
    fn broker_executes_fs_read_and_reports_unavailable_for_other_capabilities() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data/allowed.txt"), "hello").unwrap();
        std::fs::create_dir_all(dir.path().join("private")).unwrap();
        std::fs::write(dir.path().join("private/secret.txt"), "secret").unwrap();
        let fs = ProjectFs::new(dir.path());
        let capabilities = Capabilities::project()
            .allow_read("data/**")
            .allow_write("data/**");
        let mut broker = CapabilityBroker::new(capabilities).with_fs(fs);

        let contents = broker
            .execute("demo.solver", "fs.read", br#"{"path":"data/allowed.txt"}"#)
            .unwrap();
        assert_eq!(String::from_utf8(contents).unwrap(), "hello");

        let denied = broker
            .execute(
                "demo.solver",
                "fs.read",
                br#"{"path":"private/secret.txt"}"#,
            )
            .unwrap_err();
        assert_eq!(denied.code, "permission.denied");

        let console = broker.execute("demo.solver", "console", b"{}").unwrap_err();
        assert_eq!(console.code, "capability.unavailable");
        let write = broker
            .execute(
                "demo.solver",
                "fs.write",
                br#"{"path":"data/x","contents":"x"}"#,
            )
            .unwrap_err();
        assert_eq!(write.code, "capability.unavailable");

        let audit = broker.audit();
        let audit = audit.lock().unwrap();
        assert!(audit.iter().any(|entry| entry.decision == "granted"));
        assert!(audit.iter().any(|entry| entry.decision == "denied"));
        assert!(audit.iter().any(|entry| entry.decision == "unavailable"));
        assert!(!format!("{audit:?}").contains("hello"));
    }
}
