//! Project and module loading for the CLI.
//!
//! Loading is static: files are parsed and analysed, never executed. Imports
//! come only from explicit YAML front matter and are resolved exactly as
//! declared (`@dependency` or a relative `.mq.md` path); nothing is searched
//! implicitly.
//!
//! Module identity is package-aware (ADR-0008): a module is
//! `PackageInstanceId::relative_path`, never a physical cache path. The
//! package layer resolves path/Git sources; this loader consumes
//! `ResolvedPackage` and does not branch on the source kind.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use marqdo_expression::Expr;
use marqdo_frontend::{analyze_source, AnalysisResult};
use marqdo_ir::{ModuleId, PlaceTarget, Program, ResolvedImport, Stmt};
use marqdo_package::{
    discover_root, LockFile, Manifest, PackageResolver, ResolvedPackage, LOCK_FILE, MANIFEST_FILE,
};
use marqdo_value::{codes, Diagnostic};

use crate::stdlib::StdlibBundle;

/// One analysed source file.
pub struct LoadedFile {
    /// Canonical module identity (package instance + package-relative path).
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

/// Logical project identity used by public Analysis/Trace artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectIdentity {
    /// A project with `marqdo.yaml`: the logical root package instance.
    Manifest {
        /// `root:<name>@<version>`.
        instance: String,
    },
    /// No manifest: a content-derived snapshot id with no rename guarantee.
    WorkspaceSnapshot {
        /// Lowercase hex digest of the sorted module/source pairs.
        digest: String,
    },
}

impl ProjectIdentity {
    /// Canonical `root_id` string used in Analysis/Trace.
    pub fn root_id(&self) -> String {
        match self {
            ProjectIdentity::Manifest { instance } => instance.clone(),
            ProjectIdentity::WorkspaceSnapshot { digest } => format!("sha256:{digest}"),
        }
    }

    /// Wire discriminator (`manifest` or `workspace_snapshot`).
    pub fn kind(&self) -> &'static str {
        match self {
            ProjectIdentity::Manifest { .. } => "manifest",
            ProjectIdentity::WorkspaceSnapshot { .. } => "workspace_snapshot",
        }
    }
}

fn workspace_snapshot_digest(files: &BTreeMap<ModuleId, LoadedFile>) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for (module_id, file) in files {
        hasher.update(module_id.canonical().as_bytes());
        hasher.update(b"\n");
        hasher.update(source_digest(&file.source).as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

fn source_digest(source: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    hex::encode(hasher.finalize())
}

/// A resolved source location inside a loaded file.
pub struct LocatedSource<'a> {
    /// The owning file.
    pub file: &'a LoadedFile,
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub column: usize,
}

/// Identity of the verified bundled stdlib used by a load, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdlibInfo {
    /// Declared version.
    pub version: String,
    /// Verified content digest.
    pub digest: String,
    /// Package instance id.
    pub instance_id: String,
}

/// A fully loaded project ready for checking or running.
pub struct LoadedProject {
    /// Project root.
    pub root: PathBuf,
    /// Entry module.
    pub entry: ModuleId,
    /// Logical project identity (path-independent).
    pub identity: ProjectIdentity,
    /// All modules.
    pub program: Program,
    /// Analysed files keyed by module id.
    pub files: BTreeMap<ModuleId, LoadedFile>,
    /// Package root per logical instance.
    pub packages: BTreeMap<String, PathBuf>,
    /// All diagnostics in deterministic order.
    pub diagnostics: Vec<FileDiagnostic>,
    /// Verified bundled stdlib identity, when one was available.
    pub stdlib: Option<StdlibInfo>,
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

    /// Resolve a logical `(module_id, span)` location to a loaded file.
    ///
    /// Returns `None` for an unknown module or an out-of-range span; callers
    /// must not guess or fall back to the entry file.
    pub fn locate(&self, module_id: &str, span: marqdo_span::Span) -> Option<LocatedSource<'_>> {
        let module = ModuleId::parse(module_id).ok()?;
        let file = self.files.get(&module)?;
        if span.start > span.end || span.end > file.source.len() {
            return None;
        }
        let position = marqdo_span::LineIndex::new(&file.source).line_col(span.start);
        Some(LocatedSource {
            file,
            line: position.line,
            column: position.column,
        })
    }

    /// Resolve a physical path to its loaded or statically addressable module.
    pub fn module_id_for_path(&self, path: &Path) -> Option<ModuleId> {
        let canonical = path.canonicalize().ok()?;
        let mut best: Option<(&String, &PathBuf)> = None;
        for (instance, root) in &self.packages {
            if canonical.starts_with(root)
                && best
                    .as_ref()
                    .map(|(_, current)| root.components().count() > current.components().count())
                    .unwrap_or(true)
            {
                best = Some((instance, root));
            }
        }
        let (instance, root) = best?;
        let relative = canonical.strip_prefix(root).ok()?;
        let mut parts = Vec::new();
        for component in relative.components() {
            parts.push(component.as_os_str().to_string_lossy().to_string());
        }
        Some(ModuleId::new(instance.clone(), parts.join("/")))
    }
}

struct Loader {
    resolver: PackageResolver,
    packages: BTreeMap<String, PathBuf>,
    files: BTreeMap<ModuleId, LoadedFile>,
    program: Program,
    diagnostics: Vec<FileDiagnostic>,
    stack: Vec<PathBuf>,
    stdlib: Option<StdlibBundle>,
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
    let lock = if root.join(LOCK_FILE).is_file() {
        let lock = LockFile::load(&root.join(LOCK_FILE))
            .map_err(|error| format!("{LOCK_FILE}: {error}"))?;
        if let Some(manifest) = &manifest {
            lock.validate_root(manifest)
                .map_err(|error| format!("{LOCK_FILE}: {error}"))?;
        }
        Some(lock)
    } else {
        if manifest
            .as_ref()
            .map(|manifest| !manifest.dependencies.is_empty())
            .unwrap_or(false)
        {
            return Err(
                "marqdo.lock is missing; run `marqdo lock` before checking or running this project"
                    .to_string(),
            );
        }
        None
    };
    let resolver = PackageResolver::new(root.clone(), manifest.clone(), lock);
    let entry_module = resolver.resolve_entry(&entry)?;
    let stdlib = StdlibBundle::discover().map_err(|error| format!("stdlib: {error}"))?;
    let mut loader = Loader {
        resolver,
        packages: BTreeMap::new(),
        files: BTreeMap::new(),
        program: Program::default(),
        diagnostics: Vec::new(),
        stack: Vec::new(),
        stdlib,
    };
    let entry_id = loader.load_module(&entry_module.package, &entry_module.relative_path)?;
    loader.program.main_module = Some(entry_id.clone());

    // Compat-B (ADR-0030): only programs that actually call 打印/print need the
    // stdlib print module; injecting it for every program would change the
    // module graph, analysis output and startup cost for no benefit.
    if program_uses_print(&loader.program) {
        if let Some(stdlib) = loader.stdlib.clone() {
            let package = stdlib.package();
            let io = loader.load_module(&package, "io.mq.md")?;
            let Some(module) = loader.program.module(&io) else {
                return Err("stdlib print module was not loaded".to_string());
            };
            let Some(function) = module.function("写值") else {
                return Err("stdlib print module has no `写值` function".to_string());
            };
            loader.program.print_override = Some(function.id.clone());
        }
    }
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
    let stdlib_info = loader.stdlib.as_ref().map(|bundle| StdlibInfo {
        version: bundle.version().to_string(),
        digest: bundle.digest().to_string(),
        instance_id: bundle.instance_id(),
    });
    let identity = match &manifest {
        Some(_) => ProjectIdentity::Manifest {
            instance: entry_id.instance.clone(),
        },
        None => ProjectIdentity::WorkspaceSnapshot {
            digest: workspace_snapshot_digest(&loader.files),
        },
    };
    Ok(LoadedProject {
        root: loader.resolver.root().to_path_buf(),
        entry: entry_id,
        identity,
        program: loader.program,
        files: loader.files,
        packages: loader.packages,
        diagnostics: loader.diagnostics,
        stdlib: stdlib_info,
    })
}

impl Loader {
    fn load_module(
        &mut self,
        package: &ResolvedPackage,
        relative_path: &str,
    ) -> Result<ModuleId, String> {
        let package_root = package
            .module_root
            .canonicalize()
            .unwrap_or_else(|_| package.module_root.clone());
        let path = package_root
            .join(relative_path)
            .canonicalize()
            .map_err(|error| format!("cannot open `{relative_path}`: {error}"))?;
        if !path.starts_with(&package_root) {
            return Err(format!(
                "module `{relative_path}` escapes package `{}`",
                package.instance_id
            ));
        }
        let relative = path.strip_prefix(&package_root).map_err(|_| {
            format!(
                "module `{}` is outside package `{}`",
                path.display(),
                package.instance_id
            )
        })?;
        let mut parts = Vec::new();
        for component in relative.components() {
            parts.push(component.as_os_str().to_string_lossy().to_string());
        }
        let module_id = ModuleId::new(&package.instance_id, parts.join("/"));
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
        self.packages
            .entry(package.instance_id.clone())
            .or_insert_with(|| package_root.clone());
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        let analysis = analyze_source(&module_id.to_string(), &source);
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
            let resolved = if let Some(std_target) = target.strip_prefix("std:") {
                match &self.stdlib {
                    Some(stdlib) => stdlib.resolve(std_target),
                    None => Err(format!(
                        "no bundled stdlib is available for `{target}`; install the official \
                         CLI bundle or set {STDLIB_DIR_ENV}",
                        STDLIB_DIR_ENV = crate::stdlib::STDLIB_DIR_ENV
                    )),
                }
            } else if let Some(alias_name) = target.strip_prefix('@') {
                self.resolver.resolve_alias_from(package, alias_name)
            } else {
                self.resolver.resolve_relative(package, &path, &target)
            };
            match resolved {
                Ok(resolved) => {
                    let target_path = match resolved
                        .package
                        .module_root
                        .join(&resolved.relative_path)
                        .canonicalize()
                    {
                        Ok(path) => path,
                        Err(error) => {
                            self.diagnostics.push(FileDiagnostic {
                                module_id: module_id.clone(),
                                path: path.clone(),
                                source: source.clone(),
                                diagnostic: Diagnostic::error(
                                    codes::MODULE_UNRESOLVED,
                                    Some(span),
                                    format!("cannot open imported module `{target}`: {error}"),
                                ),
                            });
                            continue;
                        }
                    };
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
                    match self.load_module(&resolved.package, &resolved.relative_path) {
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
                Err(error) => {
                    self.diagnostics.push(FileDiagnostic {
                        module_id: module_id.clone(),
                        path: path.clone(),
                        source: source.clone(),
                        diagnostic: Diagnostic::error(
                            codes::MODULE_UNRESOLVED,
                            Some(span),
                            format!("cannot resolve imported module `{target}`: {error}"),
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
        if let Some(file) = self.files.get(&module_id) {
            self.program
                .modules
                .insert(module_id.clone(), file.analysis.module.clone());
        }
        Ok(module_id)
    }
}

/// Whether the loaded program calls the legacy `打印`/`print` builtin.
fn program_uses_print(program: &Program) -> bool {
    program.modules.values().any(|module| {
        module
            .functions
            .iter()
            .any(|function| stmts_use_print(&function.body))
    })
}

fn stmts_use_print(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Stmt::Assign { target, value, .. } => {
            let target_uses = match target {
                PlaceTarget::Local { .. } => false,
                PlaceTarget::Index { base, index, .. } => {
                    expr_uses_print(base) || expr_uses_print(index)
                }
            };
            target_uses || expr_uses_print(value)
        }
        Stmt::Eval { expr, .. } | Stmt::Return { value: expr, .. } => expr_uses_print(expr),
        Stmt::Call { call, .. } => call.args.iter().any(|arg| expr_uses_print(&arg.value)),
        Stmt::Branch { arms, .. } => arms
            .iter()
            .any(|arm| expr_uses_print(&arm.condition) || stmts_use_print(&arm.body)),
        Stmt::Loop {
            condition, body, ..
        } => expr_uses_print(condition) || stmts_use_print(body),
    })
}

fn expr_uses_print(expr: &Expr) -> bool {
    match expr {
        Expr::Call { callee, args, .. } => {
            matches!(callee.as_ref(), Expr::Ident(ident) if matches!(ident.name.as_str(), "打印" | "print"))
                || expr_uses_print(callee)
                || args.iter().any(|arg| expr_uses_print(&arg.value))
        }
        Expr::List(items) => items.iter().any(expr_uses_print),
        Expr::Record(entries) => entries.iter().any(|entry| expr_uses_print(&entry.value)),
        Expr::Unary { operand, .. } => expr_uses_print(operand),
        Expr::Binary { left, right, .. } => expr_uses_print(left) || expr_uses_print(right),
        Expr::Member { base, .. } => expr_uses_print(base),
        Expr::Index { base, index, .. } => expr_uses_print(base) || expr_uses_print(index),
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Text(_)
        | Expr::Bool(_)
        | Expr::None
        | Expr::Ident(_) => false,
    }
}
