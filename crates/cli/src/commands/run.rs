//! `marqdo run` execution assembly.
//!
//! Child module of `commands`: it may use the parent's project loader and
//! runtime helpers, but keeps the run session out of the command table.

use super::*;

pub fn execute(
    file: Option<&Path>,
    function: Option<&str>,
    named_args: &[(String, Value)],
    settings: &SandboxSettings,
    json: bool,
    trace: bool,
) -> i32 {
    let command_name = if function.is_some() { "call" } else { "run" };
    let mut project = match load_project(file, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    if let Err(error) = add_static_plugin_descriptors(&mut project, &settings.plugins) {
        if json {
            output::print_json(&json!({"success": false, "error": error}));
        } else {
            eprintln!("error: {error}");
        }
        return 2;
    }
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
            eprintln!("error: {error}");
            return 2;
        }
    };
    if mode == SandboxMode::Strict {
        if let Err(error) = settings.strict_preflight() {
            if json {
                output::print_json(&json!({"success": false, "error": error}));
            } else {
                eprintln!("error: {error}");
            }
            return 2;
        }
        if marqdo_sandbox::marker_present() {
            // We were re-executed by the supervisor: validate the marker set
            // before running any project code.
            if let Err(reason) = marqdo_sandbox::verify_active() {
                if json {
                    output::print_json(&json!({
                        "success": false,
                        "error": format!("strict isolation verification failed: {reason}"),
                    }));
                } else {
                    eprintln!("error: strict isolation verification failed: {reason}");
                }
                return 1;
            }
        } else {
            let platform = PlatformCapabilities::detect();
            if let Err(error) = marqdo_sandbox::enforce(mode, &platform) {
                if json {
                    output::print_json(&json!({"success": false, "error": error.to_string()}));
                } else {
                    eprintln!("error: {error}");
                }
                return 1;
            }
            return launch_strict_process(&project.root, file, settings);
        }
    }
    let platform = PlatformCapabilities::detect();
    if let Err(error) = marqdo_sandbox::enforce(mode, &platform) {
        if json {
            output::print_json(&json!({"success": false, "error": error.to_string()}));
        } else {
            eprintln!("error: {error}");
        }
        return 1;
    }
    let capabilities = match settings.capabilities() {
        Ok(capabilities) => capabilities,
        Err(error) => {
            eprintln!("error: {error}");
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
    for plugin_path in &settings.plugins {
        if let Err(message) =
            load_plugin_into_host(&mut host, plugin_path, settings.trust_native_plugin)
        {
            if json {
                output::print_json(&json!({"success": false, "error": message}));
            } else {
                eprintln!("error: {message}");
            }
            return 1;
        }
    }
    // Expose the complete export names loaded from trusted manifests so the
    // runtime resolves exactly those names instead of probing namespaces.
    for (name, _signature, digest, package) in host.plugin_exports() {
        if project
            .program
            .external_callables
            .iter()
            .any(|callable| callable.public_name == name)
        {
            continue;
        }
        project
            .program
            .external_callables
            .push(marqdo_ir::callable::CallableDescriptor {
                id: marqdo_ir::callable::CallableId(format!("plugin:{package}:{name}")),
                public_name: name.clone(),
                owner_package: package.clone(),
                signature: marqdo_ir::callable::CallableSignature {
                    params: Vec::new(),
                    kind: marqdo_ir::SignatureKind::Explicit,
                },
                effects_requested: vec!["host".to_string()],
                origin: marqdo_ir::callable::CallableOrigin::AbiPlugin {
                    package,
                    export: name,
                },
                implementation_digest: Some(digest),
                source_span: None,
            });
    }
    let function_name = function.unwrap_or("main").to_string();
    let entry_module = &project.program.modules[&project.entry];
    if entry_module.function(&function_name).is_none() {
        if function.is_none() {
            eprintln!(
                "error[sem.no_entry_function]: {} has no callable `main`; declare an Exec H2 \
                 (`## @exec main`) or pass an explicit function name",
                project.entry_file().path.display()
            );
        } else {
            eprintln!(
                "error: no function named `{function_name}` in {}",
                project.entry_file().path.display()
            );
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
            eprintln!("error: {error}");
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
