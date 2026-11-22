//! Capability model and the trusted capability-handler registry.
//!
//! A project manifest can *request* capabilities, but requests are not grants.
//! Effective permissions are the host default for the sandbox mode intersected
//! with what the operator/CLI explicitly granted. Every host operation checks
//! against this set at the point of execution.
//!
//! Executable vendor/core capabilities additionally need an explicitly
//! registered trusted [`CapabilityHandler`] (ADR-0057, RFC-EXT-0001 §7.2).
//! Being authorized is never the same as being executable: a request without a
//! handler fails with `capability.unavailable` instead of returning an empty
//! successful reply.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use glob::Pattern;
use marqdo_plugin_abi::abi::MAX_CAPABILITY_REPLY_BYTES;
use marqdo_plugin_abi::PluginError;
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError};

use crate::budget::Budget;
use crate::fs::ProjectFs;

/// Core capability names understood by [`CapabilityBroker::decide`].
///
/// This is the closed set of host primitives (ADR-0011). A trusted embedder
/// may install a [`CapabilityHandler`] for one of these names; until it does,
/// the capability is authorized by policy but *unavailable* at execution.
pub const CORE_CAPABILITIES: [&str; 10] = [
    "console",
    "math.compute",
    "fs.read",
    "fs.write",
    "net.request",
    "process",
    "clock",
    "random",
    "log",
    "artifact.record",
];

/// Capability namespace prefixes reserved for the host itself.
///
/// Handler registration rejects these prefixes so a vendor package can never
/// shadow a host-internal operation.
pub const RESERVED_HANDLER_PREFIXES: [&str; 2] = ["marqdo.", "host."];

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
    /// Wall-clock duration of the decision/execution in microseconds.
    pub duration_micros: u64,
}

/// One capability request handed to an installed [`CapabilityHandler`].
#[derive(Debug, Clone, Copy)]
pub struct CapabilityRequest<'a> {
    /// Trusted package identity supplied by the host, never by plugin wire data.
    pub package_instance: &'a str,
    /// Capability being requested.
    pub capability: &'a str,
    /// Raw request payload (JSON for the predefined capabilities).
    pub payload: &'a [u8],
    /// Run/session trace id used to correlate audit entries with a run.
    pub trace_id: &'a str,
}

/// Trusted, explicitly registered executor for one capability.
///
/// RFC-EXT-0001 §7.2 names the policy parameter `EffectiveGrant` and the budget
/// `ResourceBudget`; this build deliberately reuses the existing
/// [`Capabilities`] grant set and [`Budget`] instead of introducing a second
/// permission system (the RFC explicitly allows that substitution).
///
/// A handler is only reachable after [`CapabilityBroker::decide`] authorizes
/// the request, but it must still verify its own scope (project-relative paths,
/// globs, per-file limits) and must charge work to `budget` where applicable.
/// Registration is host-only: there is no wire path from a plugin to this
/// registry.
pub trait CapabilityHandler: Send {
    /// Stable capability name this handler serves.
    fn name(&self) -> &str;
    /// Execute one already-authorized request.
    fn execute(
        &mut self,
        request: &CapabilityRequest<'_>,
        capabilities: &Capabilities,
        budget: &mut Budget,
    ) -> Result<Vec<u8>, PluginError>;
}

/// Built-in handler for granted `fs.read` requests.
///
/// Reads go through [`ProjectFs`] (root confinement, traversal rejection,
/// regular-file/UTF-8/8 MiB limits) and the grant is checked **again** against
/// the normalized project-relative path at the execution point, so a raw
/// payload cannot win the glob check with a path the filesystem later
/// normalizes differently.
pub struct FsReadHandler {
    fs: ProjectFs,
}

impl FsReadHandler {
    /// Install the built-in handler over one project filesystem view.
    pub fn new(fs: ProjectFs) -> Self {
        Self { fs }
    }
}

impl CapabilityHandler for FsReadHandler {
    fn name(&self) -> &str {
        "fs.read"
    }

    fn execute(
        &mut self,
        request: &CapabilityRequest<'_>,
        capabilities: &Capabilities,
        _budget: &mut Budget,
    ) -> Result<Vec<u8>, PluginError> {
        // Reply-size limits are enforced by the broker, so this handler keeps
        // the historical per-read behavior and only needs the grant re-check.
        let raw_path = request_path(request.payload)?;
        let (_, relative) = self.fs.resolve(&raw_path, None).map_err(map_runtime)?;
        capabilities
            .check_read(&relative, None)
            .map_err(map_runtime)?;
        let contents = self.fs.read(&raw_path, None).map_err(map_runtime)?;
        Ok(contents.into_bytes())
    }
}

struct HandlerEntry {
    handler: Box<dyn CapabilityHandler>,
    /// Exact trusted package binding. `None` means the handler is authorized
    /// by the capability grant alone (for example project-scoped `fs.read`).
    package: Option<String>,
}

/// Broker for plugin host-capability requests (ADR-0011, ADR-0057).
///
/// The broker keeps two independent sets: trusted vendor *policy* entries and
/// installed *handlers*. Execution is
///
/// ```text
/// decide (grant + policy + payload shape)
///   -> handler installed? (else capability.unavailable)
///   -> package-bound handler owns the claimed package? (else permission.denied)
///   -> handler executes under the broker budget and reply limit
///   -> audit { package, capability, granted|denied|unavailable, reason, duration }
/// ```
///
/// A capability that is merely authorized never executes: there is no
/// empty-success path.
pub struct CapabilityBroker {
    capabilities: Capabilities,
    vendor_policy: BTreeSet<String>,
    handlers: BTreeMap<String, HandlerEntry>,
    reply_limit: usize,
    budget: Budget,
    trace_id: String,
    audit: Arc<Mutex<Vec<CapabilityAudit>>>,
}

impl std::fmt::Debug for CapabilityBroker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bound: Vec<(&str, &str)> = self
            .handlers
            .iter()
            .filter_map(|(name, entry)| {
                entry
                    .package
                    .as_deref()
                    .map(|package| (name.as_str(), package))
            })
            .collect();
        formatter
            .debug_struct("CapabilityBroker")
            .field("capabilities", &self.capabilities)
            .field("vendor_policy", &self.vendor_policy)
            .field("handlers", &self.handlers.keys().collect::<Vec<_>>())
            .field("package_bound_handlers", &bound)
            .field("reply_limit", &self.reply_limit)
            .field("budget", &self.budget)
            .field("trace_id", &self.trace_id)
            .finish()
    }
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
            handlers: BTreeMap::new(),
            reply_limit: MAX_CAPABILITY_REPLY_BYTES,
            budget: Budget::default(),
            trace_id: String::new(),
            audit,
        }
    }

    /// Attach the project filesystem and install the trusted built-in
    /// `fs.read` handler.
    ///
    /// Without an attached filesystem, granted `fs.read` requests stay
    /// authorized but have no handler, so execution returns
    /// `capability.unavailable` (never an empty success).
    ///
    /// # Panics
    ///
    /// Panics if a handler for `fs.read` was already registered on this broker,
    /// which would make the built-in handler ambiguous.
    pub fn with_fs(mut self, fs: ProjectFs) -> Self {
        self.register_handler(FsReadHandler::new(fs))
            .expect("a fresh broker has no fs.read handler to conflict with");
        self
    }

    /// Override the maximum reply size a handler may return.
    pub fn with_reply_limit(mut self, bytes: usize) -> Self {
        self.reply_limit = bytes;
        self
    }

    /// Attach the per-run budget that handlers charge work to.
    pub fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = budget;
        self
    }

    /// Set the run/session trace id surfaced to handlers.
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = trace_id.into();
        self
    }

    /// Remaining broker budget (diagnostics and tests).
    pub fn budget(&self) -> &Budget {
        &self.budget
    }

    /// Install a trusted handler reachable by any package that holds a grant.
    ///
    /// Registration is a host-only operation; a plugin manifest cannot reach
    /// this API. Duplicate, reserved and empty/invalid names are rejected.
    pub fn register_handler<H>(&mut self, handler: H) -> Result<(), PluginError>
    where
        H: CapabilityHandler + 'static,
    {
        self.install_handler(handler, None)
    }

    /// Install a trusted handler bound to one exact trusted package identity.
    ///
    /// A request that claims any other package is `permission.denied`, even
    /// when the capability grant and vendor policy match.
    pub fn register_package_handler<H>(
        &mut self,
        package: impl Into<String>,
        handler: H,
    ) -> Result<(), PluginError>
    where
        H: CapabilityHandler + 'static,
    {
        self.install_handler(handler, Some(package.into()))
    }

    fn install_handler<H>(&mut self, handler: H, package: Option<String>) -> Result<(), PluginError>
    where
        H: CapabilityHandler + 'static,
    {
        let name = handler.name().to_string();
        validate_handler_name(&name)?;
        if self.handlers.contains_key(&name) {
            return Err(PluginError::capability(
                "capability.handler_conflict",
                format!("a capability handler is already registered for `{name}`"),
            ));
        }
        self.handlers.insert(
            name,
            HandlerEntry {
                handler: Box::new(handler),
                package,
            },
        );
        Ok(())
    }

    /// Names with an installed trusted handler, in deterministic order.
    pub fn handler_names(&self) -> Vec<&str> {
        self.handlers.keys().map(String::as_str).collect()
    }

    /// Whether a trusted executing handler is installed for `capability`.
    pub fn has_handler(&self, capability: &str) -> bool {
        self.handlers.contains_key(capability)
    }

    /// Register a trusted vendor capability policy entry.
    ///
    /// This is policy only: execution additionally requires an installed
    /// handler. Registering a vendor name does not install anything.
    pub fn allow_vendor(&mut self, capability: impl Into<String>) {
        self.vendor_policy.insert(capability.into());
    }

    /// Shared audit log.
    pub fn audit(&self) -> Arc<Mutex<Vec<CapabilityAudit>>> {
        Arc::clone(&self.audit)
    }

    /// Authorize one request, returning reply bytes on success.
    ///
    /// This is the grant decision only; it does not prove that an executor is
    /// installed. Call [`Self::execute`] to actually run a capability.
    pub fn authorize(
        &mut self,
        package: &str,
        capability: &str,
        request: &[u8],
    ) -> Result<Vec<u8>, PluginError> {
        let started = Instant::now();
        let result = self.decide(package, capability, request);
        let (decision, reason) = match &result {
            Ok(()) => ("granted", String::new()),
            Err(error) => ("denied", format!("{}: {}", error.code, error.message)),
        };
        self.record(package, capability, decision, &reason, started);
        result.map(|()| Vec::new())
    }

    /// Authorize and execute one capability request through its trusted
    /// handler.
    ///
    /// The chain is fixed: `decide` → handler installed → package binding →
    /// handler execution under the broker budget and reply limit → audit.
    /// Anything without an installed handler is `capability.unavailable`; it
    /// is never reported as an empty success (ADR-0011, ADR-0057).
    pub fn execute(
        &mut self,
        package: &str,
        capability: &str,
        request: &[u8],
    ) -> Result<Vec<u8>, PluginError> {
        let started = Instant::now();

        // 1. Authorization: effective grants, vendor policy, payload shape.
        if let Err(error) = self.decide(package, capability, request) {
            let reason = format!("{}: {}", error.code, error.message);
            self.record(package, capability, "denied", &reason, started);
            return Err(error);
        }

        // 2. A trusted executor must exist; a package-bound executor must own
        //    the trusted package identity the host is claiming.
        let bound_package = match self.handlers.get(capability) {
            Some(entry) => entry.package.clone(),
            None => {
                let message = "this capability has no executing handler in this build";
                self.record(package, capability, "unavailable", message, started);
                return Err(PluginError::capability(
                    "capability.unavailable",
                    message.to_string(),
                ));
            }
        };
        if let Some(bound) = bound_package {
            if bound != package {
                let error = plugin_denied(&format!(
                    "capability `{capability}` is bound to another trusted package"
                ));
                let reason = format!("{}: {}", error.code, error.message);
                self.record(package, capability, "denied", &reason, started);
                return Err(error);
            }
        }

        // 3. Execute under the broker budget and reply limit.
        let broker_request = CapabilityRequest {
            package_instance: package,
            capability,
            payload: request,
            trace_id: &self.trace_id,
        };
        let reply_limit = self.reply_limit;
        let outcome = self
            .handlers
            .get_mut(capability)
            .expect("handler presence was checked above")
            .handler
            .execute(&broker_request, &self.capabilities, &mut self.budget);
        match outcome {
            Ok(contents) => {
                if contents.len() > reply_limit {
                    let error = PluginError::capability(
                        "resource.exhausted",
                        format!(
                            "capability reply is {} bytes; the limit is {reply_limit} bytes",
                            contents.len()
                        ),
                    );
                    let reason = format!("{}: {}", error.code, error.message);
                    self.record(package, capability, "denied", &reason, started);
                    return Err(error);
                }
                self.record(package, capability, "granted", "", started);
                Ok(contents)
            }
            Err(error) => {
                let reason = format!("{}: {}", error.code, error.message);
                let decision = if error.code == "capability.unavailable" {
                    "unavailable"
                } else {
                    "denied"
                };
                self.record(package, capability, decision, &reason, started);
                Err(error)
            }
        }
    }

    fn record(
        &mut self,
        package: &str,
        capability: &str,
        decision: &'static str,
        reason: &str,
        started: Instant,
    ) {
        let elapsed = started.elapsed();
        let duration_micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        if let Ok(mut audit) = self.audit.lock() {
            audit.push(CapabilityAudit {
                package: package.to_string(),
                capability: capability.to_string(),
                decision,
                reason: reason.to_string(),
                duration_micros,
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
                if is_vendor_capability(other) {
                    if self.vendor_policy.contains(other) || self.vendor_policy.contains("*") {
                        Ok(())
                    } else {
                        Err(plugin_denied(&format!(
                            "vendor capability `{other}` has no trusted policy entry for package `{package}`"
                        )))
                    }
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

/// Validate a trusted handler name before registration.
///
/// Registration uses [`CapabilityHandler::name`] as the registry key, so there
/// is no second name argument that could disagree with the handler's declared
/// capability (the RFC's "declared protocol mismatch" rejection).
fn validate_handler_name(name: &str) -> Result<(), PluginError> {
    if name.is_empty() || name.trim() != name {
        return Err(PluginError::capability(
            "capability.handler_invalid",
            "capability handler names must be non-empty and must not contain surrounding whitespace",
        ));
    }
    if RESERVED_HANDLER_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return Err(PluginError::capability(
            "capability.handler_reserved",
            format!("capability handler name `{name}` uses a reserved host namespace"),
        ));
    }
    if !CORE_CAPABILITIES.contains(&name) && !is_vendor_capability(name) {
        return Err(PluginError::capability(
            "capability.handler_invalid",
            format!(
                "`{name}` is not a registrable capability name: expected a core capability or a vendor-namespaced name"
            ),
        ));
    }
    Ok(())
}

/// Effective capability grants for one run.
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    /// Write to program standard output (`打印`).
    pub console: bool,
    /// Grant ceiling for the `math.compute` plugin capability.
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
    /// Whether OS entropy may be read (ADR-0046).
    pub random: bool,
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
            random: false,
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

    /// Grant OS entropy reads (ADR-0046).
    pub fn allow_random(mut self) -> Self {
        self.random = true;
        self
    }

    /// Check the entropy grant.
    pub fn check_random(&self, span: Option<Span>) -> Result<(), RuntimeError> {
        if self.random {
            Ok(())
        } else {
            Err(denied(
                span,
                "OS entropy was not granted",
                "grant it with --allow-random",
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
        // Declaring a vendor capability is not a grant: without a trusted
        // policy entry authorization is denied (RFC-EXT-0001 CAP-01).
        let vendor_denied = broker
            .authorize("demo.solver", "vendor.device.read", b"{}")
            .unwrap_err();
        assert_eq!(vendor_denied.code, "permission.denied");
        broker.allow_vendor("vendor.device.read");
        assert!(broker
            .authorize("demo.solver", "vendor.device.read", b"{}")
            .is_ok());
        // Policy is not a handler: authorize() succeeds but execution still
        // needs an executor and reports `capability.unavailable`.
        assert!(!broker.has_handler("vendor.device.read"));
        assert_eq!(
            broker
                .execute("demo.solver", "vendor.device.read", b"{}")
                .unwrap_err()
                .code,
            "capability.unavailable"
        );
        assert!(!is_vendor_capability("vendor"));
        assert!(!is_vendor_capability("fs.read.extra"));
        let audit = broker.audit();
        let audit = audit.lock().unwrap();
        assert_eq!(audit.len(), 6);
        assert!(audit.iter().any(|entry| entry.decision == "denied"));
        assert!(audit.iter().any(|entry| entry.decision == "unavailable"));
        assert!(audit.iter().all(|entry| entry.duration_micros <= 5_000_000));
    }

    #[test]
    fn every_core_capability_is_understood_by_decide() {
        for name in CORE_CAPABILITIES {
            let broker = CapabilityBroker::new(Capabilities::project());
            let payload: &[u8] = match name {
                "fs.read" => br#"{"path":"data/a.csv"}"#,
                "fs.write" => br#"{"path":"data/a.csv","contents":"x"}"#,
                "net.request" => br#"{"host":"127.0.0.1"}"#,
                _ => b"{}",
            };
            let error = broker
                .decide("demo.solver", name, payload)
                .err()
                .map(|error| error.code);
            assert_ne!(
                error.as_deref(),
                Some("capability.unknown"),
                "core capability `{name}` must be handled by decide"
            );
        }
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
        assert!(broker.has_handler("fs.read"));
        assert_eq!(broker.handler_names(), vec!["fs.read"]);
        assert_eq!(broker.reply_limit, MAX_CAPABILITY_REPLY_BYTES);

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
