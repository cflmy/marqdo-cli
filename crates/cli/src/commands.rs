//! Command implementations.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use marqdo_host::{Host, StdHost};
use marqdo_runtime::{run_with_limits, EvaluatedArg, MemoryLimits, RuntimeError, Value};
use marqdo_sandbox::{PlatformCapabilities, SandboxMode};
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
use crate::loader::{self, FileDiagnostic, LoadedProject};
use crate::output;

fn load_project(file: Option<&Path>, json: bool) -> Result<LoadedProject, i32> {
    let file = file.unwrap_or_else(|| Path::new("index.mq.md"));
    match loader::load(file) {
        Ok(project) => Ok(project),
        Err(error) => {
            if json {
                output::print_json(&json!({"success": false, "error": error}));
            } else {
                eprintln!("error: {error}");
            }
            if error.contains("marqdo.lock") || error.contains("lockfile") {
                Err(1)
            } else {
                Err(2)
            }
        }
    }
}

/// Add plugin export descriptors from manifests without loading a library.
fn add_static_plugin_descriptors(
    project: &mut LoadedProject,
    paths: &[PathBuf],
) -> Result<(), String> {
    for path in paths {
        let manifest_path = if path.is_dir() {
            path.join("plugin.yaml")
        } else {
            path.clone()
        };
        let manifest = marqdo_plugin_abi::PluginManifest::load(&manifest_path)
            .map_err(|error| format!("{}: {}", error.code, error.message))?;
        manifest
            .validate()
            .map_err(|error| format!("{}: {}", error.code, error.message))?;
        let digest = manifest
            .resolve_artifact(&marqdo_plugin_abi::HostTarget::current())
            .map_err(|error| format!("{}: {}", error.code, error.message))?
            .sha256
            .clone();
        for export in &manifest.plugin.exports {
            if project
                .program
                .external_callables
                .iter()
                .any(|callable| callable.public_name == export.name)
            {
                return Err(format!(
                    "plugin export `{}` is declared more than once",
                    export.name
                ));
            }
            let signature = plugin_signature(&export.signature).map_err(|error| {
                format!(
                    "plugin export `{}` has an invalid signature: {error}",
                    export.name
                )
            })?;
            project
                .program
                .external_callables
                .push(marqdo_ir::callable::CallableDescriptor {
                    id: marqdo_ir::callable::CallableId(format!(
                        "plugin:{}:{}",
                        manifest.plugin.name, export.name
                    )),
                    public_name: export.name.clone(),
                    owner_package: manifest.plugin.name.clone(),
                    signature,
                    effects_requested: manifest.plugin.requires.clone(),
                    origin: marqdo_ir::callable::CallableOrigin::AbiPlugin {
                        package: manifest.plugin.name.clone(),
                        export: export.name.clone(),
                    },
                    implementation_digest: Some(digest.clone()),
                    source_span: None,
                });
        }
    }
    if paths.is_empty() {
        return Ok(());
    }
    let existing: Vec<(Option<marqdo_span::Span>, String)> = project
        .diagnostics
        .iter()
        .map(|file| {
            (
                file.diagnostic.span,
                format!("{}:{}", file.module_id, file.diagnostic.message),
            )
        })
        .collect();
    for (module_id, diagnostic) in marqdo_frontend::calls::validate_program(&project.program) {
        let key = format!("{}:{}", module_id, diagnostic.message);
        if existing.contains(&(diagnostic.span, key)) {
            continue;
        }
        if let Some(file) = project.files.get(&module_id) {
            project.diagnostics.push(FileDiagnostic {
                module_id,
                path: file.path.clone(),
                source: file.source.clone(),
                diagnostic,
            });
        }
    }
    Ok(())
}

/// Convert a plugin manifest signature (`($a: Int) -> Int`) to the language
/// parameter list, dropping the display-only return annotation and `$` sigils.
fn plugin_signature(raw: &str) -> Result<marqdo_ir::callable::CallableSignature, String> {
    let before_return = raw.split("->").next().unwrap_or(raw).trim();
    let open = before_return
        .find('(')
        .ok_or_else(|| "missing `(`".to_string())?;
    let close = before_return
        .rfind(')')
        .ok_or_else(|| "missing `)`".to_string())?;
    if close < open {
        return Err("malformed parameter list".to_string());
    }
    let params = before_return[open + 1..close].replace('$', "");
    let parsed =
        marqdo_frontend::signature::parse(&format!("({params})"), marqdo_span::Span::default());
    if parsed.has_errors() {
        return Err(parsed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
            .join("; "));
    }
    Ok(marqdo_ir::callable::CallableSignature {
        params: parsed.params,
        kind: marqdo_ir::SignatureKind::Explicit,
    })
}

/// `marqdo check`.
fn validate_refs(project: &LoadedProject) -> Vec<FileDiagnostic> {
    let mut diagnostics = Vec::new();
    for file in project.files.values() {
        let parent = file.path.parent().unwrap_or_else(|| Path::new("."));
        let mut unresolved: Vec<marqdo_span::Span> = Vec::new();
        file.analysis.document.root.walk(&mut |node| {
            let marqdo_markdown::MdKind::Link { url, .. } = &node.kind else {
                return;
            };
            let (resolved, _, _) = resolve_link(project, &file.analysis.units, parent, url);
            if !resolved {
                unresolved.push(node.span);
            }
        });
        for span in unresolved {
            diagnostics.push(FileDiagnostic {
                module_id: file.module_id.clone(),
                path: file.path.clone(),
                source: file.source.clone(),
                diagnostic: marqdo_value::Diagnostic::error(
                    marqdo_value::codes::REF_UNRESOLVED,
                    Some(span),
                    "reference target could not be resolved",
                )
                .with_hint("fix the link target or add the missing heading/file"),
            });
        }
    }
    diagnostics
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

fn refs_json(project: &LoadedProject) -> serde_json::Value {
    let entry = project.entry_file();
    let parent = entry.path.parent().unwrap_or_else(|| Path::new("."));
    let mut edges = Vec::new();
    entry.analysis.document.root.walk(&mut |node| {
        let marqdo_markdown::MdKind::Link { url, .. } = &node.kind else {
            return;
        };
        let (resolved, target_module, target_unit) =
            resolve_link(project, &entry.analysis.units, parent, url);
        edges.push(json!({
            "source": entry.module_id.to_string(),
            "url": url,
            "span": node.span,
            "resolved": resolved,
            "target_module": target_module,
            "target_unit": target_unit,
        }));
    });
    serde_json::Value::Array(edges)
}

fn resolve_link(
    project: &LoadedProject,
    units: &marqdo_frontend::UnitIndex,
    parent: &Path,
    url: &str,
) -> (bool, Option<String>, Option<String>) {
    if let Some(fragment) = url.strip_prefix('#') {
        let unit = units.anchors.get(fragment).or_else(|| {
            units
                .units
                .iter()
                .find(|unit| unit.name == fragment)
                .map(|unit| &unit.id)
        });
        return match unit {
            Some(unit) => (
                true,
                Some(project.entry.to_string()),
                Some(unit.to_string()),
            ),
            None => (false, None, None),
        };
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return (true, None, None);
    }
    let (path_part, fragment) = match url.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (url, None),
    };
    let candidate = parent.join(path_part);
    let Ok(candidate) = candidate.canonicalize() else {
        return (false, None, None);
    };
    let Some(module_id) = project.module_id_for_path(&candidate) else {
        return (false, None, None);
    };
    let analyzed;
    let target_units = match project.files.get(&module_id) {
        Some(target_file) => &target_file.analysis.units,
        None => {
            // Links may reference documents that are not imported. Analyse the
            // target statically for reference checking; nothing executes.
            let Ok(source) = std::fs::read_to_string(&candidate) else {
                return (false, None, None);
            };
            analyzed = marqdo_frontend::analyze_source(&module_id.to_string(), &source);
            &analyzed.units
        }
    };
    let Some(fragment) = fragment else {
        return (true, Some(module_id.to_string()), None);
    };
    let resolved = target_units.anchors.get(fragment).or_else(|| {
        target_units
            .units
            .iter()
            .find(|unit| unit.name == fragment)
            .map(|unit| &unit.id)
    });
    match resolved {
        Some(unit) => (true, Some(module_id.to_string()), Some(unit.to_string())),
        None => (false, Some(module_id.to_string()), None),
    }
}

/// Re-execute this CLI inside the strict Linux supervisor.
fn launch_strict_process(
    project_root: &Path,
    document: Option<&Path>,
    settings: &SandboxSettings,
) -> i32 {
    let program = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("error: cannot locate the marqdo executable: {error}");
            return 1;
        }
    };
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let output_limit = settings
        .max_output
        .unwrap_or(1024 * 1024)
        .min(64 * 1024 * 1024);
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
    match marqdo_sandbox::launch_strict(&command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            1
        }
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
            location["file"] = json!(located.file.path.display().to_string());
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
        "file": location["file"],
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
