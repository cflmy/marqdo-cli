//! Command implementations.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use marqdo_host::{Host, StdHost};
use marqdo_runtime::{run_with_limits, EvaluatedArg, MemoryLimits, RuntimeError, Value};
use marqdo_sandbox::{PlatformCapabilities, SandboxMode, StrictOutcome};
use marqdo_span::Span;
use serde_json::json;

mod check;
mod init;
mod inspect;
mod package;
mod run;
mod sandbox;
mod tool;

pub use check::check;
pub use init::init;
pub use inspect::inspect;
pub use inspect::InspectFlags;
pub use package::{add, lock, remove, sync};
pub use run::execute;
pub use sandbox::SandboxSettings;
pub use tool::{fmt, plugin_inspect};

use crate::args;
use crate::extension_descriptors::{self, StaticPluginSet};
use crate::loader::{FileDiagnostic, LoadedProject};
use crate::observation::ObservationSession;
use crate::output;
use crate::project_prepare::{self, PrepareOptions, PreparedProgram};

/// Prepare one document through the single composition-root step.
///
/// `check`/`run`/`call`/`inspect` must all use this function so they share the
/// same call bindings and diagnostics (RFC-EXT-0001 §4.4, §18.2).
pub(super) fn prepare_project(
    file: Option<&Path>,
    json: bool,
    plugins: &[PathBuf],
) -> Result<PreparedProgram, i32> {
    let options = PrepareOptions {
        plugins: plugins.to_vec(),
    };
    match project_prepare::prepare(file, &options) {
        Ok(prepared) => Ok(prepared),
        Err(error) => {
            if json {
                output::print_json(&json!({
                    "success": false,
                    "error": {
                        "code": error.code,
                        "message": error.message,
                    }
                }));
            } else {
                eprintln!("error[{}]: {}", error.code, error.message);
            }
            Err(error.exit_code)
        }
    }
}

fn summary_json(project: &LoadedProject) -> serde_json::Value {
    let entry = project.entry_file();
    json!({
        "modules": project.program.modules.len(),
        "functions": entry.analysis.module.functions.len(),
        "units": entry.analysis.units.units.len(),
        "resources": entry.analysis.module.resources.len(),
    })
}

fn signatures_json(module: &marqdo_ir::Module) -> serde_json::Value {
    serde_json::Value::Array(
        module
            .functions
            .iter()
            .map(|function| {
                json!({
                    "name": function.id.name,
                    "local_name": function.local_name,
                    "signature": match function.signature {
                        marqdo_ir::SignatureKind::Explicit => "explicit",
                        marqdo_ir::SignatureKind::Inferred => "inferred",
                    },
                    "params": function.params.iter().map(|param| json!({
                        "name": param.name,
                        "type": param.type_annotation,
                        "has_default": param.default.is_some(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// Machine-readable routing decision for one captured strict execution.
#[derive(Debug, PartialEq, Eq)]
enum StrictRoute {
    /// Forward the captured child bytes unchanged.
    Forward { exit_code: i32 },
    /// Emit exactly one JSON error document instead of the raw child output.
    Failure { code: &'static str, message: String },
}

/// Classify a captured strict execution.
///
/// Machine output is forwarded only if the child emitted one complete
/// current Observation snapshot whose terminal agrees with the exit status.
fn classify_strict_outcome(outcome: &StrictOutcome, snapshot: bool) -> StrictRoute {
    let exit_code = outcome.exit_code.unwrap_or(1);
    if !snapshot {
        return StrictRoute::Forward { exit_code };
    }
    if outcome.exit_code.is_some()
        && crate::observation::is_complete_snapshot(&outcome.stdout, exit_code)
    {
        return StrictRoute::Forward { exit_code };
    }
    if outcome.exit_code.is_none() {
        return StrictRoute::Failure {
            code: "strict.resource_limit",
            message: "strict child was terminated by a resource limit or signal".to_string(),
        };
    }
    let stderr = String::from_utf8_lossy(&outcome.stderr);
    if stderr.contains("namespace") || stderr.contains("bwrap") {
        return StrictRoute::Failure {
            code: "strict.namespace_failure",
            message: "the strict supervisor could not construct the isolation namespaces; refusing to run the project"
                .to_string(),
        };
    }
    StrictRoute::Failure {
        code: "strict.child_invalid",
        message: "the strict child did not produce exactly one JSON observation snapshot"
            .to_string(),
    }
}

/// Route captured child bytes through the selected output contract.
fn route_strict_outcome(outcome: StrictOutcome, observation: Option<&ObservationSession>) -> i32 {
    use std::io::Write;
    match classify_strict_outcome(&outcome, observation.is_some()) {
        StrictRoute::Forward { exit_code } => {
            let mut stdout = std::io::stdout().lock();
            if let Err(error) = stdout
                .write_all(&outcome.stdout)
                .and_then(|()| stdout.flush())
            {
                return run::transport_failure(&error.to_string());
            }
            let _ = std::io::stderr().write_all(&outcome.stderr);
            exit_code
        }
        StrictRoute::Failure { code, message } => {
            eprintln!("error[{code}]: {message}");
            if let Some(observation) = observation {
                let result = if code == "strict.namespace_failure" {
                    observation.rejected("strict", code, &message)
                } else {
                    observation.incomplete()
                };
                if let Err(error) = result {
                    return run::transport_failure(&error);
                }
            }
            outcome.exit_code.filter(|code| *code != 0).unwrap_or(1)
        }
    }
}

fn launch_strict_process(
    project_root: &Path,
    document: Option<&Path>,
    settings: &SandboxSettings,
    observation: Option<&ObservationSession>,
) -> i32 {
    let program = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            let message = format!("cannot locate the marqdo executable: {error}");
            return run::fail(observation, "strict", "strict.launch_failed", &message, 1);
        }
    };
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let program_output_limit = settings
        .max_output
        .unwrap_or(marqdo_host::DEFAULT_OUTPUT_BYTES);
    let output_limit = if observation.is_some() {
        // Allow framing and runtime observations within the existing
        // supervisor capture ceiling. The child's program budget still applies.
        64 * 1024 * 1024
    } else {
        program_output_limit.min(64 * 1024 * 1024)
    };
    let timeout = Duration::from_secs(settings.timeout.unwrap_or(30));
    let stdlib_dir = crate::stdlib::StdlibBundle::discover()
        .ok()
        .flatten()
        .map(|bundle| bundle.root().to_path_buf());
    let command = marqdo_sandbox::StrictHostCommand {
        program: &program,
        args: &args,
        project_root,
        document,
        original_document: document,
        output_limit,
        timeout,
        stdlib_dir: stdlib_dir.as_deref(),
    };
    match marqdo_sandbox::launch_strict_capture(&command) {
        Ok(outcome) => route_strict_outcome(outcome, observation),
        Err(error) => run::fail(observation, "strict", error.code(), &error.to_string(), 1),
    }
}

/// `marqdo run` / `marqdo call`.
fn load_kernel_descriptor(host: &mut StdHost, path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read kernel descriptor `{}`: {error}",
            path.display()
        )
    })?;
    let mut descriptor: marqdo_foreign::KernelDescriptor = serde_json::from_str(&text)
        .map_err(|error| format!("invalid kernel descriptor `{}`: {error}", path.display()))?;
    if descriptor.builtin {
        return Err("external kernel descriptors must not set `builtin: true`".to_string());
    }
    if descriptor.digest.is_none() {
        return Err(
            "external kernel descriptors must declare a `digest` of the provider program"
                .to_string(),
        );
    }
    if descriptor.entrypoint.is_relative() {
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        descriptor.entrypoint = base.join(&descriptor.entrypoint);
    }
    descriptor.entrypoint = descriptor
        .entrypoint
        .canonicalize()
        .map_err(|error| format!("cannot open kernel provider: {error}"))?;
    if !descriptor.entrypoint.is_file() {
        return Err(format!(
            "kernel provider executable `{}` does not exist",
            descriptor.entrypoint.display()
        ));
    }
    host.register_kernel(descriptor)
        .map_err(|error| format!("{}: {}", error.code(), error.message))
}

fn load_plugin_into_host(
    host: &mut StdHost,
    path: &Path,
    trusted: bool,
) -> Result<Vec<String>, String> {
    let manifest_path = if path.is_dir() {
        path.join("plugin.yaml")
    } else {
        path.to_path_buf()
    };
    let base_dir = manifest_path
        .parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest = marqdo_plugin_abi::PluginManifest::load(&manifest_path)
        .map_err(|error| format!("{}: {}", error.code, error.message))?;
    manifest
        .validate()
        .map_err(|error| format!("{}: {}", error.code, error.message))?;
    if manifest.plugin.kind == marqdo_plugin_abi::PluginKind::Native && !trusted {
        return Err(format!(
            "loading `{}` in-process requires --trust-native-plugin; use `kind: worker` for crash isolation",
            manifest.plugin.name
        ));
    }
    host.load_plugin(&manifest, &base_dir, trusted)
        .map_err(|error| format!("{}: {}", error.code(), error.message))
}

fn runtime_location_json(
    project: &LoadedProject,
    module_id: Option<&str>,
    span: Option<marqdo_span::Span>,
) -> serde_json::Value {
    let mut location = json!({
        "module_id": module_id,
        "span": span.map(|span| json!({"start": span.start, "end": span.end})),
    });
    if let (Some(module_id), Some(span)) = (module_id, span) {
        if let Some(located) = project.locate(module_id, span) {
            location["line"] = json!(located.line);
            location["column"] = json!(located.column);
            // Attribution for a shareable snapshot is the project-relative
            // path; an absolute host path is environment data, not contract.
            location["relative_path"] = json!(located.file.module_id.relative_path);
        }
    }
    location
}

fn runtime_error_json(error: &RuntimeError, project: &LoadedProject) -> serde_json::Value {
    let location = runtime_location_json(project, error.module_id.as_deref(), error.span);
    let frames: Vec<serde_json::Value> = error
        .frames
        .iter()
        .map(|frame| {
            let mut value = runtime_location_json(project, frame.module_id.as_deref(), frame.span);
            value["function"] = json!(frame.function);
            value
        })
        .collect();
    json!({
        "code": error.code(),
        "origin": error.origin,
        "message": error.message,
        "module_id": error.module_id,
        "span": location["span"],
        "line": location["line"],
        "column": location["column"],
        "relative_path": location["relative_path"],
        "frames": frames,
    })
}

fn render_runtime_error(error: &RuntimeError, project: &LoadedProject) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "runtime error[{}]: {}\n",
        error.code(),
        error.message
    ));
    if let Some(span) = error.span {
        match error
            .module_id
            .as_deref()
            .and_then(|module| project.locate(module, span))
        {
            Some(located) => out.push_str(&format!(
                "  at {}:{}:{}\n",
                located.file.path.display(),
                located.line,
                located.column
            )),
            None => out.push_str(&format!(
                "  at {}@{}..{}\n",
                error.module_id.as_deref().unwrap_or("<unknown>"),
                span.start,
                span.end
            )),
        }
    }
    for frame in &error.frames {
        out.push_str(&format!("  in function `{}`", frame.function));
        if let Some(span) = frame.span {
            match frame
                .module_id
                .as_deref()
                .and_then(|module| project.locate(module, span))
            {
                Some(located) => out.push_str(&format!(
                    " at {}:{}:{}",
                    located.file.path.display(),
                    located.line,
                    located.column
                )),
                None => {
                    if let Some(module) = &frame.module_id {
                        out.push_str(&format!(" at {module}@{}..{}", span.start, span.end));
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

/// Resolve `--arg` strings into typed arguments.
pub fn parse_named_args(raw: &[String]) -> Result<Vec<(String, Value)>, String> {
    raw.iter()
        .map(|entry| args::parse_named_arg(entry))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(
        exit_code: Option<i32>,
        signal: Option<i32>,
        stdout: &[u8],
        stderr: &[u8],
    ) -> StrictOutcome {
        StrictOutcome {
            exit_code,
            signal,
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    #[test]
    fn strict_json_success_is_forwarded_once() {
        let route = classify_strict_outcome(
            &outcome(Some(0), None, br#"{"schema":"marqdo.observation","schema_version":1,"run":{"id":"r","status":"ok"},"capture":{},"events":[{"schema":"marqdo.observation","schema_version":1,"run_id":"r","seq":0,"kind":"stream.opened","payload":{}},{"schema":"marqdo.observation","schema_version":1,"run_id":"r","seq":1,"kind":"run.completed","payload":{"status":"ok"}}]}"#, b""),
            true,
        );
        assert_eq!(route, StrictRoute::Forward { exit_code: 0 });
    }

    #[test]
    fn strict_json_program_error_is_forwarded_unchanged() {
        let route = classify_strict_outcome(
            &outcome(Some(2), None, br#"{"schema":"marqdo.observation","schema_version":1,"run":{"id":"r","status":"error"},"capture":{},"events":[{"schema":"marqdo.observation","schema_version":1,"run_id":"r","seq":0,"kind":"stream.opened","payload":{}},{"schema":"marqdo.observation","schema_version":1,"run_id":"r","seq":1,"kind":"run.failed","payload":{"status":"error"}}]}"#, b""),
            true,
        );
        assert_eq!(route, StrictRoute::Forward { exit_code: 2 });
    }

    #[test]
    fn strict_json_empty_stdout_is_one_error_document() {
        let route = classify_strict_outcome(
            &outcome(Some(1), None, b"", b"unexpected child noise\n"),
            true,
        );
        match route {
            StrictRoute::Failure { code, message } => {
                assert_eq!(code, "strict.child_invalid");
                assert!(message.contains("exactly one JSON"));
            }
            other => panic!("expected Failure, got {other:?}"),
        }
    }

    #[test]
    fn strict_json_extra_text_is_not_forwarded() {
        let route = classify_strict_outcome(
            &outcome(
                Some(0),
                None,
                br#"{"success":true}
not json"#,
                b"",
            ),
            true,
        );
        assert!(matches!(
            route,
            StrictRoute::Failure {
                code: "strict.child_invalid",
                ..
            }
        ));
    }

    #[test]
    fn strict_namespace_failure_is_classified() {
        let route = classify_strict_outcome(
            &outcome(
                Some(1),
                None,
                b"",
                b"bwrap: Creating new namespace failed: Resource temporarily unavailable\n",
            ),
            true,
        );
        assert!(matches!(
            route,
            StrictRoute::Failure {
                code: "strict.namespace_failure",
                ..
            }
        ));
    }

    #[test]
    fn strict_signal_is_a_resource_limit() {
        let route = classify_strict_outcome(&outcome(None, Some(9), b"", b""), true);
        assert!(matches!(
            route,
            StrictRoute::Failure {
                code: "strict.resource_limit",
                ..
            }
        ));
    }

    #[test]
    fn strict_human_mode_forwards_captured_bytes() {
        let route = classify_strict_outcome(&outcome(Some(1), None, b"", b"boom\n"), false);
        assert_eq!(route, StrictRoute::Forward { exit_code: 1 });
    }

    #[test]
    fn sandbox_errors_expose_stable_codes() {
        use marqdo_sandbox::{SandboxError, StrictFailureKind};
        assert_eq!(
            SandboxError::StrictUnavailable {
                reason: "none".to_string()
            }
            .code(),
            "strict.probe_unavailable"
        );
        assert_eq!(
            SandboxError::launch(StrictFailureKind::SpawnFailed, "x").code(),
            "strict.spawn_failed"
        );
        assert_eq!(
            SandboxError::launch(StrictFailureKind::Timeout, "x").code(),
            "strict.timeout"
        );
    }
}
