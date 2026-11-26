//! End-to-end semantic conformance tests.
//!
//! Each `tests/conformance/semantics/*.mq.md` fixture is analysed by
//! [`marqdo_frontend::analyze_source`], assembled into an
//! [`marqdo_ir::Program`] and executed through [`marqdo_runtime::run`] with a
//! [`marqdo_host::StdHost`] in `project` mode. The typed wire representation
//! ([`marqdo_value::to_wire`]) of the returned value and the captured program
//! stdout are compared with the sibling `*.expected.json`:
//!
//! ```json
//! {"function": "<name>", "args": {"name": <wire value>},
//!  "value": <wire value>, "stdout": "..."}
//! ```
//!
//! Fixtures that need an explicit grant declare it as
//! `"grants": ["foreign"]`; they are skipped when the kernel is unavailable.
//!
//! Developer note: `UPDATE_SEMANTICS_GOLDENS=1` rewrites `value`/`stdout` from
//! the implementation. Never use it without hand-checking every rewritten
//! expectation; the checked-in goldens were reviewed individually.
#[path = "support/stdlib.rs"]
pub mod stdlib_support;

use std::fs;

use std::path::{Path, PathBuf};

use marqdo_host::{Capabilities, StdHost};
use marqdo_host_contract::Host;
use marqdo_ir::{ModuleId, Program, ResolvedImport};
use marqdo_runtime::{run, EvaluatedArg};
use marqdo_sandbox::SandboxMode;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/cli has a parent")
        .parent()
        .expect("crates has a parent")
        .to_path_buf()
}

fn semantics_dir() -> PathBuf {
    repo_root()
        .join("tests")
        .join("conformance")
        .join("semantics")
}

fn fixture_paths() -> Vec<PathBuf> {
    let dir = semantics_dir();
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    paths.sort();
    paths
}

fn expected_path(fixture: &Path) -> PathBuf {
    let name = fixture
        .file_name()
        .expect("fixture has a file name")
        .to_string_lossy();
    let stem = name
        .strip_suffix(".mq.md")
        .unwrap_or_else(|| panic!("fixture `{}` must end in .mq.md", fixture.display()));
    fixture.with_file_name(format!("{stem}.expected.json"))
}

fn python_available() -> bool {
    // Ask the same platform-aware resolver the runtime uses. On Windows a
    // PATH entry is `python3.exe`, not the extension-less `python3` token, so a
    // raw `Command::new("python3")` probe can disagree with execution.
    marqdo_foreign::detect("python").is_some()
}

/// Unique scratch directory for the host project root. Program fixtures are
/// pure, so nothing is read from or written to it; it only satisfies the host
/// constructor contract.
fn scratch_project_root() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "marqdo-conformance-semantics-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap_or_else(|error| {
        panic!(
            "cannot create scratch project root {}: {error}",
            dir.display()
        )
    });
    dir
}

fn render_diagnostics(result: &marqdo_frontend::AnalysisResult) -> String {
    let mut out = String::new();
    for diagnostic in &result.diagnostics {
        out.push_str(&format!(
            "  {}[{}]: {}\n",
            diagnostic.severity, diagnostic.code, diagnostic.message
        ));
    }
    out
}

#[test]
fn semantic_goldens_match_end_to_end_execution() {
    let fixtures = fixture_paths();
    assert!(
        fixtures.len() >= 25,
        "the P0 corpus must keep at least 25 semantic fixtures, found {}",
        fixtures.len()
    );
    let update = std::env::var_os("UPDATE_SEMANTICS_GOLDENS").is_some_and(|value| value == "1");
    let has_python = python_available();
    let project_root = scratch_project_root();
    let mut skipped = 0usize;
    let mut updated = 0usize;

    for fixture in &fixtures {
        let expected_path = expected_path(fixture);
        let expected_text = fs::read_to_string(&expected_path).unwrap_or_else(|error| {
            panic!(
                "missing semantic golden {}: {error}",
                expected_path.display()
            )
        });
        let mut expected: serde_json::Value = serde_json::from_str(&expected_text)
            .unwrap_or_else(|error| panic!("invalid JSON in {}: {error}", expected_path.display()));
        let object = expected
            .as_object()
            .unwrap_or_else(|| panic!("{} must be a JSON object", expected_path.display()));

        let function = object
            .get("function")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{} has no `function`", expected_path.display()))
            .to_string();
        let grant_foreign = object
            .get("grants")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|grants| grants.iter().any(|grant| grant == "foreign"));
        if grant_foreign && !has_python {
            skipped += 1;
            println!(
                "skipping {}: python3 is unavailable (foreign kernel fixture)",
                fixture.display()
            );
            continue;
        }

        let source = fs::read_to_string(fixture)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", fixture.display()));
        let module_id = ModuleId::from("index.mq.md");
        let analysis = marqdo_frontend::analyze_source(&module_id.to_string(), &source);
        if analysis.has_errors() {
            panic!(
                "semantic fixture `{}` must compile without errors:\n{}",
                fixture.display(),
                render_diagnostics(&analysis)
            );
        }

        let mut program = marqdo_ir::Program::default();
        program.insert(analysis.module.clone());
        program.main_module = Some(module_id.clone());
        install_entry_stdlib_imports(&mut program, &module_id, &analysis);
        bind_calls_with_stdlib_aliases(&mut program);

        let mut capabilities = Capabilities::pure();
        if grant_foreign {
            capabilities = capabilities.allow_foreign();
        }
        let mut host = StdHost::new(SandboxMode::Project, capabilities, project_root.clone());

        let args_value = object
            .get("args")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let args_object = args_value
            .as_object()
            .unwrap_or_else(|| panic!("`args` of {} must be an object", expected_path.display()));
        let mut runtime_args = Vec::new();
        // BTreeMap-backed JSON objects iterate in sorted key order; argument
        // evaluation order is irrelevant here because arguments are constants.
        for (name, wire) in args_object {
            let value = marqdo_value::from_wire(wire).unwrap_or_else(|error| {
                panic!(
                    "invalid wire argument `{name}` in {}: {error}",
                    expected_path.display()
                )
            });
            runtime_args.push(EvaluatedArg::named(
                name.clone(),
                value,
                marqdo_span::Span::default(),
            ));
        }

        let outcome =
            run(&program, &mut host, &module_id, &function, runtime_args).unwrap_or_else(|error| {
                panic!(
                    "semantic fixture `{}` failed with {}: {}\nframes: {:?}",
                    fixture.display(),
                    error.code(),
                    error.message,
                    error.frames
                )
            });
        let stdout = String::from_utf8_lossy(&host.take_output()).to_string();
        let value = marqdo_value::to_wire(&outcome.value);

        if update {
            let mut object = expected.as_object().cloned().expect("object");
            object.insert("value".into(), value);
            object.insert("stdout".into(), serde_json::Value::String(stdout));
            expected = serde_json::Value::Object(object);
            let mut text = serde_json::to_string_pretty(&expected).expect("serializable");
            text.push('\n');
            fs::write(&expected_path, text).unwrap_or_else(|error| {
                panic!("cannot write {}: {error}", expected_path.display())
            });
            updated += 1;
            continue;
        }

        let expected_value = object.get("value").unwrap_or_else(|| {
            panic!(
                "{} has no `value`; regenerate it with UPDATE_SEMANTICS_GOLDENS=1 \
                 after reviewing the behaviour",
                expected_path.display()
            )
        });
        let expected_stdout = object
            .get("stdout")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{} has no `stdout`", expected_path.display()));
        assert_eq!(
            &value,
            expected_value,
            "value mismatch for `{}`\n  expected: {}\n  actual:   {}",
            fixture.display(),
            expected_value,
            value
        );
        assert_eq!(
            stdout,
            expected_stdout,
            "stdout mismatch for `{}`",
            fixture.display()
        );
    }

    if update {
        println!("UPDATE_SEMANTICS_GOLDENS=1: rewrote {updated} semantic golden(s)");
        return;
    }
    let _ = fs::remove_dir_all(&project_root);
    println!(
        "semantic conformance: {} fixture(s) checked, {skipped} skipped",
        fixtures.len()
    );
}

fn normalise_relative(path: &Path) -> String {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_os_string()),
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {}
        }
    }
    parts
        .iter()
        .map(|part| part.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Bind direct-runtime fixtures exactly like the CLI composition root.
///
/// The semantic harness builds a `Program` directly, so it mirrors the
/// production path: load the official stdlib public-alias targets, build the
/// unique registry, run the single binding pass and install its table. There is
/// no `print_override` and no runtime name guessing.
fn bind_calls_with_stdlib_aliases(program: &mut Program) {
    let root = stdlib_support::bundle_dir();
    let mut loaded = std::collections::BTreeSet::new();
    let mut official = Vec::new();
    if root.join("stdlib.json").is_file() {
        let text = fs::read_to_string(root.join("stdlib.json"))
            .unwrap_or_else(|error| panic!("cannot read stdlib.json: {error}"));
        let manifest: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("invalid stdlib.json: {error}"));
        if let Some(aliases) = manifest
            .get("public_aliases")
            .and_then(serde_json::Value::as_array)
        {
            for entry in aliases {
                let public_names: Vec<String> = entry
                    .get("public_names")
                    .and_then(serde_json::Value::as_array)
                    .unwrap_or_else(|| panic!("public alias has no public_names"))
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(ToString::to_string)
                    .collect();
                let target_module = entry
                    .get("target_module")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("public alias has no target_module"));
                let target_function = entry
                    .get("target_function")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("public alias has no target_function"));
                let module_id = load_stdlib_module(&root, target_module, program, &mut loaded)
                    .unwrap_or_else(|error| panic!("cannot load stdlib alias module: {error}"));
                let function = program
                    .module(&module_id)
                    .and_then(|module| module.function(target_function))
                    .unwrap_or_else(|| {
                        panic!("stdlib alias target `{target_function}` is missing")
                    });
                let effects = entry
                    .get("effects")
                    .and_then(serde_json::Value::as_array)
                    .map(|effects| {
                        effects
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(ToString::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                official.push(marqdo_ir::callable::OfficialAlias {
                    public_names,
                    function: function.id.clone(),
                    signature: marqdo_ir::callable::CallableSignature {
                        params: function.params.clone(),
                        kind: function.signature,
                    },
                    owner_package: module_id.instance.clone(),
                    effects_requested: effects,
                });
            }
        }
    }
    let mut registry = program.callable_registry();
    for alias in official {
        registry
            .insert_official_alias(alias)
            .unwrap_or_else(|error| panic!("cannot register stdlib alias: {error}"));
    }
    let bound = marqdo_frontend::calls::bind_program_calls(program, &registry);
    if !bound.diagnostics.is_empty() {
        panic!(
            "fixture failed static call binding: {:#?}",
            bound.diagnostics
        );
    }
    program.set_call_bindings(bound.bindings);
}

fn load_stdlib_module(
    root: &Path,
    relative: &str,
    program: &mut Program,
    loaded: &mut std::collections::BTreeSet<String>,
) -> Result<ModuleId, String> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let relative = normalise_relative(Path::new(relative));
    if loaded.contains(&relative) {
        if let Some((id, _)) = program
            .modules
            .iter()
            .find(|(id, _)| id.relative_path == relative)
        {
            return Ok(id.clone());
        }
        return Err(format!("stdlib import cycle at `{relative}`"));
    }
    loaded.insert(relative.clone());
    let path = root.join(&relative);
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    let module_id = ModuleId::new("std:test", relative.clone());
    let analysis = marqdo_frontend::analyze_source(&module_id.to_string(), &source);
    if analysis.has_errors() {
        return Err(format!("stdlib module `{relative}` has diagnostics"));
    }
    let imports: Vec<(String, String)> = analysis
        .document
        .frontmatter()
        .and_then(|frontmatter| frontmatter.imports().ok())
        .map(|mapping| mapping.into_iter().collect())
        .unwrap_or_default();
    for (alias, target) in imports {
        let target = target.strip_prefix("std:").unwrap_or(&target);
        let child = if target.starts_with("./") || target.starts_with("../") {
            let base = Path::new(&relative)
                .parent()
                .unwrap_or_else(|| Path::new(""));
            normalise_relative(&base.join(target))
        } else {
            target.to_string()
        };
        let to = load_stdlib_module(&root, &child, program, loaded)?;
        program.resolved_imports.push(ResolvedImport {
            from: module_id.clone(),
            alias,
            to,
        });
    }
    program.insert(analysis.module);
    Ok(module_id)
}

/// Resolve `std:` imports declared by a direct-runtime fixture entry.
fn install_entry_stdlib_imports(
    program: &mut Program,
    module_id: &ModuleId,
    analysis: &marqdo_frontend::AnalysisResult,
) {
    let root = stdlib_support::bundle_dir();
    let Some(imports) = analysis
        .document
        .frontmatter()
        .and_then(|frontmatter| frontmatter.imports().ok())
    else {
        return;
    };
    let mut loaded = std::collections::BTreeSet::new();
    for (alias, target) in imports {
        let Some(relative) = target.strip_prefix("std:") else {
            continue;
        };
        let relative = if relative.ends_with(".mq.md") {
            relative.to_string()
        } else {
            format!("{relative}.mq.md")
        };
        let to = match load_stdlib_module(&root, &relative, program, &mut loaded) {
            Ok(id) => id,
            Err(error) => panic!("cannot load stdlib import `{target}`: {error}"),
        };
        program.resolved_imports.push(ResolvedImport {
            from: module_id.clone(),
            alias,
            to,
        });
    }
}
