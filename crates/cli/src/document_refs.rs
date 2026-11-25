//! One document-link resolver and its explicit catalogs (spec/analysis.md §5).
//!
//! The resolver is a pure function over a [`DocumentCatalog`]: it never loads a
//! module, binds a call, imports, executes or accesses the network. Ordinary
//! Analysis uses the loaded-module catalog only. `check --refs` and
//! `inspect --refs` may extend a separate catalog with directly referenced
//! local files through [`extend_for_reference`]; those files never enter the
//! program, its imports or its call bindings.
//!
//! All states are explicit: a local candidate outside the catalog is
//! `not_loaded` (unverified), never `missing`.

use marqdo_span::Span;
use marqdo_value::{codes, Diagnostic};
use std::collections::{BTreeMap, BTreeSet};

use crate::loader::LoadedProject;

/// Public state of one Markdown reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceState {
    /// Exact target in this catalog, or an explicitly verified local file.
    Resolved,
    /// Local candidate is outside this catalog; existence was not verified.
    NotLoaded,
    /// Known module lacks the fragment, or explicit verification proved absence.
    Missing,
    /// Canonical-name fallback produced several exact candidates.
    Ambiguous,
    /// URI with an external scheme; never accessed.
    External,
    /// Local target escapes the allowed package root or is absolute.
    OutsideScope,
    /// Malformed URI, percent encoding or unsupported address.
    Invalid,
}

impl ReferenceState {
    /// Stable public name.
    pub fn as_str(self) -> &'static str {
        match self {
            ReferenceState::Resolved => "resolved",
            ReferenceState::NotLoaded => "not_loaded",
            ReferenceState::Missing => "missing",
            ReferenceState::Ambiguous => "ambiguous",
            ReferenceState::External => "external",
            ReferenceState::OutsideScope => "outside_scope",
            ReferenceState::Invalid => "invalid",
        }
    }
}

/// Kind of a resolved local target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    /// A loaded/analysed `.mq.md` module.
    Module,
    /// A regular local file that was explicitly verified to exist.
    File,
}

impl TargetKind {
    /// Stable public name.
    pub fn as_str(self) -> &'static str {
        match self {
            TargetKind::Module => "module",
            TargetKind::File => "file",
        }
    }
}

/// One resolved public reference record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// Source module id.
    pub source: marqdo_ir::ModuleId,
    /// Innermost owning unit of the link.
    pub source_unit: marqdo_ir::UnitId,
    /// Raw URL as written.
    pub url: String,
    /// Link node span.
    pub span: Span,
    /// Resolution state.
    pub state: ReferenceState,
    /// Resolved target kind.
    pub target_kind: Option<TargetKind>,
    /// Resolved target module.
    pub target_module: Option<marqdo_ir::ModuleId>,
    /// Resolved target unit.
    pub target_unit: Option<marqdo_ir::UnitId>,
    /// Candidate package instance for a local target.
    pub candidate_package_instance_id: Option<String>,
    /// Normalised package-relative candidate path.
    pub candidate_path: Option<String>,
    /// Once-decoded fragment, `None` when absent or empty.
    pub fragment: Option<String>,
    /// Exact unit ids for an ambiguous canonical-name fallback.
    pub candidates: Vec<marqdo_ir::UnitId>,
}

impl Reference {
    fn placeholder(
        source: &marqdo_ir::ModuleId,
        units: &marqdo_frontend::UnitIndex,
        url: &str,
        span: Span,
    ) -> Self {
        let source_unit = units
            .unit_containing_span(span)
            .unwrap_or_else(|| marqdo_ir::UnitId {
                module: source.clone(),
                path: Vec::new(),
            });
        Self {
            source: source.clone(),
            source_unit,
            url: url.to_string(),
            span,
            state: ReferenceState::Invalid,
            target_kind: None,
            target_module: None,
            target_unit: None,
            candidate_package_instance_id: None,
            candidate_path: None,
            fragment: None,
            candidates: Vec::new(),
        }
    }
}

/// Facts needed to resolve a fragment in one document.
#[derive(Debug, Clone)]
pub struct ModuleFacts {
    /// Anchor map (same namespace as `UnitIndex.anchors`).
    pub anchors: BTreeMap<String, marqdo_ir::UnitId>,
    /// Exact canonical names in document order.
    pub names: BTreeMap<String, Vec<marqdo_ir::UnitId>>,
}

/// An explicit catalog of documents available to the resolver.
#[derive(Debug, Clone, Default)]
pub struct DocumentCatalog {
    modules: BTreeMap<marqdo_ir::ModuleId, ModuleFacts>,
    path_index: BTreeMap<(String, String), marqdo_ir::ModuleId>,
    instances: BTreeSet<String>,
    verified_files: BTreeSet<(String, String)>,
    known_missing: BTreeSet<(String, String)>,
}

impl DocumentCatalog {
    /// Build the ordinary loaded-module catalog. No extra file is read.
    pub fn from_project(project: &LoadedProject) -> Self {
        let mut catalog = Self::default();
        catalog.instances.extend(project.packages.keys().cloned());
        for (module_id, file) in &project.files {
            catalog.instances.insert(module_id.instance.clone());
            catalog.insert_module(module_id.clone(), &file.analysis.units);
        }
        catalog
    }

    /// Add one analysed document's facts.
    pub fn insert_module(
        &mut self,
        module: marqdo_ir::ModuleId,
        units: &marqdo_frontend::UnitIndex,
    ) {
        let mut names: BTreeMap<String, Vec<marqdo_ir::UnitId>> = BTreeMap::new();
        for unit in &units.units {
            if !unit.name.is_empty() {
                names
                    .entry(unit.name.clone())
                    .or_default()
                    .push(unit.id.clone());
            }
        }
        self.path_index.insert(
            (module.instance.clone(), module.relative_path.clone()),
            module.clone(),
        );
        self.modules.insert(
            module.clone(),
            ModuleFacts {
                anchors: units.anchors.clone(),
                names,
            },
        );
    }

    fn facts(&self, module: &marqdo_ir::ModuleId) -> Option<&ModuleFacts> {
        self.modules.get(module)
    }

    fn module_for(&self, instance: &str, relative_path: &str) -> Option<&marqdo_ir::ModuleId> {
        self.path_index
            .get(&(instance.to_string(), relative_path.to_string()))
    }

    fn has_instance(&self, instance: &str) -> bool {
        self.instances.contains(instance)
    }
}

/// One reference-related diagnostic tied to its module.
#[derive(Debug, Clone)]
pub struct ReferenceIssue {
    /// Module whose file owns the link.
    pub module: marqdo_ir::ModuleId,
    /// Diagnostic to attach to that file.
    pub diagnostic: Diagnostic,
}

/// Catalogue scope requested by a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceMode {
    /// Ordinary Analysis: loaded modules only, never open a link target.
    LoadedOnly,
    /// `--refs`: directly referenced local targets may be verified once.
    VerifyLocal,
}

/// All references and the diagnostics implied by the requested scope.
#[derive(Debug, Clone, Default)]
pub struct ReferenceReport {
    /// References per module, in link source order.
    pub by_module: BTreeMap<marqdo_ir::ModuleId, Vec<Reference>>,
    /// Diagnostics produced by explicit verification (`--refs` only).
    pub issues: Vec<ReferenceIssue>,
}

/// Collect and resolve every Markdown link in the loaded project.
pub fn references_for_project(project: &LoadedProject, mode: ReferenceMode) -> ReferenceReport {
    let mut catalog = DocumentCatalog::from_project(project);
    let raw = collect_links(project);
    let mut resolved = resolve_all(&catalog, &raw);

    let mut extension_errors: Vec<ReferenceIssue> = Vec::new();
    if mode == ReferenceMode::VerifyLocal {
        let mut budget = CatalogBudget::default();
        let mut attempted: BTreeSet<(String, String)> = BTreeSet::new();
        for reference in &resolved {
            let (Some(instance), Some(path)) = (
                reference.candidate_package_instance_id.as_deref(),
                reference.candidate_path.as_deref(),
            ) else {
                continue;
            };
            if reference.state != ReferenceState::NotLoaded {
                continue;
            }
            if !attempted.insert((instance.to_string(), path.to_string())) {
                continue;
            }
            if let Err(error) = extend_for_reference(project, &mut catalog, reference, &mut budget)
            {
                let mut diagnostic = Diagnostic::error(
                    codes::REF_INVALID,
                    Some(reference.span),
                    format!(
                        "cannot verify reference target `{}`: {error}",
                        reference.url
                    ),
                );
                diagnostic = diagnostic.with_hint(
                    "fix the link target, or remove the broken link from the explicit refs check",
                );
                extension_errors.push(ReferenceIssue {
                    module: reference.source.clone(),
                    diagnostic,
                });
            }
        }
        resolved = resolve_all(&catalog, &raw);
    }

    let mut by_module: BTreeMap<marqdo_ir::ModuleId, Vec<Reference>> = BTreeMap::new();
    for reference in resolved {
        by_module
            .entry(reference.source.clone())
            .or_default()
            .push(reference);
    }
    for references in by_module.values_mut() {
        references.sort_by_key(|reference| reference.span.start);
    }
    let mut issues = if mode == ReferenceMode::VerifyLocal {
        reference_issues(&by_module)
    } else {
        Vec::new()
    };
    issues.extend(extension_errors);
    ReferenceReport { by_module, issues }
}

fn reference_issues(
    by_module: &BTreeMap<marqdo_ir::ModuleId, Vec<Reference>>,
) -> Vec<ReferenceIssue> {
    let mut issues = Vec::new();
    for references in by_module.values() {
        for reference in references {
            let (severity, code, message) = match reference.state {
                ReferenceState::Resolved | ReferenceState::External => continue,
                ReferenceState::NotLoaded => (
                    "warning",
                    codes::REF_NOT_LOADED,
                    format!(
                        "reference `{}` was not verified by the selected catalog",
                        reference.url
                    ),
                ),
                ReferenceState::Missing => (
                    "error",
                    codes::REF_UNRESOLVED,
                    format!("reference target `{}` does not exist", reference.url),
                ),
                ReferenceState::Ambiguous => (
                    "error",
                    codes::REF_AMBIGUOUS,
                    format!(
                        "reference `{}` matches more than one exact heading; candidates: {}",
                        reference.url,
                        reference
                            .candidates
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ),
                ReferenceState::OutsideScope | ReferenceState::Invalid => (
                    "error",
                    codes::REF_INVALID,
                    format!(
                        "reference `{}` is not a valid in-scope target",
                        reference.url
                    ),
                ),
            };
            let hint = match reference.state {
                ReferenceState::NotLoaded => {
                    "link to an imported module or a `.mq.md` heading; plain Markdown is not a Marqdo program"
                }
                ReferenceState::Missing => "fix the link target or add the missing heading/file",
                ReferenceState::Ambiguous => {
                    "use the exact anchor or rename one of the headings; no target was chosen"
                }
                _ => "use a relative package path or a documented external URL",
            };
            let mut diagnostic = if severity == "error" {
                Diagnostic::error(code, Some(reference.span), message)
            } else {
                Diagnostic::warning(code, Some(reference.span), message)
            };
            diagnostic = diagnostic.with_hint(hint);
            issues.push(ReferenceIssue {
                module: reference.source.clone(),
                diagnostic,
            });
        }
    }
    issues.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| {
                left.diagnostic
                    .span
                    .map(|span| span.start)
                    .unwrap_or(usize::MAX)
                    .cmp(
                        &right
                            .diagnostic
                            .span
                            .map(|span| span.start)
                            .unwrap_or(usize::MAX),
                    )
            })
            .then_with(|| left.diagnostic.code.cmp(&right.diagnostic.code))
    });
    issues
}

struct RawLink<'a> {
    module: &'a marqdo_ir::ModuleId,
    units: &'a marqdo_frontend::UnitIndex,
    url: &'a str,
    span: Span,
}

fn collect_links(project: &LoadedProject) -> Vec<RawLink<'_>> {
    let mut links = Vec::new();
    for (module_id, file) in &project.files {
        file.analysis.document.root.walk(&mut |node| {
            if let marqdo_markdown::MdKind::Link { url, .. } = &node.kind {
                links.push(RawLink {
                    module: module_id,
                    units: &file.analysis.units,
                    url,
                    span: node.span,
                });
            }
        });
    }
    links
}

fn resolve_all(catalog: &DocumentCatalog, raw: &[RawLink<'_>]) -> Vec<Reference> {
    raw.iter()
        .map(|link| resolve_link(catalog, link.module, link.units, link.url, link.span))
        .collect()
}

/// Resolve one Markdown URL against an explicit catalog.
pub fn resolve_link(
    catalog: &DocumentCatalog,
    source: &marqdo_ir::ModuleId,
    units: &marqdo_frontend::UnitIndex,
    url: &str,
    span: Span,
) -> Reference {
    let mut reference = Reference::placeholder(source, &units, url, span);

    // Empty destination and `#...` both address the current document.
    if url.is_empty() {
        return document_root(catalog, &mut reference, source);
    }
    if let Some(fragment) = url.strip_prefix('#') {
        reference.target_module = Some(source.clone());
        let fragment = match decode_fragment(fragment) {
            Ok(fragment) => fragment,
            Err(message) => return invalid(&mut reference, message),
        };
        reference.fragment = fragment;
        let module = catalog.facts(source).cloned();
        return resolve_fragment(&mut reference, source, module.as_ref());
    }

    let (path_raw, fragment_raw) = match url.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (url, None),
    };
    if path_raw.is_empty() {
        reference.target_module = Some(source.clone());
        if let Some(fragment_raw) = fragment_raw {
            let fragment = match decode_fragment(fragment_raw) {
                Ok(fragment) => fragment,
                Err(message) => return invalid(&mut reference, message),
            };
            reference.fragment = fragment;
        }
        let module = catalog.facts(source).cloned();
        return resolve_fragment(&mut reference, source, module.as_ref());
    }

    let fragment = match fragment_raw {
        Some(fragment_raw) => match decode_fragment(fragment_raw) {
            Ok(fragment) => fragment,
            Err(message) => return invalid(&mut reference, message),
        },
        None => None,
    };
    reference.fragment = fragment.clone();

    match classify_uri(path_raw) {
        UriClass::External => {
            reference.state = ReferenceState::External;
            return reference;
        }
        UriClass::OutsideScope => {
            reference.state = ReferenceState::OutsideScope;
            return reference;
        }
        UriClass::Invalid(message) => return invalid(&mut reference, message),
        UriClass::Local => {}
    }

    let decoded = match percent_decode(path_raw) {
        Ok(decoded) => decoded,
        Err(message) => return invalid(&mut reference, message),
    };
    if decoded.contains('\0') {
        return invalid(&mut reference, "link path contains a NUL byte".to_string());
    }
    if decoded.contains('\\') {
        return invalid(
            &mut reference,
            "link path uses a backslash separator; use `/`".to_string(),
        );
    }
    if decoded.starts_with('/') || decoded.starts_with("//") {
        reference.state = ReferenceState::OutsideScope;
        return reference;
    }

    let source_dir_segments: Vec<&str> = source.relative_path.split('/').collect();
    let mut parts: Vec<&str> = source_dir_segments;
    parts.pop();
    let trailing_slash = decoded.ends_with('/');
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    reference.state = ReferenceState::OutsideScope;
                    return reference;
                }
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        reference.state = ReferenceState::OutsideScope;
        return reference;
    }
    let candidate_path = parts.join("/");
    reference.candidate_package_instance_id = Some(source.instance.clone());
    reference.candidate_path = Some(candidate_path.clone());
    if trailing_slash {
        // Directory links are not program documents in this contract.
        reference.state = ReferenceState::NotLoaded;
        return reference;
    }
    if !catalog.has_instance(&source.instance) {
        reference.state = ReferenceState::NotLoaded;
        return reference;
    }

    let Some(target_module) = catalog
        .module_for(&source.instance, &candidate_path)
        .cloned()
    else {
        let key = (source.instance.clone(), candidate_path.clone());
        if catalog.verified_files.contains(&key) {
            reference.state = ReferenceState::Resolved;
            reference.target_kind = Some(TargetKind::File);
            return reference;
        }
        if catalog.known_missing.contains(&key) {
            reference.state = ReferenceState::Missing;
            return reference;
        }
        reference.state = ReferenceState::NotLoaded;
        return reference;
    };

    reference.target_module = Some(target_module.clone());
    if candidate_path.ends_with(".mq.md") && catalog.facts(&target_module).is_none() {
        reference.state = ReferenceState::NotLoaded;
        return reference;
    }
    if !candidate_path.ends_with(".mq.md") {
        // A plain local file with a fragment cannot be verified as a Marqdo
        // anchor; without one, explicit verification reports file existence.
        let key = (source.instance.clone(), candidate_path.clone());
        if catalog.verified_files.contains(&key) {
            reference.state = ReferenceState::Resolved;
            reference.target_kind = Some(TargetKind::File);
            reference.target_module = None;
            return reference;
        }
        reference.state = ReferenceState::NotLoaded;
        return reference;
    }
    let facts = catalog.facts(&target_module).cloned();
    resolve_fragment(&mut reference, &target_module, facts.as_ref())
}

fn document_root(
    catalog: &DocumentCatalog,
    reference: &mut Reference,
    source: &marqdo_ir::ModuleId,
) -> Reference {
    reference.target_module = Some(source.clone());
    if catalog.facts(source).is_some() {
        reference.state = ReferenceState::Resolved;
        reference.target_kind = Some(TargetKind::Module);
    } else {
        reference.state = ReferenceState::NotLoaded;
    }
    reference.clone()
}

fn resolve_fragment(
    reference: &mut Reference,
    module: &marqdo_ir::ModuleId,
    facts: Option<&ModuleFacts>,
) -> Reference {
    let Some(facts) = facts else {
        reference.state = ReferenceState::NotLoaded;
        return reference.clone();
    };
    reference.target_module = Some(module.clone());
    let Some(fragment) = reference.fragment.clone() else {
        reference.state = ReferenceState::Resolved;
        reference.target_kind = Some(TargetKind::Module);
        return reference.clone();
    };
    if fragment.is_empty() {
        reference.state = ReferenceState::Resolved;
        reference.target_kind = Some(TargetKind::Module);
        return reference.clone();
    }
    if let Some(unit) = facts.anchors.get(&fragment) {
        reference.state = ReferenceState::Resolved;
        reference.target_kind = Some(TargetKind::Module);
        reference.target_unit = Some(unit.clone());
        return reference.clone();
    }
    match facts.names.get(&fragment).map(Vec::as_slice) {
        None | Some([]) => {
            reference.state = ReferenceState::Missing;
        }
        Some([unit]) => {
            reference.state = ReferenceState::Resolved;
            reference.target_kind = Some(TargetKind::Module);
            reference.target_unit = Some(unit.clone());
        }
        Some(candidates) => {
            reference.state = ReferenceState::Ambiguous;
            reference.candidates = candidates.to_vec();
        }
    }
    reference.clone()
}

fn invalid(reference: &mut Reference, _message: String) -> Reference {
    reference.state = ReferenceState::Invalid;
    reference.clone()
}

enum UriClass {
    Local,
    External,
    OutsideScope,
    Invalid(String),
}

fn classify_uri(path: &str) -> UriClass {
    if path.starts_with("//") || path.starts_with('\\') {
        return UriClass::OutsideScope;
    }
    if path.starts_with('/') {
        return UriClass::OutsideScope;
    }
    let Some(colon) = path.find(':') else {
        return UriClass::Local;
    };
    let slash = path.find('/');
    if slash.is_some_and(|slash| slash < colon) {
        return UriClass::Local;
    }
    let scheme = &path[..colon];
    if scheme.is_empty() {
        return UriClass::Invalid("empty URI scheme".to_string());
    }
    let mut chars = scheme.chars();
    if !chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
    {
        return UriClass::Invalid(format!("invalid URI scheme `{scheme}`"));
    }
    if scheme.eq_ignore_ascii_case("file") {
        return UriClass::OutsideScope;
    }
    // A single-letter scheme followed by a separator or nothing is a Windows
    // drive or a bare drive designator, not a local package path.
    if scheme.len() == 1 {
        let rest = &path[colon + 1..];
        if rest.is_empty() || rest.starts_with('\\') || rest.starts_with('/') {
            return UriClass::OutsideScope;
        }
    }
    UriClass::External
}

fn decode_fragment(raw: &str) -> Result<Option<String>, String> {
    if raw.is_empty() {
        return Ok(None);
    }
    let decoded = percent_decode(raw)?;
    if decoded.is_empty() {
        return Ok(None);
    }
    Ok(Some(decoded))
}

/// Decode `%XX` exactly once. Raw non-ASCII UTF-8 is preserved.
pub fn percent_decode(raw: &str) -> Result<String, String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("truncated percent escape".to_string());
            }
            let high =
                hex_value(bytes[index + 1]).ok_or_else(|| "invalid percent escape".to_string())?;
            let low =
                hex_value(bytes[index + 2]).ok_or_else(|| "invalid percent escape".to_string())?;
            out.push((high << 4) | low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "percent escape does not decode to UTF-8".to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Explicit local-verification budget for one `--refs` invocation.
#[derive(Debug)]
pub struct CatalogBudget {
    /// Files opened so far.
    pub reads: usize,
    /// Maximum files opened per invocation.
    pub max_reads: usize,
    /// Maximum bytes read from one `.mq.md` candidate.
    pub max_bytes: u64,
}

impl Default for CatalogBudget {
    fn default() -> Self {
        Self {
            reads: 0,
            max_reads: 1024,
            max_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Outcome of trying to extend the explicit catalog for one candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtendOutcome {
    /// Target `.mq.md` was parsed and added to this catalog.
    LoadedModule,
    /// A regular non-program file exists and was recorded as verified.
    VerifiedFile,
    /// The target does not exist.
    Missing,
    /// The target is not a verifiable document (directory, unsupported).
    Unverified,
}

/// Read one directly referenced local target for the explicit `--refs` catalog.
///
/// The target is opened at most once per catalog invocation, inside the
/// package root, after refusing symlinks and with size/count limits. The
/// ordinary Analysis catalog never calls this.
pub fn extend_for_reference(
    project: &LoadedProject,
    catalog: &mut DocumentCatalog,
    reference: &Reference,
    budget: &mut CatalogBudget,
) -> Result<ExtendOutcome, String> {
    let (Some(instance), Some(relative_path)) = (
        reference.candidate_package_instance_id.as_deref(),
        reference.candidate_path.as_deref(),
    ) else {
        return Ok(ExtendOutcome::Unverified);
    };
    if !catalog.has_instance(instance) {
        return Ok(ExtendOutcome::Unverified);
    }
    let Some(package_root) = project.packages.get(instance) else {
        return Ok(ExtendOutcome::Unverified);
    };
    let package_root = package_root
        .canonicalize()
        .unwrap_or_else(|_| package_root.clone());
    let path = package_root.join(relative_path);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            catalog
                .known_missing
                .insert((instance.to_string(), relative_path.to_string()));
            return Ok(ExtendOutcome::Missing);
        }
        Err(error) => return Err(format!("cannot stat `{relative_path}`: {error}")),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "`{relative_path}` is a symbolic link; explicit target verification refuses symlinks"
        ));
    }
    if metadata.is_dir() {
        return Ok(ExtendOutcome::Unverified);
    }
    if !metadata.is_file() {
        return Ok(ExtendOutcome::Unverified);
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("cannot open `{relative_path}`: {error}"))?;
    if !canonical.starts_with(&package_root) {
        return Err(format!("`{relative_path}` escapes its package root"));
    }
    budget.reads += 1;
    if budget.reads > budget.max_reads {
        return Err(format!(
            "more than {} directly referenced files in one explicit refs check",
            budget.max_reads
        ));
    }
    if !relative_path.ends_with(".mq.md") {
        catalog
            .verified_files
            .insert((instance.to_string(), relative_path.to_string()));
        return Ok(ExtendOutcome::VerifiedFile);
    }
    if metadata.len() > budget.max_bytes {
        return Err(format!(
            "`{relative_path}` is larger than the {} byte explicit verification limit",
            budget.max_bytes
        ));
    }
    let source = std::fs::read_to_string(&canonical)
        .map_err(|error| format!("cannot read `{relative_path}`: {error}"))?;
    let module_id = marqdo_ir::ModuleId::new(instance, relative_path);
    let analysis = marqdo_frontend::analyze_source(&module_id.to_string(), &source);
    catalog.insert_module(module_id, &analysis.units);
    Ok(ExtendOutcome::LoadedModule)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(source: &str, id: &str) -> (marqdo_ir::ModuleId, marqdo_frontend::AnalysisResult) {
        let module_id = marqdo_ir::ModuleId::from(id);
        let analysis = marqdo_frontend::analyze_source(&module_id.to_string(), source);
        (module_id, analysis)
    }

    fn catalog_of(documents: &[(&str, &str)]) -> DocumentCatalog {
        let mut catalog = DocumentCatalog::default();
        for (id, source) in documents {
            let (module_id, analysis) = module(source, id);
            catalog.instances.insert(module_id.instance.clone());
            catalog.insert_module(module_id, &analysis.units);
        }
        catalog
    }

    fn resolve(
        catalog: &DocumentCatalog,
        source: &marqdo_ir::ModuleId,
        analysis: &marqdo_frontend::AnalysisResult,
        url: &str,
    ) -> Reference {
        resolve_link(catalog, source, &analysis.units, url, Span::new(0, 1))
    }

    #[test]
    fn same_document_anchor_uses_the_real_source_module() {
        let source = "## @exec main\n\n见[入口](#main)。\n";
        let (module_id, analysis) = module(source, "sub/doc.mq.md");
        let catalog = catalog_of(&[("sub/doc.mq.md", source)]);
        let reference = resolve(&catalog, &module_id, &analysis, "#main");
        assert_eq!(reference.state, ReferenceState::Resolved);
        assert_eq!(reference.target_module.as_ref(), Some(&module_id));
        assert!(reference
            .target_unit
            .as_ref()
            .is_some_and(|unit| unit.path.last().is_some_and(|name| name == "main")));
    }

    #[test]
    fn empty_fragment_is_the_document_root() {
        let source = "## @exec main\n\n`return 1`\n";
        let (module_id, analysis) = module(source, "index.mq.md");
        let catalog = catalog_of(&[("index.mq.md", source)]);
        let reference = resolve(&catalog, &module_id, &analysis, "#");
        assert_eq!(reference.state, ReferenceState::Resolved);
        assert_eq!(reference.fragment, None);
        assert_eq!(reference.target_unit, None);
    }

    #[test]
    fn exact_anchor_wins_and_name_fallback_can_be_ambiguous() {
        let source = "## @exec f\n";
        let (module_id, analysis) = module(source, "index.mq.md");
        let target = "# @knowledge a\n\n## @knowledge A B\n\n# @knowledge b\n\n## @knowledge A B\n";
        let (target_id, target_analysis) = module(target, "theories.mq.md");
        let mut catalog = catalog_of(&[("index.mq.md", source), ("theories.mq.md", target)]);
        catalog.instances.insert(target_id.instance.clone());
        catalog.insert_module(target_id.clone(), &target_analysis.units);
        // The exact slug resolves deterministically to the first occurrence.
        let exact = resolve(&catalog, &module_id, &analysis, "./theories.mq.md#a-b");
        assert_eq!(exact.state, ReferenceState::Resolved);
        // The exact anchor map has no `A B` key (the anchor is `a-b`), so the
        // name fallback sees two exact candidates and must not choose one.
        let ambiguous = resolve(&catalog, &module_id, &analysis, "./theories.mq.md#A B");
        assert_eq!(ambiguous.state, ReferenceState::Ambiguous, "{ambiguous:?}");
        assert_eq!(ambiguous.candidates.len(), 2);
        assert_eq!(ambiguous.target_unit, None);
    }

    #[test]
    fn unique_name_fallback_resolves_without_a_slug() {
        let source = "## @exec main\n\n[x](./targets.mq.md#A B)\n";
        let (module_id, analysis) = module(source, "index.mq.md");
        let target = "# @knowledge Namespace\n\n## @knowledge A B\n";
        let (target_id, target_analysis) = module(target, "targets.mq.md");
        let mut catalog = catalog_of(&[("index.mq.md", source), ("targets.mq.md", target)]);
        catalog.instances.insert(target_id.instance.clone());
        catalog.insert_module(target_id, &target_analysis.units);
        let reference = resolve(&catalog, &module_id, &analysis, "./targets.mq.md#A B");
        assert_eq!(reference.state, ReferenceState::Resolved, "{reference:?}");
        assert!(reference
            .target_unit
            .as_ref()
            .is_some_and(|unit| unit.path.last().is_some_and(|name| name == "A B")));
    }

    #[test]
    fn external_and_out_of_scope_targets_are_classified_without_io() {
        let source = "## @exec main\n\n`return 1`\n";
        let (module_id, analysis) = module(source, "index.mq.md");
        let catalog = catalog_of(&[("index.mq.md", source)]);
        for (url, state) in [
            ("https://example.com/x", ReferenceState::External),
            ("mailto:a@example.com", ReferenceState::External),
            ("file:///etc/passwd", ReferenceState::OutsideScope),
            ("/etc/passwd", ReferenceState::OutsideScope),
            ("../outside.md", ReferenceState::OutsideScope),
            ("%2e%2e/outside.md", ReferenceState::OutsideScope),
            ("C:\\Windows\\x.md", ReferenceState::OutsideScope),
            ("//server/share/x.md", ReferenceState::OutsideScope),
            ("#bad%zz", ReferenceState::Invalid),
        ] {
            let reference = resolve(&catalog, &module_id, &analysis, url);
            assert_eq!(reference.state, state, "{url}: {reference:?}");
        }
    }

    #[test]
    fn unloaded_cross_module_target_is_not_loaded_not_missing() {
        let source = "## @exec main\n\n[x](./theory.mq.md#formula)\n";
        let (module_id, analysis) = module(source, "index.mq.md");
        let catalog = catalog_of(&[("index.mq.md", source)]);
        let reference = resolve(&catalog, &module_id, &analysis, "./theory.mq.md#formula");
        assert_eq!(reference.state, ReferenceState::NotLoaded);
        assert_eq!(reference.candidate_path.as_deref(), Some("theory.mq.md"));
        assert_eq!(reference.fragment.as_deref(), Some("formula"));
        assert_eq!(reference.target_unit, None);
    }

    #[test]
    fn loaded_cross_module_target_resolves_through_the_catalog() {
        let source = "## @exec main\n\n[x](./theory.mq.md#formula)\n";
        let (module_id, analysis) = module(source, "index.mq.md");
        let (target_id, target_analysis) =
            module("## @knowledge formula\n\n正文\n", "theory.mq.md");
        let mut catalog = catalog_of(&[
            ("index.mq.md", source),
            ("theory.mq.md", "## @knowledge formula\n\n正文\n"),
        ]);
        catalog.instances.insert(target_id.instance.clone());
        catalog.insert_module(target_id.clone(), &target_analysis.units);
        let reference = resolve(&catalog, &module_id, &analysis, "./theory.mq.md#formula");
        assert_eq!(reference.state, ReferenceState::Resolved);
        assert_eq!(reference.target_module.as_ref(), Some(&target_id));
        assert!(reference.target_unit.is_some());
    }

    #[test]
    fn missing_anchor_in_a_known_module_is_missing() {
        let source = "## @exec main\n\n[x](./theory.mq.md#nope)\n";
        let (module_id, analysis) = module(source, "index.mq.md");
        let (target_id, target_analysis) = module("## @knowledge formula\n", "theory.mq.md");
        let mut catalog = catalog_of(&[
            ("index.mq.md", source),
            ("theory.mq.md", "## @knowledge formula\n"),
        ]);
        catalog.instances.insert(target_id.instance.clone());
        catalog.insert_module(target_id.clone(), &target_analysis.units);
        let reference = resolve(&catalog, &module_id, &analysis, "./theory.mq.md#nope");
        assert_eq!(reference.state, ReferenceState::Missing);
        assert_eq!(reference.target_module.as_ref(), Some(&target_id));
    }

    #[test]
    fn percent_decoding_happens_once_and_survives_utf8() {
        assert_eq!(percent_decode("%E8%AE%A1%E7%AE%97").unwrap(), "计算");
        assert_eq!(percent_decode("a%2520b").unwrap(), "a%20b");
        assert!(percent_decode("%zz").is_err());
        assert_eq!(
            percent_decode("100%").unwrap_err(),
            "truncated percent escape"
        );
    }
}
