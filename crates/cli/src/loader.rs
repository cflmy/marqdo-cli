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
use marqdo_frontend::AnalysisResult;
use marqdo_ir::callable::OfficialAlias;
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

/// `sha256:<hex>` of exact bytes, the wire form used by Analysis snapshots.
fn bytes_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
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
    /// Official stdlib public aliases referenced by this program (ADR-0058).
    ///
    /// These are ordinary `Marqdo` bindings; the runtime has no print special
    /// case and no `Program.print_override`.
    pub official_aliases: Vec<marqdo_ir::callable::OfficialAlias>,
    /// Project manifest, when the entry belongs to a project.
    pub manifest: Option<Manifest>,
    /// Verified lock file, when present (lock v4).
    pub lock: Option<LockFile>,
    /// SHA-256 of the exact `marqdo.lock` bytes captured while loading, or an
    /// empty string when the project has no lock. Analysis must never re-read
    /// the file: a later edit cannot change the snapshot the program used.
    pub lock_digest: String,
    /// SHA-256 of the exact `marqdo.yaml` bytes captured while loading, or an
    /// empty string when there is no manifest.
    pub manifest_digest: String,
    /// Resolved `marqdo.yaml.extensions` activation sources (static paths only).
    pub extension_sources: Vec<marqdo_package::ResolvedExtension>,
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
    // Read each snapshot input exactly once. The digest recorded in Analysis
    // is computed from these bytes, not from a second read after preparation.
    let manifest_path = root.join(MANIFEST_FILE);
    let (manifest, manifest_digest) = if manifest_path.is_file() {
        let bytes = std::fs::read(&manifest_path)
            .map_err(|error| format!("cannot read {MANIFEST_FILE}: {error}"))?;
        let yaml = String::from_utf8(bytes.clone())
            .map_err(|error| format!("{MANIFEST_FILE} is not valid UTF-8: {error}"))?;
        let manifest =
            Manifest::from_yaml(&yaml).map_err(|error| format!("{MANIFEST_FILE}: {error}"))?;
        (Some(manifest), bytes_digest(&bytes))
    } else {
        (None, String::new())
    };
    let lock_path = root.join(LOCK_FILE);
    let (lock, lock_digest) = if lock_path.is_file() {
        let bytes = std::fs::read(&lock_path)
            .map_err(|error| format!("cannot read {LOCK_FILE}: {error}"))?;
        let yaml = String::from_utf8(bytes.clone())
            .map_err(|error| format!("{LOCK_FILE} is not valid UTF-8: {error}"))?;
        let lock = LockFile::from_yaml(&yaml).map_err(|error| format!("{LOCK_FILE}: {error}"))?;
        if let Some(manifest) = &manifest {
            lock.validate_root(manifest)
                .map_err(|error| format!("{LOCK_FILE}: {error}"))?;
        }
        (Some(lock), bytes_digest(&bytes))
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
        (None, String::new())
    };
    let resolver = PackageResolver::new(root.clone(), manifest.clone(), lock.clone());
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

    // Official stdlib public aliases (ADR-0058): the verified metadata names
    // the target module/function; only aliases the program actually references
    // load their module, and the runtime binds them as ordinary Marqdo calls.
    let official_aliases = loader.load_official_aliases()?;
    let extension_sources = loader
        .resolver
        .resolve_extension_sources()
        .map_err(|error| format!("extension selection: {error}"))?;

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
        official_aliases,
        manifest,
        lock,
        lock_digest,
        manifest_digest,
        extension_sources,
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
        let analysis = marqdo_frontend::analyze_source_for_program(&module_id.to_string(), &source);
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

    /// Resolve the official stdlib public aliases referenced by the program.
    ///
    /// Only the verified `stdlib.json` metadata can declare an official alias;
    /// a third-party package cannot claim `打印`/`print` or `std.*`. Modules are
    /// loaded only when the program references one of the alias names, so an
    /// ordinary program does not grow its module graph or startup cost.
    fn load_official_aliases(&mut self) -> Result<Vec<OfficialAlias>, String> {
        let Some(stdlib) = self.stdlib.clone() else {
            return Ok(Vec::new());
        };
        if stdlib.public_aliases().is_empty() {
            return Ok(Vec::new());
        }
        let referenced = referenced_identifiers(&self.program);
        let mut aliases = Vec::new();
        for alias in stdlib.public_aliases() {
            if !alias
                .public_names
                .iter()
                .any(|name| referenced.contains(name))
            {
                continue;
            }
            let resolved = stdlib.resolve(&alias.target_module)?;
            let module_id = self.load_module(&resolved.package, &resolved.relative_path)?;
            let Some(module) = self.program.module(&module_id) else {
                return Err(format!(
                    "stdlib public alias target module `{}` was not loaded",
                    alias.target_module
                ));
            };
            let Some(function) = module.function(&alias.target_function) else {
                return Err(format!(
                    "stdlib public alias `{}` target `{}::{}` does not exist",
                    alias.public_names.join("/"),
                    alias.target_module,
                    alias.target_function
                ));
            };
            if let Some(expected) = &alias.expected_signature {
                let parsed =
                    marqdo_frontend::signature::parse(expected, marqdo_span::Span::default());
                if parsed.has_errors() {
                    return Err(format!(
                        "stdlib public alias `{}` has an invalid expected_signature `{expected}`",
                        alias.public_names.join("/")
                    ));
                }
                let expected_names: Vec<&str> = parsed
                    .params
                    .iter()
                    .map(|param| param.name.as_str())
                    .collect();
                let actual_names: Vec<&str> = function
                    .params
                    .iter()
                    .map(|param| param.name.as_str())
                    .collect();
                if expected_names != actual_names
                    || function.signature != marqdo_ir::SignatureKind::Explicit
                {
                    return Err(format!(
                        "stdlib public alias `{}` expected signature `{expected}` does not match \
                         target `{}::{}` (params {:?}, kind {:?})",
                        alias.public_names.join("/"),
                        alias.target_module,
                        alias.target_function,
                        actual_names,
                        function.signature
                    ));
                }
            }
            aliases.push(OfficialAlias {
                public_names: alias.public_names.clone(),
                function: function.id.clone(),
                signature: marqdo_ir::callable::CallableSignature {
                    params: function.params.clone(),
                    kind: function.signature,
                },
                owner_package: module_id.instance.clone(),
                effects_requested: alias.effects.clone(),
            });
        }
        Ok(aliases)
    }
}

/// Collect every identifier referenced by a program.
///
/// This is a conservative "is this alias used at all?" scan; it only decides
/// whether an official alias target module should be loaded. It does not decide
/// call targets (that is the single binding pass).
fn referenced_identifiers(program: &Program) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    for module in program.modules.values() {
        for function in &module.functions {
            for param in &function.params {
                if let Some(default) = &param.default {
                    collect_identifiers_expr(default, &mut names);
                }
            }
            collect_identifiers_stmts(&function.body, &mut names);
        }
    }
    names
}

fn collect_identifiers_stmts(stmts: &[Stmt], names: &mut std::collections::BTreeSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { target, value, .. } => {
                if let PlaceTarget::Index { base, index, .. } = target {
                    collect_identifiers_expr(base, names);
                    collect_identifiers_expr(index, names);
                }
                collect_identifiers_expr(value, names);
            }
            Stmt::Eval { expr, .. } | Stmt::Return { value: expr, .. } => {
                collect_identifiers_expr(expr, names)
            }
            Stmt::Branch { arms, .. } => {
                for arm in arms {
                    collect_identifiers_expr(&arm.condition, names);
                    collect_identifiers_stmts(&arm.body, names);
                }
            }
            Stmt::Loop {
                condition, body, ..
            } => {
                collect_identifiers_expr(condition, names);
                collect_identifiers_stmts(body, names);
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }
}

fn collect_identifiers_expr(expr: &Expr, names: &mut std::collections::BTreeSet<String>) {
    match expr {
        Expr::Ident(ident) => {
            names.insert(ident.name.clone());
        }
        Expr::List(items) => items
            .iter()
            .for_each(|item| collect_identifiers_expr(item, names)),
        Expr::Record(entries) => entries
            .iter()
            .for_each(|entry| collect_identifiers_expr(&entry.value, names)),
        Expr::Unary { operand, .. } => collect_identifiers_expr(operand, names),
        Expr::Binary { left, right, .. } => {
            collect_identifiers_expr(left, names);
            collect_identifiers_expr(right, names);
        }
        Expr::Member { base, .. } => collect_identifiers_expr(base, names),
        Expr::Index { base, index, .. } => {
            collect_identifiers_expr(base, names);
            collect_identifiers_expr(index, names);
        }
        Expr::Call { callee, args, .. } => {
            collect_identifiers_expr(callee, names);
            for arg in args {
                collect_identifiers_expr(&arg.value, names);
            }
        }
        Expr::Int(_) | Expr::Float(_) | Expr::Text(_) | Expr::Bool(_) | Expr::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_project(files: &[(&str, &str)]) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("marqdo-loader-test-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for (name, source) in files {
            std::fs::write(root.join(name), source).unwrap();
        }
        root
    }

    #[test]
    fn project_snapshot_digests_are_captured_once() {
        let root = temp_project(&[
            ("marqdo.yaml", "project:\n  name: snap\n  version: 0.1.0\n"),
            ("index.mq.md", "## @exec main\n\n`return 1`\n"),
        ]);
        let entry = root.join("index.mq.md");
        let project = load(&entry).expect("load project");
        let manifest_bytes = std::fs::read(root.join("marqdo.yaml")).unwrap();
        assert_eq!(project.manifest_digest, bytes_digest(&manifest_bytes));
        assert!(project.lock_digest.is_empty());

        // The snapshot is what the program used; editing the manifest after
        // loading cannot change the recorded digest.
        std::fs::write(
            root.join("marqdo.yaml"),
            "project:\n  name: snap\n  version: 0.2.0\n",
        )
        .unwrap();
        assert_eq!(project.manifest_digest, bytes_digest(&manifest_bytes));
        let _ = std::fs::remove_dir_all(&root);
    }
}
