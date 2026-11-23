//! Static parameter inference via definite-assignment data-flow analysis.
//!
//! Parameters come only from free reads of executable expressions. Prose,
//! headings, inert code spans and natural language never introduce parameters.
//!
//! Each reachable path tracks definite and possible assignments. Sequences
//! merge fallthrough, break and continue separately, so a transfer never
//! contributes bindings from the statements it skips. A return ends the path.
//!
//! The first iteration starts from the pre-loop state. A first read before a
//! body assignment can therefore still be a parameter (the counter case).

use std::collections::{BTreeMap, BTreeSet};

use marqdo_expression::{Arg, Expr, Ident, RecordEntry};
use marqdo_ir::{BranchArm, PlaceTarget, Stmt};
use marqdo_span::Span;
use marqdo_value::{codes, Diagnostic};

use crate::signature::Signature;

/// Result of the inference analysis for one function.
#[derive(Debug, Clone, Default)]
pub struct Inference {
    /// Candidate parameters sorted by canonical name.
    pub params: Vec<ParamCandidate>,
    /// Diagnostics (ambiguity errors, explicit-signature unknown reads).
    pub diagnostics: Vec<Diagnostic>,
}

/// One inferred parameter candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamCandidate {
    /// Canonical parameter name.
    pub name: String,
    /// Span of the first free read.
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
struct State {
    must: BTreeSet<String>,
    may: BTreeSet<String>,
}

impl State {
    fn join(&self, other: &State) -> State {
        State {
            must: self.must.intersection(&other.must).cloned().collect(),
            may: self.may.union(&other.may).cloned().collect(),
        }
    }
}

/// Join only reachable paths. An absent path contributes no assignments.
fn merge_paths(left: &mut Option<State>, right: Option<State>) {
    if let Some(right) = right {
        *left = Some(match left.take() {
            Some(left) => left.join(&right),
            None => right,
        });
    }
}

#[derive(Default)]
struct Flow {
    next: Option<State>,
    breaks: Option<State>,
    continues: Option<State>,
}

impl Flow {
    fn merge(&mut self, other: Self) {
        merge_paths(&mut self.next, other.next);
        self.merge_loop_transfers(other.breaks, other.continues);
    }

    fn merge_loop_transfers(&mut self, breaks: Option<State>, continues: Option<State>) {
        merge_paths(&mut self.breaks, breaks);
        merge_paths(&mut self.continues, continues);
    }
}

struct Analyzer<'a> {
    /// Names that are parameters/locals/builtins/imports and therefore never
    /// reported as unknown; used for explicit signatures.
    known: Option<&'a BTreeSet<String>>,
    reads: BTreeMap<String, Span>,
    diagnostics: Vec<Diagnostic>,
    ambiguous_reads: BTreeMap<String, Span>,
    unknown_reported: BTreeSet<String>,
}

impl<'a> Analyzer<'a> {
    fn new(known: Option<&'a BTreeSet<String>>) -> Self {
        Self {
            known,
            reads: BTreeMap::new(),
            diagnostics: Vec::new(),
            ambiguous_reads: BTreeMap::new(),
            unknown_reported: BTreeSet::new(),
        }
    }

    fn read_name(&mut self, name: &str, span: Span, state: &State) {
        if state.must.contains(name) {
            return;
        }
        if state.may.contains(name) {
            self.ambiguous_reads.entry(name.to_string()).or_insert(span);
            return;
        }
        if let Some(known) = self.known {
            if known.contains(name) {
                return;
            }
            if !self.unknown_reported.contains(name) {
                self.unknown_reported.insert(name.to_string());
                self.diagnostics.push(
                    Diagnostic::error(
                        codes::SYMBOL_UNKNOWN,
                        Some(span),
                        format!("unknown variable `{name}`"),
                    )
                    .with_hint("declare it in the explicit signature or assign it first"),
                );
            }
            return;
        }
        self.reads.entry(name.to_string()).or_insert(span);
    }

    fn read_expr(&mut self, expr: &Expr, state: &State) {
        match expr {
            Expr::Int(_) | Expr::Float(_) | Expr::Text(_) | Expr::Bool(_) | Expr::None => {}
            Expr::Ident(Ident { name, span }) => self.read_name(name, *span, state),
            Expr::List(items) => {
                for item in items {
                    self.read_expr(item, state);
                }
            }
            Expr::Record(entries) => {
                for RecordEntry { value, .. } in entries {
                    self.read_expr(value, state);
                }
            }
            Expr::Unary { operand, .. } => self.read_expr(operand, state),
            Expr::Binary { left, right, .. } => {
                self.read_expr(left, state);
                self.read_expr(right, state);
            }
            Expr::Call { callee, args, .. } => {
                // A callee identifier names a function; a `ns.name` callee is a
                // builtin namespace (resource.request) or an imported module alias.
                // Neither is a variable read.
                match callee.as_ref() {
                    Expr::Ident(_) => {}
                    Expr::Member { base, name, .. } => {
                        // `ns.f(...)` bases are namespaces: builtin namespaces,
                        // imported module aliases or explicitly trusted plugin
                        // namespaces bound at run time. They are never inferred
                        // parameters. Non-identifier bases are values and are
                        // still read normally.
                        let _ = name;
                        if !matches!(base.as_ref(), Expr::Ident(_)) {
                            self.read_expr(base, state);
                        }
                    }
                    other => self.read_expr(other, state),
                }
                for Arg { value, .. } in args {
                    self.read_expr(value, state);
                }
            }
            Expr::Member { base, .. } => self.read_expr(base, state),
            Expr::Index { base, index, .. } => {
                self.read_expr(base, state);
                self.read_expr(index, state);
            }
        }
    }

    fn analyze_stmts(&mut self, stmts: &[Stmt], state: State) -> Flow {
        let mut flow = Flow {
            next: Some(state),
            ..Flow::default()
        };
        for stmt in stmts {
            let Some(mut state) = flow.next.take() else {
                break;
            };
            flow.next = match stmt {
                Stmt::Assign { target, value, .. } => {
                    match target {
                        PlaceTarget::Local { name, .. } => {
                            self.read_expr(value, &state);
                            state.must.insert(name.clone());
                            state.may.insert(name.clone());
                        }
                        PlaceTarget::Index { base, index, .. } => {
                            // Keep the runtime's evaluation order.
                            self.read_expr(base, &state);
                            self.read_expr(index, &state);
                            self.read_expr(value, &state);
                        }
                    }
                    Some(state)
                }
                Stmt::Eval { expr, .. } => {
                    self.read_expr(expr, &state);
                    Some(state)
                }
                Stmt::Return { value, .. } => {
                    self.read_expr(value, &state);
                    None
                }
                Stmt::Break { .. } => {
                    merge_paths(&mut flow.breaks, Some(state));
                    None
                }
                Stmt::Continue { .. } => {
                    merge_paths(&mut flow.continues, Some(state));
                    None
                }
                Stmt::Branch { arms, .. } => {
                    let branch = self.analyze_branch(arms, state);
                    flow.merge_loop_transfers(branch.breaks, branch.continues);
                    branch.next
                }
                Stmt::Loop {
                    condition, body, ..
                } => self.analyze_loop(condition, body, state),
            };
        }
        flow
    }

    fn analyze_branch(&mut self, arms: &[BranchArm], state: State) -> Flow {
        let mut flow = Flow::default();
        for arm in arms {
            // Only earlier false conditions lead to this arm. Conditions do
            // not change the assignment state of the current frame.
            self.read_expr(&arm.condition, &state);
            if arm.condition != Expr::Bool(false) {
                flow.merge(self.analyze_stmts(&arm.body, state.clone()));
            }
            if arm.condition == Expr::Bool(true) {
                return flow;
            }
        }
        merge_paths(&mut flow.next, Some(state));
        flow
    }

    fn analyze_loop(&mut self, condition: &Expr, body: &[Stmt], state: State) -> Option<State> {
        self.read_expr(condition, &state);
        if *condition == Expr::Bool(false) {
            return Some(state);
        }

        let Flow {
            mut next,
            mut breaks,
            continues,
        } = self.analyze_stmts(body, state.clone());
        merge_paths(&mut next, continues);
        // A later break can inherit assignments from an earlier iteration.
        // It can also occur on the first, so only "may" gains those bindings.
        if let (Some(breaks), Some(backedge)) = (&mut breaks, &next) {
            breaks.may.extend(backedge.may.iter().cloned());
        }
        if *condition == Expr::Bool(true) {
            return breaks;
        }

        // A false condition may be seen before any iteration or after a
        // backedge. A break exits directly and a return never reaches here.
        merge_paths(&mut next, Some(state));
        merge_paths(&mut next, breaks);
        next
    }
}

/// Infer parameters for a function with no explicit signature.
pub fn infer(
    stmts: &[Stmt],
    signature: Option<&Signature>,
    imports: &BTreeSet<String>,
) -> Inference {
    let known = signature.map(|signature| {
        let mut names: BTreeSet<String> = signature
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect();
        names.extend(imports.iter().cloned());
        names.extend(
            marqdo_ir::builtins::BUILTINS
                .iter()
                .map(|builtin| builtin.name.to_string()),
        );
        names
    });
    let mut analyzer = Analyzer::new(known.as_ref());
    let initial = signature.map_or_else(State::default, |signature| {
        let bound: BTreeSet<_> = signature
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect();
        State {
            must: bound.clone(),
            may: bound,
        }
    });
    analyzer.analyze_stmts(stmts, initial);
    for (name, span) in analyzer.ambiguous_reads {
        // A reachable free read makes the name an entry parameter throughout
        // an inferred function, including before or after a local rebinding.
        if !analyzer.reads.contains_key(&name) {
            analyzer.diagnostics.push(
                Diagnostic::error(
                    codes::SYMBOL_UNINITIALIZED,
                    Some(span),
                    format!("`{name}` may be read before it is assigned"),
                )
                .with_hint(
                    "assign it on every path before this read, or pass it in as a parameter",
                ),
            );
        }
    }
    let mut params: Vec<ParamCandidate> = analyzer
        .reads
        .iter()
        .filter(|(name, _)| {
            // Builtins and imported aliases are never parameters.
            marqdo_ir::builtins::find(name).is_none() && !imports.contains(*name)
        })
        .map(|(name, span)| ParamCandidate {
            name: name.clone(),
            span: *span,
        })
        .collect();
    params.sort_by(|a, b| a.name.cmp(&b.name));
    Inference {
        params,
        diagnostics: analyzer.diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use marqdo_expression::parse_expression;

    fn assign(name: &str, value: &str) -> Stmt {
        Stmt::Assign {
            target: PlaceTarget::Local {
                name: name.to_string(),
                name_span: Span::new(0, 0),
            },
            value: parse_expression(value).unwrap(),
            span: Span::new(0, 0),
        }
    }

    fn ret(expr: &str) -> Stmt {
        Stmt::Return {
            value: parse_expression(expr).unwrap(),
            span: Span::new(0, 0),
        }
    }

    fn params(stmts: &[Stmt]) -> Vec<String> {
        infer(stmts, None, &BTreeSet::new())
            .params
            .into_iter()
            .map(|candidate| candidate.name)
            .collect()
    }

    #[test]
    fn index_assignment_reads_its_base_and_index_without_binding_a_local() {
        let stmts = vec![
            Stmt::Assign {
                target: PlaceTarget::Index {
                    base: parse_expression("m").unwrap(),
                    index: parse_expression("i").unwrap(),
                    span: Span::new(0, 0),
                },
                value: parse_expression("9").unwrap(),
                span: Span::new(0, 0),
            },
            ret("m[0]"),
        ];
        // `m` and `i` are free reads; the place never rebinds `m` as a local.
        assert_eq!(params(&stmts), vec!["i".to_string(), "m".to_string()]);
    }

    #[test]
    fn infers_free_reads() {
        // n * 阶乘(n=n-1)
        let call: Expr = parse_expression("n * 阶乘(n=n-1)").unwrap();
        let stmts = vec![Stmt::Return {
            value: call,
            span: Span::new(0, 0),
        }];
        assert_eq!(params(&stmts), vec!["n".to_string()]);
    }

    #[test]
    fn unconditional_entry_assignment_is_local() {
        let stmts = vec![assign("n", "3"), ret("n")];
        assert!(params(&stmts).is_empty());
    }

    #[test]
    fn loop_counter_is_parameter() {
        let stmts = vec![
            Stmt::Loop {
                condition: parse_expression("n > 0").unwrap(),
                condition_span: Span::new(0, 0),
                body: vec![assign("n", "n - 1")],
                span: Span::new(0, 0),
            },
            ret("n"),
        ];
        let inference = infer(&stmts, None, &BTreeSet::new());
        assert_eq!(inference.params[0].name, "n");
        assert_eq!(inference.params.len(), 1);
        assert!(
            inference.diagnostics.is_empty(),
            "{:?}",
            inference.diagnostics
        );
    }

    #[test]
    fn partial_branch_assignment_is_ambiguous() {
        let stmts = vec![
            Stmt::Branch {
                arms: vec![BranchArm {
                    condition: parse_expression("flag").unwrap(),
                    condition_span: Span::new(0, 0),
                    body: vec![assign("x", "1")],
                    span: Span::new(0, 0),
                }],
                span: Span::new(0, 0),
            },
            ret("x"),
        ];
        let inference = infer(&stmts, None, &BTreeSet::new());
        assert_eq!(
            inference
                .params
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>(),
            ["flag"]
        );
        assert!(inference
            .diagnostics
            .iter()
            .any(|d| d.code == codes::SYMBOL_UNINITIALIZED));
    }

    #[test]
    fn explicit_signature_reports_unknown_reads() {
        let signature = Signature {
            params: vec![marqdo_ir::Param {
                name: "r".into(),
                span: Span::new(0, 0),
                type_annotation: None,
                default: None,
            }],
            span: Span::new(0, 0),
            diagnostics: Vec::new(),
        };
        let stmts = vec![ret("r * pi * typo")];
        let inference = infer(&stmts, Some(&signature), &BTreeSet::new());
        assert!(inference.params.is_empty());
        assert!(inference
            .diagnostics
            .iter()
            .any(|d| d.code == codes::SYMBOL_UNKNOWN && d.message.contains("typo")));
    }
}
