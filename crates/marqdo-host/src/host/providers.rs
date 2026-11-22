//! Trusted resource-provider registration (ADR-0033/0035/0036/0037/0038).
//!
//! Registration is host-side installation metadata; language imports cannot
//! install or authorize a provider. Native libraries and helper processes are
//! trusted, not sandboxed.

use super::*;

impl StdHost {
    /// Register an explicitly trusted in-process provider (ADR-0033).
    ///
    /// In-process providers share the host address space and are not a
    /// sandbox; only trusted adapters may be registered. Registration is not a
    /// language operation and does not grant access by itself.
    pub fn register_resource_provider(
        &mut self,
        provider: Box<dyn ResourceProvider>,
    ) -> Result<(), RuntimeError> {
        self.register_provider_checked(provider)
    }

    /// Register a trusted C ABI resource provider (ADR-0036).
    ///
    /// The shared library runs in the host address space and is **trusted**, not
    /// sandboxed. Registration still requires the provider-name grant at
    /// `resource.open` and is project-mode only.
    pub fn register_c_resource_provider(
        &mut self,
        name: &str,
        kinds: &[&str],
        library: &std::path::Path,
    ) -> Result<(), RuntimeError> {
        let kinds = kinds
            .iter()
            .map(|kind| (*kind).to_string())
            .collect::<Vec<_>>();
        let provider = CResourceProvider::open(name, &kinds, library)?;
        self.register_provider_checked(Box::new(provider))
    }

    /// Register a trusted C ABI provider from a `marqdo.provider` manifest
    /// (ADR-0037). The manifest is host-side installation metadata; language
    /// imports cannot install or authorize a provider.
    pub fn register_provider_package(
        &mut self,
        manifest_path: &std::path::Path,
    ) -> Result<(), RuntimeError> {
        let provider = load_package(manifest_path)?;
        self.register_provider_checked(Box::new(provider))
    }

    /// Register a trusted worker-process resource provider (ADR-0035).
    ///
    /// The helper speaks framed JSON over stdin/stdout and is **not** an OS
    /// sandbox; only explicitly trusted helpers may be spawned.
    pub fn register_worker_resource_provider(
        &mut self,
        name: &str,
        kinds: &[&str],
        program: &std::path::Path,
        args: &[String],
    ) -> Result<(), RuntimeError> {
        let kinds = kinds
            .iter()
            .map(|kind| (*kind).to_string())
            .collect::<Vec<_>>();
        let provider = WorkerResourceProvider::spawn(name, &kinds, program, args)?;
        self.register_provider_checked(Box::new(provider))
    }

    fn register_provider_checked(
        &mut self,
        provider: Box<dyn ResourceProvider>,
    ) -> Result<(), RuntimeError> {
        if provider.protocol_version() != 1 {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                None,
                format!(
                    "resource provider `{}` uses unsupported protocol version {}",
                    provider.name(),
                    provider.protocol_version()
                ),
            ));
        }
        let name = provider.name().to_string();
        if name.is_empty() {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                None,
                "resource provider name is empty".to_string(),
            ));
        }
        if is_core_resource_kind(&name) {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                None,
                format!("resource provider name `{name}` is a reserved core resource kind"),
            ));
        }
        let kinds = provider.kind_strings();
        if kinds.is_empty() {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                None,
                format!("resource provider `{name}` declares no kinds"),
            ));
        }
        for kind in &kinds {
            if kind.is_empty() {
                return Err(RuntimeError::new(
                    ErrorCategory::ResourceUnsupported,
                    None,
                    format!("resource provider `{name}` declares an empty kind"),
                ));
            }
            if is_core_resource_kind(kind) {
                return Err(RuntimeError::new(
                    ErrorCategory::ResourceUnsupported,
                    None,
                    format!("resource provider `{name}` declares reserved core kind `{kind}`"),
                ));
            }
            if let Some((existing, _)) = self.resource_providers.get(kind) {
                return Err(RuntimeError::new(
                    ErrorCategory::ResourceUnsupported,
                    None,
                    format!(
                        "resource kind `{kind}` is already registered by provider `{existing}`"
                    ),
                ));
            }
        }
        let provider: SharedResourceProvider = std::sync::Arc::new(std::sync::Mutex::new(provider));
        for kind in kinds {
            self.resource_providers
                .insert(kind, (name.clone(), std::sync::Arc::clone(&provider)));
        }
        Ok(())
    }
}
