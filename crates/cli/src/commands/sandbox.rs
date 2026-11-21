//! Sandbox mode parsing and capability construction.
//!
//! This module owns the CLI-to-`Capabilities` mapping and strict preflight;
//! command implementations only consume the resulting grant set.

use std::path::PathBuf;

use marqdo_host::Capabilities;
use marqdo_sandbox::SandboxMode;

/// Effective sandbox settings from the command line.
#[derive(Debug, Clone, Default)]
pub struct SandboxSettings {
    /// Mode name.
    pub mode: String,
    /// Read grants.
    pub allow_read: Vec<String>,
    /// Write grants.
    pub allow_write: Vec<String>,
    /// Network grants (IP literals, ADR-0034).
    pub allow_net: Vec<String>,
    /// Environment variable grants (ADR-0043).
    pub allow_env: Vec<String>,
    /// Process program grants (ADR-0044).
    pub allow_process: Vec<String>,
    /// OS entropy grant (ADR-0046).
    pub allow_random: bool,
    /// Foreign execution grant.
    pub allow_foreign: bool,
    /// Plugin grants.
    pub allow_plugin: Vec<String>,
    /// Fuel override.
    pub fuel: Option<u64>,
    /// Stack depth override.
    pub stack: Option<usize>,
    /// Output limit override.
    pub max_output: Option<usize>,
    /// Total committed addressable-memory budget override (bytes).
    pub max_memory: Option<usize>,
    /// Per-region committed addressable-memory budget override (bytes).
    pub max_region_memory: Option<usize>,
    /// Wall-clock timeout in seconds.
    pub timeout: Option<u64>,
    /// Explicitly trusted plugin manifests to load.
    pub plugins: Vec<PathBuf>,
    /// Explicitly selected resource provider v2 manifests (RFC-EXT-0001 §5.4).
    pub providers: Vec<PathBuf>,
    /// Exact resource provider name grants (declaration is not a grant).
    pub allow_provider: Vec<String>,
    /// Explicit trust decision for in-process native plugin loading.
    pub trust_native_plugin: bool,
    /// Optional Trace v2 output path.
    pub trace_file: Option<PathBuf>,
    /// Trace event budget override (0 = unlimited).
    pub trace_max_events: Option<usize>,
    /// Trace approximate byte budget override (0 = unlimited).
    pub trace_max_bytes: Option<usize>,
    /// External foreign-kernel descriptor paths.
    pub kernels: Vec<PathBuf>,
}

impl SandboxSettings {
    /// Parse the mode string.
    pub fn sandbox_mode(&self) -> Result<SandboxMode, String> {
        SandboxMode::parse(&self.mode).ok_or_else(|| {
            format!(
                "unknown sandbox mode `{}` (expected pure, project or strict)",
                self.mode
            )
        })
    }

    /// Reject strict-mode requests that this L1 isolation level does not
    /// support. Strict currently covers pure Marqdo and native Math only;
    /// granting Foreign, ABI plugins, resource providers, tools or trace files
    /// would require extending the supervisor contract first.
    pub fn strict_preflight(&self) -> Result<(), String> {
        let requested = self.allow_foreign
            || !self.allow_read.is_empty()
            || !self.allow_net.is_empty()
            || !self.allow_write.is_empty()
            || !self.allow_env.is_empty()
            || !self.allow_process.is_empty()
            || self.allow_random
            || !self.allow_plugin.is_empty()
            || !self.plugins.is_empty()
            || !self.providers.is_empty()
            || !self.allow_provider.is_empty()
            || self.trust_native_plugin
            || !self.kernels.is_empty()
            || self.trace_file.is_some();
        if requested {
            return Err(
                "strict mode currently supports pure Marqdo and native Math only; \
                 --allow-read/--allow-write/--allow-env/--allow-process/--allow-random/\
                 --allow-foreign/--allow-plugin/--plugin/--provider/--allow-provider/\
                 --trust-native-plugin/--kernel/--trace-file are rejected. \
                 Use --mode project for those capabilities."
                    .to_string(),
            );
        }
        Ok(())
    }

    /// Build the effective capability set.
    pub fn capabilities(&self) -> Result<Capabilities, String> {
        let mode = self.sandbox_mode()?;
        match mode {
            SandboxMode::Pure => {
                if !self.allow_read.is_empty()
                    || !self.allow_net.is_empty()
                    || !self.allow_write.is_empty()
                    || !self.allow_env.is_empty()
                    || !self.allow_process.is_empty()
                    || self.allow_random
                    || self.allow_foreign
                    || !self.allow_plugin.is_empty()
                    || !self.providers.is_empty()
                    || !self.allow_provider.is_empty()
                {
                    return Err(
                        "pure mode prohibits external I/O grants and extension providers; \
                         use --mode project"
                            .to_string(),
                    );
                }
                Ok(Capabilities::pure())
            }
            SandboxMode::Project => {
                let mut capabilities = Capabilities::project();
                for pattern in &self.allow_read {
                    capabilities = capabilities.allow_read(pattern.clone());
                }
                for pattern in &self.allow_write {
                    capabilities = capabilities.allow_write(pattern.clone());
                }
                for host in &self.allow_net {
                    capabilities = capabilities.allow_net(host.clone());
                }
                for name in &self.allow_env {
                    capabilities = capabilities.allow_env(name.clone());
                }
                for program in &self.allow_process {
                    capabilities = capabilities.allow_process(program.clone());
                }
                if self.allow_random {
                    capabilities = capabilities.allow_random();
                }
                if self.allow_foreign {
                    capabilities = capabilities.allow_foreign();
                }
                for plugin in &self.allow_plugin {
                    capabilities = capabilities.allow_plugin(plugin.clone());
                }
                for provider in &self.allow_provider {
                    validate_provider_grant(provider)?;
                    capabilities = capabilities.allow_resource_provider(provider.clone());
                }
                Ok(capabilities)
            }
            SandboxMode::Strict => {
                self.strict_preflight()?;
                Ok(Capabilities::project())
            }
        }
    }
}

/// Reject non-exact `--allow-provider` grants.
///
/// Provider grants are exact stable names (RFC-EXT-0001 §5.2): there is no `*`
/// wildcard, no prefix matching and no implicit discovery, so a grant either
/// names a selected manifest or the run fails closed.
fn validate_provider_grant(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("--allow-provider requires a non-empty provider name".to_string());
    }
    if name
        .chars()
        .any(|character| matches!(character, '*' | '?' | '[' | ']'))
    {
        return Err(format!(
            "--allow-provider `{name}`: wildcards are not supported; grants are exact provider names"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(format!(
            "--allow-provider `{name}`: provider names must not contain control characters"
        ));
    }
    Ok(())
}
