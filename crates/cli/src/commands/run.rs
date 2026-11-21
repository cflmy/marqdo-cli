//! `marqdo run` execution assembly.
//!
//! Child module of `commands`: it may use the parent's project loader and
//! runtime helpers, but keeps the run session out of the command table.

use super::*;
use crate::provider_manifest;
use marqdo_sandbox::StrictChildState;

pub fn execute(
    file: Option<&Path>,
    function: Option<&str>,
    named_args: &[(String, Value)],
    settings: &SandboxSettings,
    json: bool,
    trace: bool,
) -> i32 {
    let command_name = if function.is_some() { "call" } else { "run" };
    let prepared = match prepare_project(file, json, &settings.plugins) {
        Ok(prepared) => prepared,
        Err(code) => return code,
    };
    let project = prepared.loaded;
    let static_plugins = prepared.plugins;
    if project.has_errors() {
        if json {
            output::print_json(&json!({
                "command": command_name,
                "success": false,
                "diagnostics": output::diagnostics_json(&project.diagnostics),
            }));
        } else {
            eprint!("{}", output::render_text(&project.diagnostics));
            eprintln!("error: refusing to run a project with compile errors");
        }
        return 1;
    }
    let warnings: Vec<FileDiagnostic> = project
        .diagnostics
        .iter()
        .filter(|item| !item.diagnostic.is_error())
        .cloned()
        .collect();
    if !json && !warnings.is_empty() {
        eprint!("{}", output::render_text(&warnings));
    }
    let mode = match settings.sandbox_mode() {
        Ok(mode) => mode,
        Err(error) => {
            emit_error(json, command_name, "cli.invalid_mode", &error);
            return 2;
        }
    };
    if mode == SandboxMode::Strict {
        if let Err(error) = settings.strict_preflight() {
            emit_error(json, command_name, "strict.unsupported_grant", &error);
            return 2;
        }
        match marqdo_sandbox::inspect_strict_child() {
            // A verified supervisor child may run project code.
            Ok(StrictChildState::VerifiedChild) => {}
            // Not a child: enforce capability and launch under the supervisor.
            Ok(StrictChildState::NotChild) => {
                let platform = PlatformCapabilities::detect();
                if let Err(error) = marqdo_sandbox::enforce(mode, &platform) {
                    emit_error(json, command_name, error.code(), &error.to_string());
                    return 1;
                }
                return launch_strict_process(&project.root, file, settings, json, command_name);
            }
            // A marker without a verified identity is never trusted, and the
            // platform-neutral inspection keeps Linux-only symbols out of this
            // module so macOS/Windows builds compile.
            Err(reason) => {
                let message = format!("strict isolation verification failed: {reason}");
                emit_error(json, command_name, "strict.identity_invalid", &message);
                return 1;
            }
        }
    }
    let platform = PlatformCapabilities::detect();
    if let Err(error) = marqdo_sandbox::enforce(mode, &platform) {
        emit_error(
            json,
            command_name,
            "sandbox.unavailable",
            &error.to_string(),
        );
        return 1;
    }
    let capabilities = match settings.capabilities() {
        Ok(capabilities) => capabilities,
        Err(error) => {
            emit_error(json, command_name, "cli.invalid_grants", &error);
            return 2;
        }
    };
    let stdin = read_stdin();
    let mut host = StdHost::new(mode, capabilities, project.root.clone()).with_stdin(stdin);
    if trace || json || settings.trace_file.is_some() {
        host = host
            .with_trace()
            .with_trace_budget(marqdo_host::TraceBudget {
                max_events: settings
                    .trace_max_events
                    .unwrap_or(marqdo_host::trace::DEFAULT_MAX_TRACE_EVENTS),
                max_bytes: settings
                    .trace_max_bytes
                    .unwrap_or(marqdo_host::trace::DEFAULT_MAX_TRACE_BYTES),
            });
    }
    if let Some(fuel) = settings.fuel {
        host.budget.fuel = fuel;
    }
    if let Some(stack) = settings.stack {
        host.budget.stack_limit = stack;
    }
    if let Some(max_output) = settings.max_output {
        host.budget.max_output_bytes = max_output;
    }
    if let Some(timeout) = settings.timeout {
        host.budget.deadline = Some(Instant::now() + Duration::from_secs(timeout));
    }
    for kernel_path in &settings.kernels {
        if let Err(error) = load_kernel_descriptor(&mut host, kernel_path) {
            if json {
                output::print_json(&json!({"success": false, "error": error}));
            } else {
                eprintln!("error: {error}");
            }
            return 2;
        }
    }
    // T6: verify every selected artifact/path/hash before starting a worker or
    // loading native code. Static path/signature checks happened in `prepare`.
    //
    // Provider selection is host assembly, not callable semantics: `check`
    // never sees it. Parse and verify every `--provider` manifest, resolve the
    // current target artifact, check the exact grant and reject duplicate
    // names/kinds *before* any provider or plugin worker process is spawned
    // (RFC-EXT-0001 §5.4.6/§18.2 T6–T7).
    let providers = match provider_manifest::read_provider_manifests(&settings.providers) {
        Ok(providers) => providers,
        Err(error) => {
            emit_prepare_error(json, command_name, &error);
            return error.exit_code;
        }
    };
    // `--provider` alone never authorizes; the exact `--allow-provider NAME`
    // is required before the helper process may be started. An ungranted
    // provider fails closed instead of being silently ignored.
    for provider in &providers {
        if !settings
            .allow_provider
            .iter()
            .any(|grant| grant == &provider.name)
        {
            let error = extension_descriptors::PrepareError::load(
                "permission.denied",
                format!(
                    "resource provider `{}` was selected with --provider but is not granted; \
                     add --allow-provider {}",
                    provider.name, provider.name
                ),
            );
            emit_prepare_error(json, command_name, &error);
            return error.exit_code;
        }
    }
    // A grant that matches no selected provider is a deterministic option
    // error: providers are never discovered from PATH, the working directory or
    // the network.
    for grant in &settings.allow_provider {
        if !providers.iter().any(|provider| &provider.name == grant) {
            let error = extension_descriptors::PrepareError::option(
                "ext.provider_grant_unmatched",
                format!(
                    "--allow-provider `{grant}` matches no selected --provider manifest; \
                     providers are never discovered automatically"
                ),
            );
            emit_prepare_error(json, command_name, &error);
            return error.exit_code;
        }
    }
    for plugin in &static_plugins.plugins {
        if let Err(error) = extension_descriptors::verify_bundle_regular_file(
            &plugin.base_dir,
            &plugin.artifact_relative,
        ) {
            emit_prepare_error(json, command_name, &error);
            return error.exit_code;
        }
        let artifact = plugin.artifact_path();
        if let Err(error) = extension_descriptors::verify_sha256(&artifact, &plugin.artifact_sha256)
        {
            emit_prepare_error(json, command_name, &error);
            return error.exit_code;
        }
    }
    // T7: start only selected, granted and verified extensions.
    for plugin in &static_plugins.plugins {
        if let Err(message) = load_plugin_into_host(
            &mut host,
            &plugin.manifest_path,
            settings.trust_native_plugin,
        ) {
            if json {
                output::print_json(&json!({"success": false, "error": message}));
            } else {
                eprintln!("error: {message}");
            }
            return 1;
        }
    }
    // T8: exact one-to-one reconciliation with the static descriptors. Never
    // synthesise a missing signature (`params: Vec::new()`) or guess an export.
    if let Err(message) = reconcile_plugin_exports(&static_plugins, &host) {
        emit_error(json, command_name, "ext.plugin_binding_mismatch", &message);
        return 1;
    }
    // T7b: start only granted and verified provider helpers. Every manifest,
    // artifact, target, grant and duplicate-kind check above already passed, so
    // no worker is spawned for a provider that failed verification.
    for provider in &providers {
        if let Err(error) = provider_manifest::load_resource_provider_into_host(
            &mut host,
            provider,
            &settings.allow_provider,
        ) {
            emit_prepare_error(json, command_name, &error);
            return error.exit_code;
        }
    }
    let function_name = function.unwrap_or("main").to_string();
    let entry_module = &project.program.modules[&project.entry];
    if entry_module.function(&function_name).is_none() {
        let message = if function.is_none() {
            format!(
                "{} has no callable `main`; declare an Exec H2 (`## @exec main`) or pass an \
                 explicit function name",
                project.entry_file().path.display()
            )
        } else {
            format!(
                "no function named `{function_name}` in {}",
                project.entry_file().path.display()
            )
        };
        if json {
            emit_error(true, command_name, "sem.no_entry_function", &message);
        } else {
            eprintln!("error[sem.no_entry_function]: {message}");
        }
        return 1;
    }
    let runtime_args: Vec<EvaluatedArg> = named_args
        .iter()
        .map(|(name, value)| EvaluatedArg::named(name.clone(), value.clone(), Span::default()))
        .collect();
    let memory_limits = MemoryLimits {
        max_total_bytes: settings
            .max_memory
            .unwrap_or(marqdo_runtime::DEFAULT_MAX_TOTAL_BYTES),
        max_region_bytes: settings
            .max_region_memory
            .unwrap_or(marqdo_runtime::DEFAULT_MAX_REGION_BYTES),
    };
    let result = run_with_limits(
        &project.program,
        &mut host,
        &project.entry,
        &function_name,
        runtime_args,
        memory_limits,
    );
    if let Some(path) = &settings.trace_file {
        let status = match &result {
            Ok(_) => "ok",
            Err(error) => error.code(),
        };
        let entry = format!("{}::{}", project.entry, function_name);
        if let Err(error) = crate::trace::write(&project, &host, &entry, status, path) {
            emit_error(json, command_name, "cli.trace_failed", &error);
            return 1;
        }
    }
    let stdout = host.take_output();
    let stdout_text = String::from_utf8_lossy(&stdout).to_string();
    let stderr = host.take_error();
    let stderr_text = String::from_utf8_lossy(&stderr).to_string();
    let trace_entries = host.trace().to_vec();
    match result {
        Ok(outcome) => {
            if json {
                output::print_json(&json!({
                    "command": command_name,
                    "success": true,
                    "value": marqdo_value::to_wire(&outcome.value),
                    "stdout": stdout_text,
                    "stderr": stderr_text,
                    "diagnostics": output::diagnostics_json(&project.diagnostics),
                    "trace": trace_entries,
                }));
            } else {
                use std::io::Write;
                if !stderr.is_empty() {
                    let _ = std::io::stderr().write_all(&stderr);
                }
                let _ = std::io::stdout().write_all(&stdout);
                if function.is_some() && outcome.value != Value::Unit {
                    println!("{}", outcome.value.display());
                }
                if trace {
                    for entry in &trace_entries {
                        eprintln!(
                            "trace: {} {} {}us {}",
                            entry.op, entry.module, entry.duration_micros, entry.result
                        );
                    }
                }
            }
            0
        }
        Err(error) => {
            if json {
                output::print_json(&json!({
                    "command": command_name,
                    "success": false,
                    "stdout": stdout_text,
                    "stderr": stderr_text,
                    "error": runtime_error_json(&error, &project),
                    "trace": trace_entries,
                }));
            } else {
                use std::io::Write;
                if !stderr.is_empty() {
                    let _ = std::io::stderr().write_all(&stderr);
                }
                let _ = std::io::stdout().write_all(&stdout);
                eprint!("{}", render_runtime_error(&error, &project));
            }
            1
        }
    }
}

fn emit_prepare_error(
    json: bool,
    command_name: &str,
    error: &crate::extension_descriptors::PrepareError,
) {
    if json {
        output::print_json(&json!({
            "command": command_name,
            "success": false,
            "error": {"code": error.code, "message": error.message},
        }));
    } else {
        eprintln!("error[{}]: {}", error.code, error.message);
    }
}

/// Compare the Host's actual plugin exports with the static manifest set.
///
/// Names, raw signatures, owners and verified artifact digests must match
/// exactly; extra, missing or altered exports fail closed with
/// `ext.plugin_binding_mismatch` and the main function never runs.
fn reconcile_plugin_exports(
    static_plugins: &StaticPluginSet,
    host: &StdHost,
) -> Result<(), String> {
    let mut expected: std::collections::BTreeMap<String, (String, String, String)> =
        std::collections::BTreeMap::new();
    for plugin in &static_plugins.plugins {
        for export in &plugin.exports {
            expected.insert(
                export.name.clone(),
                (
                    export.signature.clone(),
                    plugin.artifact_sha256.clone(),
                    plugin.package.clone(),
                ),
            );
        }
    }
    let loaded = host.plugin_exports();
    if loaded.len() != expected.len() {
        return Err(format!(
            "static plugin descriptors declare {} export(s) but the runtime loaded {}; \
             refusing to execute",
            expected.len(),
            loaded.len()
        ));
    }
    for (name, signature, digest, package) in loaded {
        let Some((expected_signature, expected_digest, expected_package)) = expected.get(&name)
        else {
            return Err(format!(
                "runtime export `{name}` was not declared by any selected plugin manifest"
            ));
        };
        if &signature != expected_signature {
            return Err(format!(
                "export `{name}` signature mismatch: manifest `{expected_signature}`, runtime `{signature}`"
            ));
        }
        if &digest != expected_digest {
            return Err(format!(
                "export `{name}` artifact digest mismatch: manifest `{expected_digest}`, runtime `{digest}`"
            ));
        }
        if &package != expected_package {
            return Err(format!(
                "export `{name}` owner mismatch: manifest `{expected_package}`, runtime `{package}`"
            ));
        }
    }
    Ok(())
}

/// Read piped stdin once at the composition boundary.
///
/// A terminal stdin is not read, so `marqdo run` never blocks waiting for an
/// interactive EOF. Non-terminal input is a finite pipe/file and is copied into
/// the run-scoped `stdio/stdin` resource.
fn read_stdin() -> Vec<u8> {
    use std::io::{IsTerminal, Read};
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if let Err(error) = stdin.read_to_end(&mut bytes) {
        eprintln!("error: cannot read stdin: {error}");
        return Vec::new();
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_host() -> StdHost {
        StdHost::new(
            SandboxMode::Project,
            marqdo_host::Capabilities::pure(),
            std::env::temp_dir(),
        )
    }

    fn static_plugin_with_export() -> StaticPluginSet {
        let descriptor = marqdo_ir::callable::CallableDescriptor {
            id: marqdo_ir::callable::CallableId("plugin:demo.ext:solver.add".into()),
            public_name: "solver.add".into(),
            owner_package: "demo.ext".into(),
            signature: marqdo_ir::callable::CallableSignature {
                params: vec![
                    marqdo_ir::Param {
                        name: "a".into(),
                        span: Span::default(),
                        type_annotation: None,
                        default: None,
                    },
                    marqdo_ir::Param {
                        name: "b".into(),
                        span: Span::default(),
                        type_annotation: None,
                        default: None,
                    },
                ],
                kind: marqdo_ir::SignatureKind::Explicit,
            },
            effects_requested: vec!["math.compute".into()],
            origin: marqdo_ir::callable::CallableOrigin::AbiPlugin {
                package: "demo.ext".into(),
                export: "solver.add".into(),
            },
            implementation_digest: Some("a".repeat(64)),
            source_span: None,
        };
        StaticPluginSet {
            plugins: vec![crate::extension_descriptors::StaticPlugin {
                manifest_path: PathBuf::from("/fixtures/demo/plugin.yaml"),
                base_dir: PathBuf::from("/fixtures/demo"),
                package: "demo.ext".into(),
                version: "0.1.0".into(),
                kind: marqdo_plugin_abi::PluginKind::Worker,
                manifest_sha256: format!("sha256:{}", "b".repeat(64)),
                artifact_target: marqdo_plugin_abi::HostTarget::current()
                    .triple()
                    .to_string(),
                artifact_sha256: "a".repeat(64),
                artifact_relative: "build/plugin.so".into(),
                exports: vec![crate::extension_descriptors::StaticPluginExport {
                    name: "solver.add".into(),
                    signature: "($a: Int, $b: Int) -> Int".into(),
                    descriptor,
                }],
                requires: Vec::new(),
            }],
        }
    }

    #[test]
    fn reconcile_accepts_matching_empty_sets() {
        assert!(
            reconcile_plugin_exports(&StaticPluginSet::default(), &empty_host()).is_ok(),
            "no selected plugin and no loaded plugin must reconcile"
        );
    }

    #[test]
    fn reconcile_rejects_missing_runtime_exports() {
        let error = reconcile_plugin_exports(&static_plugin_with_export(), &empty_host())
            .expect_err("a declared export that the runtime did not load must fail closed");
        assert!(
            error.contains("declare 1 export(s)") || error.contains("not declared"),
            "{error}"
        );
    }
}
