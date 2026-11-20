//! Command implementations.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use marqdo_host::{Capabilities, Host, StdHost};
use marqdo_runtime::{run_with_limits, EvaluatedArg, MemoryLimits, RuntimeError, Value};
use marqdo_sandbox::{PlatformCapabilities, SandboxMode};
use marqdo_span::Span;
use serde_json::json;

use crate::args;
use crate::loader::{self, FileDiagnostic, LoadedProject};
use crate::output;

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
    /// granting Foreign, ABI plugins, tools or trace files would require
    /// extending the supervisor contract first.
    pub fn strict_preflight(&self) -> Result<(), String> {
        let requested = self.allow_foreign
            || !self.allow_read.is_empty()
            || !self.allow_net.is_empty()
            || !self.allow_write.is_empty()
            || !self.allow_plugin.is_empty()
            || !self.plugins.is_empty()
            || self.trust_native_plugin
            || !self.kernels.is_empty()
            || self.trace_file.is_some();
        if requested {
            return Err(
                "strict mode currently supports pure Marqdo and native Math only; \
                 --allow-read/--allow-write/--allow-foreign/--allow-plugin/--plugin/\
                 --trust-native-plugin/--kernel/--trace-file are rejected. Use \
                 --mode project for those capabilities."
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
                    || self.allow_foreign
                    || !self.allow_plugin.is_empty()
                {
                    return Err(
                        "pure mode prohibits external I/O grants; use --mode project".to_string(),
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
                if self.allow_foreign {
                    capabilities = capabilities.allow_foreign();
                }
                for plugin in &self.allow_plugin {
                    capabilities = capabilities.allow_plugin(plugin.clone());
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

/// `marqdo init`.
pub fn init(dir: Option<&Path>) -> i32 {
    let dir = dir.unwrap_or_else(|| Path::new("."));
    if dir.join(marqdo_package::MANIFEST_FILE).exists() {
        eprintln!(
            "error: `{}` already contains {}",
            dir.display(),
            marqdo_package::MANIFEST_FILE
        );
        return 1;
    }
    let name = dir
        .canonicalize()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "example".to_string());
    if let Err(error) = std::fs::create_dir_all(dir) {
        eprintln!("error: cannot create {}: {error}", dir.display());
        return 1;
    }
    let manifest = marqdo_package::Manifest::template(&name);
    if let Err(error) = std::fs::write(dir.join(marqdo_package::MANIFEST_FILE), manifest.to_yaml())
    {
        eprintln!("error: cannot write manifest: {error}");
        return 1;
    }
    let entry = dir.join("index.mq.md");
    if let Err(error) = std::fs::write(&entry, INIT_TEMPLATE) {
        eprintln!("error: cannot write {}: {error}", entry.display());
        return 1;
    }
    let _ = std::fs::write(dir.join(".gitignore"), ".marqdo/\n");
    println!("Initialized Marqdo project `{name}` in {}", dir.display());
    println!("Next: marqdo check index.mq.md && marqdo run index.mq.md");
    0
}

const INIT_TEMPLATE: &str = "## main\n\n**问候 = \"你好，Marqdo\"**\n\n> 打印(问候)\n\n*问候*\n";

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
pub fn check(
    file: Option<&Path>,
    json: bool,
    refs: bool,
    plugins: &[PathBuf],
    emit: Option<&Path>,
) -> i32 {
    let mut project = match load_project(file, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    if let Err(error) = add_static_plugin_descriptors(&mut project, plugins) {
        if json {
            output::print_json(&json!({"success": false, "error": error}));
        } else {
            eprintln!("error: {error}");
        }
        return 2;
    }
    if refs {
        let ref_diagnostics = validate_refs(&project);
        project.diagnostics.extend(ref_diagnostics);
    }
    let errors = output::error_count(&project.diagnostics);
    if let Some(path) = emit {
        if errors > 0 {
            let _ = std::fs::remove_file(path);
        } else if let Err(error) = crate::analysis::write(&project, path) {
            eprintln!("error: {error}");
            return 2;
        }
    }
    if json {
        output::print_json(&json!({
            "command": "check",
            "success": errors == 0,
            "errors": errors,
            "diagnostics": output::diagnostics_json(&project.diagnostics),
        }));
    } else {
        let text = output::render_text(&project.diagnostics);
        eprint!("{text}");
        if errors == 0 {
            let checked = project
                .program
                .modules
                .keys()
                .filter(|id| !id.instance.starts_with("std:"))
                .count();
            println!("checked {checked} module(s): ok");
        } else {
            eprintln!(
                "check failed: {} error(s), {} warning(s)",
                errors,
                project.diagnostics.len() - errors
            );
        }
    }
    if errors == 0 {
        0
    } else {
        1
    }
}

/// Options for `marqdo inspect`.
#[derive(Debug, Clone, Default)]
pub struct InspectFlags {
    /// Show the Markdown AST.
    pub ast: bool,
    /// Show heading units.
    pub units: bool,
    /// Show inferred/explicit signatures.
    pub signature: bool,
    /// Show executable IR.
    pub ir: bool,
    /// Show link reference edges.
    pub refs: bool,
    /// Show platform capability report.
    pub platform: bool,
}

/// `marqdo inspect`.
pub fn inspect(file: Option<&Path>, flags: &InspectFlags, json: bool) -> i32 {
    let project = match load_project(file, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let entry = project.entry_file();
    let module = &entry.analysis.module;
    let selected_any =
        flags.ast || flags.units || flags.signature || flags.ir || flags.refs || flags.platform;
    if json {
        let mut object = serde_json::Map::new();
        object.insert("command".into(), json!("inspect"));
        object.insert("module".into(), json!(entry.module_id.to_string()));
        if !selected_any {
            object.insert("summary".into(), summary_json(&project));
        }
        if flags.ast {
            object.insert(
                "ast".into(),
                serde_json::to_value(&entry.analysis.document.root).unwrap_or_default(),
            );
        }
        if flags.units {
            object.insert(
                "units".into(),
                serde_json::to_value(&entry.analysis.units.units).unwrap_or_default(),
            );
        }
        if flags.signature {
            object.insert("signatures".into(), signatures_json(module));
        }
        if flags.ir {
            object.insert(
                "ir".into(),
                serde_json::to_value(&module.functions).unwrap_or_default(),
            );
        }
        if flags.refs {
            object.insert("refs".into(), json!(refs_json(&project)));
        }
        if flags.platform {
            object.insert(
                "platform".into(),
                serde_json::to_value(PlatformCapabilities::detect()).unwrap_or_default(),
            );
        }
        output::print_json(&serde_json::Value::Object(object));
        return 0;
    }
    if !selected_any {
        println!("module: {}", entry.module_id);
        println!("path: {}", entry.path.display());
        println!("project root: {}", project.root.display());
        println!("functions:");
        for function in &module.functions {
            let params = function
                .params
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let kind = match function.signature {
                marqdo_ir::SignatureKind::Explicit => "explicit",
                marqdo_ir::SignatureKind::Inferred => "inferred",
            };
            println!("  {} ({params}) [{kind}]", function.id.name);
        }
        println!(
            "resources: {} formula/code block(s)",
            module.resources.len()
        );
        return 0;
    }
    if flags.ast {
        println!(
            "{}",
            serde_json::to_string_pretty(&entry.analysis.document.root).unwrap_or_default()
        );
    }
    if flags.units {
        println!(
            "{}",
            serde_json::to_string_pretty(&entry.analysis.units.units).unwrap_or_default()
        );
    }
    if flags.signature {
        println!(
            "{}",
            serde_json::to_string_pretty(&signatures_json(module)).unwrap_or_default()
        );
    }
    if flags.ir {
        println!(
            "{}",
            serde_json::to_string_pretty(&module.functions).unwrap_or_default()
        );
    }
    if flags.refs {
        println!(
            "{}",
            serde_json::to_string_pretty(&refs_json(&project)).unwrap_or_default()
        );
    }
    if flags.platform {
        println!(
            "{}",
            serde_json::to_string_pretty(&PlatformCapabilities::detect()).unwrap_or_default()
        );
    }
    0
}

/// Validate Markdown reference edges for every loaded document.
///
/// Links are static references: checking them never executes anything. Broken
/// same-document anchors, missing files and missing target anchors are
/// `ref.unresolved` errors; external HTTP(S) links are accepted as external.
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
    let mut host = StdHost::new(mode, capabilities, project.root.clone());
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
        eprintln!(
            "error: no function named `{function_name}` in {}",
            project.entry_file().path.display()
        );
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
    let trace_entries = host.trace().to_vec();
    match result {
        Ok(outcome) => {
            if json {
                output::print_json(&json!({
                    "command": command_name,
                    "success": true,
                    "value": marqdo_value::to_wire(&outcome.value),
                    "stdout": stdout_text,
                    "diagnostics": output::diagnostics_json(&project.diagnostics),
                    "trace": trace_entries,
                }));
            } else {
                use std::io::Write;
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
                    "error": runtime_error_json(&error, &project),
                    "trace": trace_entries,
                }));
            } else {
                use std::io::Write;
                let _ = std::io::stdout().write_all(&stdout);
                eprint!("{}", render_runtime_error(&error, &project));
            }
            1
        }
    }
}

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

/// `marqdo fmt`.
pub fn fmt(file: Option<&Path>, check_only: bool) -> i32 {
    let file = file.unwrap_or_else(|| Path::new("index.mq.md"));
    let Ok(source) = std::fs::read_to_string(file) else {
        eprintln!("error: cannot read `{}`", file.display());
        return 2;
    };
    let mut formatted = source.replace("\r\n", "\n");
    while formatted.ends_with("\n\n") {
        formatted.pop();
    }
    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }
    if formatted == source {
        return 0;
    }
    if check_only {
        eprintln!(
            "error: `{}` is not formatted (trailing whitespace/newlines or CRLF)",
            file.display()
        );
        return 1;
    }
    if let Err(error) = std::fs::write(file, formatted) {
        eprintln!("error: cannot write `{}`: {error}", file.display());
        return 1;
    }
    0
}

/// `marqdo plugin inspect`.
pub fn plugin_inspect(target: &Path, json: bool) -> i32 {
    let manifest_path = if target.is_dir() {
        target.join("plugin.yaml")
    } else {
        target.to_path_buf()
    };
    let base_dir = manifest_path
        .parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest = match marqdo_plugin_abi::PluginManifest::load(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            report_plugin_error(&error, json);
            return 1;
        }
    };
    if let Err(error) = manifest.validate() {
        report_plugin_error(&error, json);
        return 1;
    }
    let host_target = marqdo_plugin_abi::HostTarget::current();
    let artifact = match manifest.verify_artifact(&base_dir, &host_target) {
        Ok((path, artifact)) => {
            let abi = match manifest.abi_requirement() {
                Ok(requirement) => {
                    if requirement.is_compatible_with(marqdo_plugin_abi::AbiVersion::current()) {
                        "compatible"
                    } else {
                        "incompatible"
                    }
                }
                Err(error) => {
                    report_plugin_error(&error, json);
                    return 1;
                }
            };
            (path, artifact, abi)
        }
        Err(error) => {
            report_plugin_error(&error, json);
            return 1;
        }
    };
    let (artifact_path, artifact_info, abi) = artifact;
    if json {
        output::print_json(&json!({
            "command": "plugin.inspect",
            "success": true,
            "plugin": manifest.plugin.name,
            "version": manifest.plugin.version,
            "abi": manifest.plugin.abi,
            "abi_compatibility": abi,
            "target": host_target.triple().as_str(),
            "artifact": {
                "file": artifact_info.file,
                "path": artifact_path.display().to_string(),
                "sha256": artifact_info.sha256,
                "verified": true,
            },
            "exports": manifest.plugin.exports.iter().map(|export| json!({
                "name": export.name,
                "signature": export.signature,
            })).collect::<Vec<_>>(),
            "loaded": false,
        }));
    } else {
        println!(
            "plugin: {} {}",
            manifest.plugin.name, manifest.plugin.version
        );
        println!("abi: {} ({abi})", manifest.plugin.abi);
        println!("target: {}", host_target.triple().as_str());
        println!(
            "artifact: {} (sha256 verified: {})",
            artifact_path.display(),
            artifact_info.sha256
        );
        println!("exports:");
        for export in &manifest.plugin.exports {
            println!("  {} {}", export.name, export.signature);
        }
        println!("loaded: false (inspection never executes plugin code)");
    }
    0
}

fn report_plugin_error(error: &marqdo_plugin_abi::PluginError, json: bool) {
    if json {
        output::print_json(&json!({
            "command": "plugin.inspect",
            "success": false,
            "error": {
                "kind": error.kind,
                "code": error.code,
                "message": error.message,
            }
        }));
    } else {
        eprintln!("error[{}]: {}", error.code, error.message);
    }
}

/// `marqdo lock`.
pub fn lock(json: bool) -> i32 {
    let root = match discover_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let manifest = match marqdo_package::Manifest::load(&root.join(marqdo_package::MANIFEST_FILE)) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let graph =
        match marqdo_package::PackageResolver::new(root.clone(), Some(manifest.clone()), None)
            .with_fetch(true)
            .with_unlocked(true)
            .resolve_lock_graph()
        {
            Ok(graph) => graph,
            Err(error) => {
                eprintln!("error: {error}");
                return 1;
            }
        };
    let lockfile = graph.lock;
    let path = root.join(marqdo_package::LOCK_FILE);
    if let Err(error) = write_atomic(&path, lockfile.to_yaml().as_bytes()) {
        eprintln!("error: cannot write {}: {error}", path.display());
        return 1;
    }
    if json {
        output::print_json(&json!({
            "command": "lock",
            "success": true,
            "packages": lockfile.packages,
        }));
    } else {
        println!(
            "locked {} package(s) in {}",
            lockfile.packages.len(),
            path.display()
        );
    }
    0
}

/// `marqdo sync [--locked]`.
pub fn sync(locked: bool, json: bool) -> i32 {
    let root = match discover_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let manifest = match marqdo_package::Manifest::load(&root.join(marqdo_package::MANIFEST_FILE)) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let lock_path = root.join(marqdo_package::LOCK_FILE);
    if !lock_path.is_file() {
        eprintln!(
            "error: {} is missing; run `marqdo lock` first",
            lock_path.display()
        );
        return 1;
    }
    let lockfile = match marqdo_package::LockFile::load(&lock_path) {
        Ok(lockfile) => lockfile,
        Err(error) => {
            eprintln!("error: {error}");
            return 1;
        }
    };
    if let Err(error) = lockfile.validate_root(&manifest) {
        eprintln!("error: {error}");
        return 1;
    }
    let resolver =
        marqdo_package::PackageResolver::new(root, Some(manifest.clone()), Some(lockfile.clone()))
            .with_fetch(!locked);
    if let Err(error) = resolver.materialize_env(&lockfile) {
        eprintln!("error: cannot materialize project environment: {error}");
        return 1;
    }
    if json {
        output::print_json(&json!({
            "command": "sync",
            "success": true,
            "locked": locked,
            "packages": lockfile.packages,
        }));
    } else {
        println!(
            "synchronized {} package(s){} into {}",
            lockfile.packages.len(),
            if locked { " (locked)" } else { "" },
            resolver.env_root().display()
        );
    }
    0
}

fn discover_project_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    marqdo_package::discover_root(&cwd).ok_or_else(|| {
        format!(
            "no {} found in {} or any parent",
            marqdo_package::MANIFEST_FILE,
            cwd.display()
        )
    })
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "marqdo.lock".to_string());
    let temp = path.with_file_name(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::File::create(&temp)
            .map_err(|error| format!("cannot create {}: {error}", temp.display()))?;
        std::io::Write::write_all(&mut file, contents)
            .map_err(|error| format!("cannot write {}: {error}", temp.display()))?;
        file.sync_all()
            .map_err(|error| format!("cannot sync {}: {error}", temp.display()))?;
        std::fs::rename(&temp, path)
            .map_err(|error| format!("cannot publish {}: {error}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// `marqdo add <name> --path <dir> [--git URL --rev SHA]`.
pub fn add(
    name: &str,
    path: Option<&Path>,
    git: Option<&str>,
    rev: Option<&str>,
    json: bool,
) -> i32 {
    let root = match discover_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let manifest_path = root.join(marqdo_package::MANIFEST_FILE);
    let mut manifest = match marqdo_package::Manifest::load(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let dependency = marqdo_package::Dependency {
        path: path.map(|path| path.to_path_buf()),
        git: git.map(str::to_string),
        rev: rev.map(str::to_string),
    };
    if let Err(error) = dependency.validate(name) {
        eprintln!("error: {error}");
        return 2;
    }
    if manifest.dependencies.contains_key(name) {
        eprintln!("error: dependency `{name}` already exists; remove it first");
        return 1;
    }
    manifest.dependencies.insert(name.to_string(), dependency);
    if let Err(error) = std::fs::write(&manifest_path, manifest.to_yaml()) {
        eprintln!("error: cannot write {}: {error}", manifest_path.display());
        return 1;
    }
    if json {
        output::print_json(&json!({
            "command": "add",
            "success": true,
            "dependency": name,
            "manifest": manifest_path.display().to_string(),
        }));
    } else {
        println!("added dependency `{name}` to {}", manifest_path.display());
        println!(
            "run `marqdo lock` to pin it in {}",
            marqdo_package::LOCK_FILE
        );
    }
    0
}

/// `marqdo remove <name>`.
pub fn remove(name: &str, json: bool) -> i32 {
    let root = match discover_project_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let manifest_path = root.join(marqdo_package::MANIFEST_FILE);
    let mut manifest = match marqdo_package::Manifest::load(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    if manifest.dependencies.remove(name).is_none() {
        eprintln!("error: dependency `{name}` is not declared");
        return 1;
    }
    if let Err(error) = std::fs::write(&manifest_path, manifest.to_yaml()) {
        eprintln!("error: cannot write {}: {error}", manifest_path.display());
        return 1;
    }
    // The lock is a graph, not a per-name list: regenerating it here would
    // require resolving the new manifest. Leave it stale and tell the user.
    if json {
        output::print_json(&json!({
            "command": "remove",
            "success": true,
            "dependency": name,
        }));
    } else {
        println!("removed dependency `{name}`");
        println!("run `marqdo lock` to refresh {}", marqdo_package::LOCK_FILE);
    }
    0
}

/// Resolve `--arg` strings into typed arguments.
pub fn parse_named_args(raw: &[String]) -> Result<Vec<(String, Value)>, String> {
    raw.iter()
        .map(|entry| args::parse_named_arg(entry))
        .collect()
}
