//! Compilation pipeline for one document: Markdown AST → unit index → IR.
//!
//! `analyze_source` never executes anything and never loads third-party code.
//! It is the entry point used by `check`, `inspect` and (before running) `run`.

use std::collections::BTreeSet;

use marqdo_ir::{Function, ImportDecl, Module, ModuleId, Param, SignatureKind};
use marqdo_markdown::MarkdownDocument;
use marqdo_value::{codes, Diagnostic};

use crate::calls;
use crate::infer;
use crate::map;
use crate::resources;
use crate::signature;
use crate::units::{self, UnitIndex};

/// Result of analyzing one source document.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Canonical module AST (available even when diagnostics contain errors).
    pub module: Module,
    /// Parsed Markdown document.
    pub document: MarkdownDocument,
    /// Unit index for inspection.
    pub units: UnitIndex,
    /// All diagnostics, in deterministic order.
    pub diagnostics: Vec<Diagnostic>,
}

impl AnalysisResult {
    /// Whether any diagnostic is an error.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// Analyze one Markdown source as a Marqdo module.
///
/// `module_id` is the canonical project-relative identity (e.g. `index.mq.md`).
///
/// This standalone entry point also runs the module-local call check so that a
/// single document can be validated without a program graph. The CLI
/// composition root uses [`analyze_source_for_program`] instead: it defers all
/// call resolution to the single program-wide binding pass (ADR-0054).
pub fn analyze_source(module_id: &str, source: &str) -> AnalysisResult {
    analyze_source_inner(module_id, source, true)
}

/// Analyze one Markdown source for the composition root.
///
/// Identical to [`analyze_source`] except that it does not run the provisional
/// module-local call check; the caller binds calls exactly once over the whole
/// program (including extension descriptors) and therefore owns the only
/// authoritative call diagnostics.
pub fn analyze_source_for_program(module_id: &str, source: &str) -> AnalysisResult {
    analyze_source_inner(module_id, source, false)
}

fn analyze_source_inner(
    module_id: &str,
    source: &str,
    validate_local_calls: bool,
) -> AnalysisResult {
    let id = ModuleId::from(module_id);
    let document = marqdo_markdown::parse(source);
    let units = units::build_units(&document, &id);
    let mut diagnostics = units.diagnostics.clone();

    // Explicit front matter imports. Nothing is inferred.
    let mut imports: Vec<ImportDecl> = Vec::new();
    if let Some(frontmatter) = document.frontmatter() {
        match &frontmatter.yaml {
            Err(error) => diagnostics.push(
                Diagnostic::error(
                    codes::FRONTMATTER_INVALID,
                    Some(frontmatter.yaml_span),
                    format!("invalid YAML front matter: {error}"),
                )
                .with_hint("front matter must be valid YAML between `---` delimiters"),
            ),
            Ok(_) => {
                for key in ["marqdo_semantics", "syntax"] {
                    if frontmatter.has_key(key) {
                        diagnostics.push(
                            Diagnostic::error(
                                codes::SEM_UNSUPPORTED_VERSION_DIRECTIVE,
                                Some(frontmatter.yaml_span),
                                format!(
                                    "`{key}` is not a syntax selector; there is one current syntax"
                                ),
                            )
                            .with_hint(
                                "remove the directive and migrate the source to RFC-LANG-0002 syntax",
                            ),
                        );
                    }
                }
                match frontmatter.imports() {
                    Ok(mapping) => {
                        for (alias, target) in mapping {
                            imports.push(ImportDecl {
                                alias,
                                target,
                                span: frontmatter.span,
                            });
                        }
                    }
                    Err(error) => diagnostics.push(
                        Diagnostic::error(
                            codes::FRONTMATTER_INVALID,
                            Some(frontmatter.span),
                            format!("invalid import declaration: {error}"),
                        )
                        .with_hint(
                            "imports are an explicit mapping: `import: { alias: \"@module\" }`",
                        ),
                    ),
                }
            }
        }
    }

    let resources = resources::collect(&document, &units, &id);
    let import_names: BTreeSet<String> =
        imports.iter().map(|import| import.alias.clone()).collect();

    let mut functions: Vec<Function> = Vec::new();
    for scope in &units.functions {
        let mut mapped = map::map_function(&document, &units, scope);
        diagnostics.append(&mut mapped.diagnostics);
        // The mapper already proved that this payload is the function's first
        // direct executable payload; a Knowledge section can never supply one.
        let signature = mapped.params.as_ref().map(|declaration| {
            signature::from_payload(&declaration.text, declaration.span, &declaration.segments)
        });
        let explicit = signature.as_ref();

        let mut params: Vec<Param> = Vec::new();
        let mut kind = SignatureKind::Inferred;
        if let Some(sig) = explicit {
            kind = SignatureKind::Explicit;
            diagnostics.extend(sig.diagnostics.clone());
            params = sig.params.clone();
        }
        let inference = infer::infer(&mapped.stmts, explicit, &import_names);
        diagnostics.extend(inference.diagnostics);
        if kind == SignatureKind::Inferred && !inference.params.is_empty() {
            diagnostics.push(Diagnostic::warning(
                codes::SEM_INFERRED_SIGNATURE,
                Some(scope.heading_span),
                "function signature was inferred; declare it with `params(...)`",
            ));
            params = inference
                .params
                .into_iter()
                .map(|candidate| Param {
                    name: candidate.name,
                    span: candidate.span,
                    type_annotation: None,
                    default: None,
                })
                .collect();
        }
        functions.push(Function {
            id: scope.id.clone(),
            local_name: scope.local_name.clone(),
            namespace: scope.namespace.clone(),
            unit: scope.unit.clone(),
            heading_span: scope.heading_span,
            span: scope.span,
            params,
            signature: kind,
            body: mapped.stmts,
        });
    }

    let module = Module {
        id,
        units: units.units.clone(),
        functions,
        imports,
        resources,
        anchors: units.anchors.clone(),
    };
    if validate_local_calls {
        diagnostics.extend(calls::validate_module(&module));
    }

    // One stable order for every consumer: source position first, then the
    // stable code, as required by `spec/mapping.md` §8. Diagnostics without a
    // precise span sort last.
    diagnostics.sort_by(|left, right| {
        let left_key = (left.span.map(|span| span.start), left.code.as_str());
        let right_key = (right.span.map(|span| span.start), right.code.as_str());
        left_key
            .0
            .unwrap_or(usize::MAX)
            .cmp(&right_key.0.unwrap_or(usize::MAX))
            .then_with(|| left_key.1.cmp(right_key.1))
    });

    AnalysisResult {
        module,
        document,
        units,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(source: &str) -> AnalysisResult {
        analyze_source("index.mq.md", source)
    }

    #[test]
    fn maps_recursive_factorial() {
        let source = r#"# @knowledge 数学

## @exec 阶乘

`if n <= 1`

`return 1`

`else`

`if n > 1`

`结果 = n * 阶乘(n=n-1)`

`return 结果`

`end`

`end`
"#;
        let result = analyze(source);
        assert!(
            !result.has_errors(),
            "unexpected diagnostics: {:#?}",
            result.diagnostics
        );
        let function = result.module.function("数学.阶乘").expect("factorial");
        assert_eq!(function.signature, SignatureKind::Inferred);
        assert_eq!(function.params.len(), 1);
        assert_eq!(function.params[0].name, "n");
        assert!(matches!(function.body[0], marqdo_ir::Stmt::Branch { .. }));
    }

    #[test]
    fn maps_countdown_loop() {
        let source = r#"## @exec 倒计时

`n = 3`

`while n > 0`

`打印(n)`

`n = n - 1`

`end`

`return n`
"#;
        let result = analyze(source);
        assert!(!result.has_errors(), "{:#?}", result.diagnostics);
        let function = result.module.function("倒计时").expect("countdown");
        assert!(function.params.is_empty());
        assert!(matches!(function.body[1], marqdo_ir::Stmt::Loop { .. }));
    }

    #[test]
    fn diagnostics_are_sorted_by_position_then_code() {
        // The duplicate-heading error is found while indexing (later in the
        // file); the payload syntax error is found while mapping (earlier).
        // The public order must be by source position, not pipeline stage.
        let source = "## @exec f\n\n`x =`\n\n## @exec f\n";
        let result = analyze(source);
        let codes: Vec<&str> = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert_eq!(
            codes,
            vec!["sem.syntax", "sem.duplicate_unit_name"],
            "{:#?}",
            result.diagnostics
        );
    }

    #[test]
    fn knowledge_section_cannot_declare_a_signature() {
        // The `params(...)` payload sits in a Knowledge section, so it is inert
        // documentation and must not become the function's signature.
        let source = "## @exec f\n\n### @knowledge 说明\n\n`params(x: Int)`\n\n### @exec 计算\n\n`return 1`\n";
        let result = analyze(source);
        assert!(
            !result.has_errors(),
            "unexpected diagnostics: {:#?}",
            result.diagnostics
        );
        let function = result.module.function("f").expect("f");
        assert_eq!(function.signature, SignatureKind::Inferred);
        assert!(function.params.is_empty());
    }

    #[test]
    fn exec_section_params_after_prose_is_the_signature() {
        // Prose and a Knowledge subsection may precede the declaration, so a
        // later Exec section can still declare the first executable payload.
        let source = "## @exec f\n\n说明文字。\n\n### @knowledge 原理\n\n`params(ignored: Int)`\n\n### @exec 计算\n\n`params(x: Int)`\n\n`return x`\n";
        let result = analyze(source);
        assert!(
            !result.has_errors(),
            "unexpected diagnostics: {:#?}",
            result.diagnostics
        );
        let function = result.module.function("f").expect("f");
        assert_eq!(function.signature, SignatureKind::Explicit);
        assert_eq!(function.params.len(), 1);
        assert_eq!(function.params[0].name, "x");
    }

    #[test]
    fn unmatched_end_points_at_the_end_payload() {
        let source = "## @exec f\n\n`x = 1`\n\n`end`\n";
        let result = analyze(source);
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "sem.unmatched_end")
            .expect("sem.unmatched_end");
        let span = diagnostic.span.expect("span");
        assert_eq!(
            &source[span.start..span.end],
            "`end`",
            "the diagnostic must point at the stray end payload"
        );
    }

    #[test]
    fn invalid_strong_payload_is_a_hard_error() {
        let result = analyze("## @exec f\n\n**a *b* c**\n");
        assert!(result.has_errors());
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == "sem.payload_nested"));
    }

    #[test]
    fn invalid_list_header_is_a_hard_error() {
        // RFC-LANG-0002: a plain list is inert Markdown; the v2 negative is an
        // unclosed control frame inside the owning Exec H2.
        let result = analyze("## @exec f\n\n`if 1`\n");
        assert!(result.has_errors());
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == "sem.unclosed_control"));
    }

    /// Strip source spans so that pure narrative edits (which shift byte
    /// offsets) still compare equal, as required by blueprint 14.2.
    fn without_spans(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut out = serde_json::Map::new();
                for (key, value) in map {
                    if key == "span"
                        || key == "heading_span"
                        || key == "name_span"
                        || key == "condition_span"
                    {
                        continue;
                    }
                    out.insert(key.clone(), without_spans(value));
                }
                serde_json::Value::Object(out)
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(without_spans).collect())
            }
            other => other.clone(),
        }
    }

    #[test]
    fn prose_changes_do_not_change_ir() {
        let base = "## @exec f\n\n说明文字。\n\n`x = 1`\n\n更多说明。\n\n`return x`\n";
        let edited =
            "## @exec f\n\n另一段完全不同的叙述。\n\n`x = 1`\n\n继续叙述。\n\n`return x`\n";
        let a = analyze(base);
        let b = analyze(edited);
        let a_ir = serde_json::to_value(&a.module.functions).unwrap();
        let b_ir = serde_json::to_value(&b.module.functions).unwrap();
        assert_eq!(without_spans(&a_ir), without_spans(&b_ir));
    }
}
