//! Static parameter inference via definite-assignment data-flow analysis.
//!
//! Parameters come only from free reads of executable expressions. Prose,
//! headings, inert code spans and natural language never introduce parameters.
//!
//! The analysis tracks, for every program point:
//! * `must`  – variables definitely assigned on every path from entry;
//! * `may`   – variables assigned on at least one path;
//! * `ambiguous` – variables that may have been assigned before the read on
//!   some path but not on all paths, which makes the variable's identity
//!   ambiguous (blueprint S-04) rather than silently inventing a parameter.
//!
//! Loop bodies get special treatment: their first iteration starts from the
//! pre-loop state, so a read of an entry variable inside a loop is a parameter
//! candidate even though the body also assigns it (the classic counter case).

use std::collections::{BTreeMap, BTreeSet};

use marqdo_expression::{Arg, Expr, Ident, RecordEntry};
use marqdo_ir::{BranchArm, Stmt};
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
    ambiguous: BTreeSet<String>,
}

impl State {
    fn join(&self, other: &State) -> State {
        State {
            must: self.must.intersection(&other.must).cloned().collect(),
            may: self.may.union(&other.may).cloned().collect(),
            ambiguous: self.ambiguous.union(&other.ambiguous).cloned().collect(),
        }
    }
}

struct Analyzer<'a> {
    /// Names that are parameters/locals/builtins/imports and therefore never
    /// reported as unknown; used for explicit signatures.
    known: Option<&'a BTreeSet<String>>,
    reads: BTreeMap<String, Span>,
    diagnostics: Vec<Diagnostic>,
    ambiguous_reported: BTreeSet<String>,
    unknown_reported: BTreeSet<String>,
}

impl<'a> Analyzer<'a> {
    fn new(known: Option<&'a BTreeSet<String>>) -> Self {
        Self {
            known,
            reads: BTreeMap::new(),
            diagnostics: Vec::new(),
            ambiguous_reported: BTreeSet::new(),
            unknown_reported: BTreeSet::new(),
        }
    }

    fn read_name(&mut self, name: &str, span: Span, state: &State) {
        if state.must.contains(name) {
            return;
        }
        if state.ambiguous.contains(name) {
            if !self.ambiguous_reported.contains(name) {
                self.ambiguous_reported.insert(name.to_string());
                self.diagnostics.push(
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
                // A bare identifier callee names a function, not a variable.
                match callee.as_ref() {
                    Expr::Ident(_) => {}
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

    fn assign(&self, name: &str, state: &mut State) {
        state.must.insert(name.to_string());
        state.may.insert(name.to_string());
        state.ambiguous.insert(name.to_string());
    }

    fn analyze_stmts(&mut self, stmts: &[Stmt], mut state: State) -> State {
        for stmt in stmts {
            state = match stmt {
                Stmt::Assign { name, value, .. } => {
                    self.read_expr(value, &state);
                    self.assign(name, &mut state);
                    state
                }
                Stmt::Eval { expr, .. } => {
                    self.read_expr(expr, &state);
                    state
                }
                Stmt::Call { call, .. } => {
                    for Arg { value, .. } in &call.args {
                        self.read_expr(value, &state);
                    }
                    state
                }
                Stmt::Return { value, .. } => {
                    self.read_expr(value, &state);
                    state
                }
                Stmt::Branch { arms, .. } => self.analyze_branch(arms, state),
                Stmt::Loop {
                    condition, body, ..
                } => {
                    self.read_expr(condition, &state);
                    // First iteration starts from the pre-loop state: body
                    // assignments must not flag pre-loop reads as ambiguous.
                    let body_out = self.analyze_stmts(
                        body,
                        State {
                            must: state.must.clone(),
                            may: state.may.clone(),
                            ambiguous: state.ambiguous.clone(),
                        },
                    );
                    State {
                        must: state.must.clone(),
                        may: body_out
                            .may
                            .union(&state.may)
                            .cloned()
                            .collect::<BTreeSet<_>>(),
                        ambiguous: body_out
                            .ambiguous
                            .union(&state.ambiguous)
                            .cloned()
                            .collect::<BTreeSet<_>>(),
                    }
                }
            };
        }
        state
    }

    fn analyze_branch(&mut self, arms: &[BranchArm], state: State) -> State {
        // Conditions are evaluated in order against the incoming state.
        for arm in arms {
            self.read_expr(&arm.condition, &state);
        }
        // The branch may match no arm, so the incoming state joins every arm's
        // state; variables assigned only inside an arm stay "may assigned".
        let mut joined: Option<State> = Some(State {
            must: state.must.clone(),
            may: state.may.clone(),
            ambiguous: state.ambiguous.clone(),
        });
        for arm in arms {
            let arm_out = self.analyze_stmts(
                &arm.body,
                State {
                    must: state.must.clone(),
                    may: state.may.clone(),
                    ambiguous: state.ambiguous.clone(),
                },
            );
            joined = Some(match joined {
                None => arm_out,
                Some(previous) => previous.join(&arm_out),
            });
        }
        match joined {
            Some(joined) => joined,
            None => state,
        }
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
    analyzer.analyze_stmts(stmts, State::default());
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
    use marqdo_expression::{parse_expression, parse_statement};

    fn assign(name: &str, value: &str) -> Stmt {
        Stmt::Assign {
            name: name.to_string(),
            name_span: Span::new(0, 0),
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
        assert_eq!(params(&stmts), vec!["n".to_string()]);
    }

    #[test]
    fn partial_branch_assignment_is_ambiguous() {
        let stmts = vec![
            Stmt::Branch {
                arms: vec![BranchArm {
                    condition: parse_expression("true").unwrap(),
                    condition_span: Span::new(0, 0),
                    body: vec![assign("x", "1")],
                    span: Span::new(0, 0),
                }],
                span: Span::new(0, 0),
            },
            ret("x"),
        ];
        let inference = infer(&stmts, None, &BTreeSet::new());
        assert!(inference.params.is_empty());
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
