//! Project and module loading for the CLI.
//!
//! Loading is static: files are parsed and analysed, never executed. Imports
//! come only from explicit YAML front matter and are resolved exactly as
//! declared (`@dependency` or a relative `.mq.md` path); nothing is searched
//! implicitly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use marqdo_frontend::{analyze_source, AnalysisResult};
use marqdo_ir::{ModuleId, Program, ResolvedImport};
use marqdo_package::{discover_root, Manifest, MANIFEST_FILE};
use marqdo_value::{codes, Diagnostic};

/// One analysed source file.
pub struct LoadedFile {
    /// Canonical module identity (project-relative path).
    pub module_id: ModuleId,
    /// Absolute path on disk.
    pub path: PathBuf,
    /// Source text.
    pub source: String,
    /// Frontend analysis.
    pub analysis: AnalysisResult,
}

/// A diagnostic tied to its file for rendering.
#[derive(Debug, Clone)]
pub struct FileDiagnostic {
    /// Module that produced it.
    pub module_id: ModuleId,
    /// Absolute file path.
    pub path: PathBuf,
    /// Source text (needed for line/column rendering).
    pub source: String,
    /// The structured diagnostic.
    pub diagnostic: Diagnostic,
}

impl FileDiagnostic {
    /// 1-based line/column of the diagnostic span.
    pub fn line_col(&self) -> Option<(usize, usize)> {
        let span = self.diagnostic.span?;
        let index = marqdo_span::LineIndex::new(&self.source);
        let position = index.line_col(span.start);
        Some((position.line, position.column))
    }

    /// Source text of the diagnostic's line.
    pub fn line_text(&self) -> Option<&str> {
        let span = self.diagnostic.span?;
        let index = marqdo_span::LineIndex::new(&self.source);
        let position = index.line_col(span.start);
        self.source.lines().nth(position.line.saturating_sub(1))
    }
}

/// A fully loaded project ready for checking or running.
pub struct LoadedProject {
    /// Project root.
    pub root: PathBuf,
    /// Entry module.
    pub entry: ModuleId,
    /// All modules.
    pub program: Program,
    /// Analysed files keyed by module id.
    pub files: BTreeMap<ModuleId, LoadedFile>,
    /// All diagnostics in deterministic order.
    pub diagnostics: Vec<FileDiagnostic>,
}

impl LoadedProject {
    /// Whether any diagnostic is an error.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diag| diag.diagnostic.is_error())
    }

    /// The entry file.
    pub fn entry_file(&self) -> &LoadedFile {
        &self.files[&self.entry]
    }
}

struct Loader {
    root: PathBuf,
    manifest: Option<Manifest>,
    files: BTreeMap<ModuleId, LoadedFile>,
    program: Program,
    diagnostics: Vec<FileDiagnostic>,
    stack: Vec<PathBuf>,
}

/// Load the entry file and its explicitly imported modules.
pub fn load(entry: &Path) -> Result<LoadedProject, String> {
    let entry = entry
        .canonicalize()
        .map_err(|error| format!("cannot open `{}`: {error}", entry.display()))?;
    if !entry.is_file() {
        return Err(format!("`{}` is not a file", entry.display()));
    }
    let root = match discover_root(&entry) {
        Some(root) => root.canonicalize().unwrap_or(root),
        None => entry
            .parent()
            .map(|parent| parent.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    let manifest = if root.join(MANIFEST_FILE).is_file() {
        Some(Manifest::load(&root.join(MANIFEST_FILE))?)
    } else {
        None
    };
    let mut loader = Loader {
        root,
        manifest,
        files: BTreeMap::new(),
        program: Program::default(),
        diagnostics: Vec::new(),
        stack: Vec::new(),
    };
    let entry_id = loader.load_module(&entry)?;
    loader.program.main_module = Some(entry_id.clone());
    for (module_id, diagnostic) in marqdo_frontend::calls::validate_program(&loader.program) {
        if let Some(file) = loader.files.get(&module_id) {
            loader.diagnostics.push(FileDiagnostic {
                module_id,
                path: file.path.clone(),
                source: file.source.clone(),
                diagnostic,
            });
        }
    }
    Ok(LoadedProject {
        root: loader.root,
        entry: entry_id,
        program: loader.program,
        files: loader.files,
        diagnostics: loader.diagnostics,
    })
}

impl Loader {
    fn module_id_for(&self, path: &Path) -> Option<ModuleId> {
        let relative = path.strip_prefix(&self.root).ok()?;
        let mut parts = Vec::new();
        for component in relative.components() {
            parts.push(component.as_os_str().to_string_lossy().to_string());
        }
        Some(ModuleId(parts.join("/")))
    }

    fn load_module(&mut self, path: &Path) -> Result<ModuleId, String> {
        let path = path
            .canonicalize()
            .map_err(|error| format!("cannot open `{}`: {error}", path.display()))?;
        let module_id = self
            .module_id_for(&path)
            .ok_or_else(|| format!("`{}` is outside the project root", path.display()))?;
        if self.files.contains_key(&module_id) {
            return Ok(module_id);
        }
        if self.stack.contains(&path) {
            return Err(format!(
                "module import cycle detected at `{}`",
                path.display()
            ));
        }
        self.stack.push(path.clone());
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        let analysis = analyze_source(&module_id.0, &source);
        let imports: Vec<(String, String, marqdo_span::Span)> = analysis
            .document
            .frontmatter()
            .and_then(|frontmatter| frontmatter.imports().ok())
            .map(|mapping| {
                mapping
                    .into_iter()
                    .map(|(alias, target)| {
                        (
                            alias,
                            target,
                            analysis
                                .document
                                .frontmatter()
                                .map(|f| f.span)
                                .unwrap_or_default(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        for (alias, target, span) in imports {
            match self.resolve_import(&path, &target) {
                Some(target_path) => {
                    if self.stack.contains(&target_path) {
                        self.diagnostics.push(FileDiagnostic {
                            module_id: module_id.clone(),
                            path: path.clone(),
                            source: source.clone(),
                            diagnostic: Diagnostic::error(
                                codes::MODULE_CYCLE,
                                Some(span),
                                format!("import `{alias}` -> `{target}` creates an import cycle"),
                            )
                            .with_hint("remove the cycle; modules register definitions only"),
                        });
                        continue;
                    }
                    match self.load_module(&target_path) {
                        Ok(target_id) => {
                            self.program.resolved_imports.push(ResolvedImport {
                                from: module_id.clone(),
                                alias,
                                to: target_id,
                            });
                        }
                        Err(error) => self.diagnostics.push(FileDiagnostic {
                            module_id: module_id.clone(),
                            path: path.clone(),
                            source: source.clone(),
                            diagnostic: Diagnostic::error(
                                codes::MODULE_UNRESOLVED,
                                Some(span),
                                error,
                            ),
                        }),
                    }
                }
                None => {
                    self.diagnostics.push(FileDiagnostic {
                        module_id: module_id.clone(),
                        path: path.clone(),
                        source: source.clone(),
                        diagnostic: Diagnostic::error(
                            codes::MODULE_UNRESOLVED,
                            Some(span),
                            format!("cannot resolve imported module `{target}`"),
                        )
                        .with_hint(
                            "use `@dependency` for marqdo.yaml dependencies or `./file.mq.md` for project files",
                        ),
                    });
                }
            }
        }

        for diagnostic in &analysis.diagnostics {
            self.diagnostics.push(FileDiagnostic {
                module_id: module_id.clone(),
                path: path.clone(),
                source: source.clone(),
                diagnostic: diagnostic.clone(),
            });
        }
        self.stack.pop();
        self.files.insert(
            module_id.clone(),
            LoadedFile {
                module_id: module_id.clone(),
                path,
                source,
                analysis,
            },
        );
        // Move the analysed module into the program.
        if let Some(file) = self.files.get(&module_id) {
            self.program
                .modules
                .insert(module_id.clone(), file.analysis.module.clone());
        }
        Ok(module_id)
    }

    fn resolve_import(&self, importing: &Path, target: &str) -> Option<PathBuf> {
        if let Some(alias) = target.strip_prefix('@') {
            let manifest = self.manifest.as_ref()?;
            let dependency = manifest.dependencies.get(alias)?;
            let path = dependency.path.as_ref()?;
            let dependency_path = self.root.join(path);
            let candidate = if dependency_path.is_dir() {
                dependency_path.join("index.mq.md")
            } else {
                dependency_path
            };
            return candidate.canonicalize().ok();
        }
        let base = importing.parent()?;
        let candidate = base.join(target);
        candidate.canonicalize().ok()
    }
}
