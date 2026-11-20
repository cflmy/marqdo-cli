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
pub fn analyze_source(module_id: &str, source: &str) -> AnalysisResult {
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
            Ok(_) => match frontmatter.imports() {
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
                    .with_hint("imports are an explicit mapping: `import: { alias: \"@module\" }`"),
                ),
            },
        }
    }

    let resources = resources::collect(&document, &units, &id);
    let import_names: BTreeSet<String> =
        imports.iter().map(|import| import.alias.clone()).collect();

    let mut functions: Vec<Function> = Vec::new();
    for scope in &units.functions {
        let blocks: Vec<&marqdo_markdown::MdNode> = document.root.children
            [scope.body_start..scope.body_end]
            .iter()
            .collect();
        let signature = signature::detect(&document, &blocks);
        let explicit = signature.as_ref().map(|(_, sig)| sig);

        let mut mapped = map::map_function(&document, &units, &imports, scope);
        diagnostics.append(&mut mapped.diagnostics);

        let mut params: Vec<Param> = Vec::new();
        let mut kind = SignatureKind::Inferred;
        if let Some(sig) = explicit {
            kind = SignatureKind::Explicit;
            diagnostics.extend(sig.diagnostics.clone());
            params = sig.params.clone();
        }
        let inference = infer::infer(&mapped.stmts, explicit, &import_names);
        diagnostics.extend(inference.diagnostics);
        if kind == SignatureKind::Inferred {
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
    diagnostics.extend(calls::validate_module(&module));

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
        let source = r#"# 数学

## 阶乘

1. `n <= 1`

   *1*

2. `n > 1`

   **结果 = n * 阶乘(n=n-1)**

   *结果*
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
        let source = r#"## 倒计时

**n = 3**

- `n > 0`

  > 打印(n)

  **n = n - 1**

*n*
"#;
        let result = analyze(source);
        assert!(!result.has_errors(), "{:#?}", result.diagnostics);
        let function = result.module.function("倒计时").expect("countdown");
        assert!(function.params.is_empty());
        assert!(matches!(function.body[1], marqdo_ir::Stmt::Loop { .. }));
    }

    #[test]
    fn invalid_strong_payload_is_a_hard_error() {
        let result = analyze("## f\n\n**a *b* c**\n");
        assert!(result.has_errors());
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == codes::MARKDOWN_INVALID_CONSTRUCT));
    }

    #[test]
    fn invalid_list_header_is_a_hard_error() {
        let result = analyze("## f\n\n1. 没有条件\n");
        assert!(result.has_errors());
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == codes::MARKDOWN_INVALID_CONSTRUCT));
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
        let base = "## f\n\n说明文字。\n\n**x = 1**\n\n更多说明。\n\n*x*\n";
        let edited = "## f\n\n另一段完全不同的叙述。\n\n**x = 1**\n\n继续叙述。\n\n*x*\n";
        let a = analyze(base);
        let b = analyze(edited);
        let a_ir = serde_json::to_value(&a.module.functions).unwrap();
        let b_ir = serde_json::to_value(&b.module.functions).unwrap();
        assert_eq!(without_spans(&a_ir), without_spans(&b_ir));
    }
}
