//! Static call validation.
//!
//! Calls are resolved after all functions have signatures (including inferred
//! ones), so arity/name mistakes are compile errors with source spans instead
//! of surprises at runtime. Type errors stay runtime concerns: v1 has no static
//! type inference beyond literal checks.

use std::collections::{BTreeMap, BTreeSet};

use marqdo_expression::{Arg, Expr, Ident};
use marqdo_ir::{builtins, Module, ModuleId, Param, PlaceTarget, Program, SignatureKind, Stmt};
use marqdo_span::Span;
use marqdo_value::{codes, Diagnostic};

/// Signature information needed to validate a call.
#[derive(Debug, Clone)]
pub struct CallableSignature {
    /// Parameter list.
    pub params: Vec<Param>,
    /// Explicit signatures allow positional arguments; inferred do not.
    pub kind: SignatureKind,
}

/// Build the callable signature table for a module.
pub fn signature_table(module: &Module) -> BTreeMap<String, CallableSignature> {
    let mut table = BTreeMap::new();
    for function in &module.functions {
        table.insert(
            function.id.name.clone(),
            CallableSignature {
                params: function.params.clone(),
                kind: function.signature,
            },
        );
    }
    table
}

/// Validate every call in every function body of the module.
pub fn validate_module(module: &Module) -> Vec<Diagnostic> {
    let table = signature_table(module);
    let mut diagnostics = Vec::new();
    for function in &module.functions {
        let mut locals: BTreeSet<String> = function
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect();
        collect_assigned(&function.body, &mut locals);
        let mut check = CallChecker {
            current_namespace: function.namespace.clone(),
            locals: &locals,
            table: &table,
            imports: module
                .imports
                .iter()
                .map(|import| import.alias.clone())
                .collect(),
            module_id: module.id.clone(),
            program: None,
            cross_only: false,
            diagnostics: Vec::new(),
        };
        check.stmts(&function.body);
        diagnostics.extend(check.diagnostics);
    }
    diagnostics
}

/// Validate cross-module and qualified-builtin calls once a whole program is
/// loaded. Local calls are already validated by [`validate_module`].
pub fn validate_program(program: &Program) -> Vec<(ModuleId, Diagnostic)> {
    let mut results = Vec::new();
    for module in program.modules.values() {
        let table = signature_table(module);
        for function in &module.functions {
            let mut locals: BTreeSet<String> = function
                .params
                .iter()
                .map(|param| param.name.clone())
                .collect();
            collect_assigned(&function.body, &mut locals);
            let mut check = CallChecker {
                current_namespace: function.namespace.clone(),
                locals: &locals,
                table: &table,
                imports: module
                    .imports
                    .iter()
                    .map(|import| import.alias.clone())
                    .collect(),
                module_id: module.id.clone(),
                program: Some(program),
                cross_only: true,
                diagnostics: Vec::new(),
            };
            check.stmts(&function.body);
            results.extend(
                check
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| (module.id.clone(), diagnostic)),
            );
        }
    }
    results
}

fn collect_assigned(stmts: &[Stmt], out: &mut BTreeSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign {
                target: PlaceTarget::Local { name, .. },
                ..
            } => {
                out.insert(name.clone());
            }
            Stmt::Branch { arms, .. } => {
                for arm in arms {
                    collect_assigned(&arm.body, out);
                }
            }
            Stmt::Loop { body, .. } => collect_assigned(body, out),
            _ => {}
        }
    }
}

struct CallChecker<'a> {
    current_namespace: Option<String>,
    locals: &'a BTreeSet<String>,
    table: &'a BTreeMap<String, CallableSignature>,
    imports: BTreeSet<String>,
    module_id: ModuleId,
    program: Option<&'a Program>,
    cross_only: bool,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> CallChecker<'a> {
    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign { target, value, .. } => {
                    if let PlaceTarget::Index { base, index, .. } = target {
                        self.expr(base);
                        self.expr(index);
                    }
                    self.expr(value);
                }
                Stmt::Eval { expr: value, .. } | Stmt::Return { value, .. } => {
                    self.expr(value);
                }
                Stmt::Branch { arms, .. } => {
                    for arm in arms {
                        self.expr(&arm.condition);
                        self.stmts(&arm.body);
                    }
                }
                Stmt::Loop {
                    condition, body, ..
                } => {
                    self.expr(condition);
                    self.stmts(body);
                }
            }
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Int(_) | Expr::Float(_) | Expr::Text(_) | Expr::Bool(_) | Expr::None => {}
            Expr::Ident(_) => {}
            Expr::List(items) => items.iter().for_each(|item| self.expr(item)),
            Expr::Record(entries) => entries.iter().for_each(|entry| self.expr(&entry.value)),
            Expr::Unary { operand, .. } => self.expr(operand),
            Expr::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Member { base, .. } => self.expr(base),
            Expr::Index { base, index, .. } => {
                self.expr(base);
                self.expr(index);
            }
            Expr::Call { callee, args, span } => {
                self.call(callee, args, *span);
                for arg in args {
                    self.expr(&arg.value);
                }
            }
        }
    }

    fn call(&mut self, callee: &Expr, args: &[Arg], span: Span) {
        match callee {
            Expr::Ident(Ident { name, .. }) => {
                if self.cross_only {
                    return;
                }
                if let Some(builtin) = builtins::find(name) {
                    if let Err(message) = builtins::validate_args(builtin, args) {
                        self.diagnostics.push(
                            Diagnostic::error(codes::CALL_ARITY, Some(span), message)
                                .with_hint("check the builtin parameter list"),
                        );
                    }
                    return;
                }
                let name_span = match callee {
                    Expr::Ident(Ident { span, .. }) => *span,
                    _ => span,
                };
                if let Some(signature) = self.resolve(name) {
                    self.validate_against(name, signature, args, span, name_span);
                    return;
                }
                if self.locals.contains(name) || self.imports.contains(name) {
                    // A local may hold a FunctionRef value; resolved at runtime.
                    return;
                }
                self.diagnostics.push(
                    Diagnostic::error(
                        codes::SYMBOL_UNKNOWN,
                        Some(name_span),
                        format!("unknown function `{name}`"),
                    )
                    .with_hint("define it as a level-2 heading or call a builtin"),
                );
            }
            Expr::Member { base, name, .. } => {
                if let Expr::Ident(Ident { name: alias, .. }) = base.as_ref() {
                    let qualified = format!("{}.{name}", alias);
                    if let Some(builtin) = builtins::find(&qualified) {
                        if let Err(message) = builtins::validate_args(builtin, args) {
                            self.diagnostics.push(
                                Diagnostic::error(codes::CALL_ARITY, Some(span), message)
                                    .with_hint("check the builtin parameter list"),
                            );
                        }
                        return;
                    }
                    if self.imports.contains(alias) {
                        if let Some(program) = self.program {
                            if let Some(target_module) =
                                program.resolve_import(&self.module_id, alias)
                            {
                                if let Some(target) = program.module(target_module) {
                                    if let Some(function) = target.function(name) {
                                        let signature = CallableSignature {
                                            params: function.params.clone(),
                                            kind: function.signature,
                                        };
                                        self.validate_against(
                                            &qualified, &signature, args, span, span,
                                        );
                                        return;
                                    }
                                    self.diagnostics.push(
                                        Diagnostic::error(
                                            codes::SYMBOL_UNKNOWN,
                                            Some(span),
                                            format!("module `{alias}` has no function `{name}`"),
                                        )
                                        .with_hint(
                                            "check `inspect --signature` on the imported module",
                                        ),
                                    );
                                    return;
                                }
                            }
                        }
                    }
                    // A declared ABI export is a static descriptor: validate
                    // its signature here, without loading the shared library.
                    if let Some(program) = self.program {
                        if let Some(descriptor) = program.external_callable(&qualified) {
                            if let Err(message) =
                                descriptor.signature.validate_args(args, &qualified)
                            {
                                self.diagnostics.push(
                                    Diagnostic::error(codes::CALL_ARITY, Some(span), message)
                                        .with_hint("check the plugin manifest export signature"),
                                );
                            }
                            return;
                        }
                    }
                }
                self.expr(base);
            }
            other => self.expr(other),
        }
    }

    fn resolve(&self, name: &str) -> Option<&'a CallableSignature> {
        if let Some(namespace) = &self.current_namespace {
            if let Some(found) = self.table.get(&format!("{namespace}.{name}")) {
                return Some(found);
            }
        }
        if let Some(found) = self.table.get(name) {
            return Some(found);
        }
        // Unique local name across namespaces.
        let mut matches = self
            .table
            .iter()
            .filter(|(key, _)| key.ends_with(&format!(".{name}")))
            .map(|(_, signature)| signature);
        let first = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            Some(first)
        }
    }

    fn validate_against(
        &mut self,
        name: &str,
        signature: &CallableSignature,
        args: &[Arg],
        call_span: Span,
        name_span: Span,
    ) {
        if signature.kind == SignatureKind::Inferred {
            if let Some(positional) = args.iter().find(|arg| arg.name.is_none()) {
                self.diagnostics.push(
                    Diagnostic::error(
                        codes::CALL_ARITY,
                        Some(positional.span),
                        format!(
                            "`{name}` has an inferred signature and only accepts named arguments"
                        ),
                    )
                    .with_hint("pass arguments as `name = value`"),
                );
            }
        }
        let mut supplied: BTreeSet<&str> = BTreeSet::new();
        let mut positional = 0usize;
        for arg in args {
            match &arg.name {
                Some(argument_name) => {
                    if !signature
                        .params
                        .iter()
                        .any(|param| param.name == *argument_name)
                    {
                        self.diagnostics.push(
                            Diagnostic::error(
                                codes::CALL_ARITY,
                                Some(arg.span),
                                format!("`{name}` has no parameter named `{argument_name}`"),
                            )
                            .with_hint("check `inspect --signature`"),
                        );
                    }
                    supplied.insert(argument_name.as_str());
                }
                None => {
                    if let Some(param) = signature.params.get(positional) {
                        supplied.insert(param.name.as_str());
                        positional += 1;
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            codes::CALL_ARITY,
                            Some(call_span),
                            format!(
                                "`{name}` accepts at most {} positional argument(s)",
                                signature.params.len()
                            ),
                        ));
                    }
                }
            }
        }
        for param in &signature.params {
            if param.default.is_none() && !supplied.contains(param.name.as_str()) {
                self.diagnostics.push(
                    Diagnostic::error(
                        codes::CALL_ARITY,
                        Some(name_span),
                        format!("`{name}` is missing required argument `{}`", param.name),
                    )
                    .with_hint("pass it by name, e.g. `x = value`"),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use marqdo_expression::parse_expression;
    use marqdo_ir::{Function, FunctionId, ModuleId, Param, UnitId};

    fn function(name: &str, params: &[&str], kind: SignatureKind, body: Vec<Stmt>) -> Function {
        Function {
            id: FunctionId {
                module: ModuleId::from("index.mq.md"),
                name: name.to_string(),
            },
            local_name: name.to_string(),
            namespace: None,
            unit: UnitId {
                module: ModuleId::from("index.mq.md"),
                path: vec![name.to_string()],
            },
            heading_span: Span::new(0, 0),
            span: Span::new(0, 0),
            params: params
                .iter()
                .map(|name| Param {
                    name: name.to_string(),
                    span: Span::new(0, 0),
                    type_annotation: None,
                    default: None,
                })
                .collect(),
            signature: kind,
            body,
        }
    }

    fn make_module(functions: Vec<Function>) -> Module {
        Module {
            id: ModuleId::from("index.mq.md"),
            units: vec![],
            functions,
            imports: vec![],
            resources: vec![],
            anchors: std::collections::BTreeMap::new(),
        }
    }

    fn return_call(code: &str) -> Stmt {
        Stmt::Return {
            value: parse_expression(code).unwrap(),
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn unknown_function_is_an_error() {
        let module = make_module(vec![function(
            "main",
            &[],
            SignatureKind::Explicit,
            vec![return_call("nope(1)")],
        )]);
        let diagnostics = validate_module(&module);
        assert!(diagnostics.iter().any(|d| d.code == codes::SYMBOL_UNKNOWN));
    }

    #[test]
    fn declared_plugin_export_is_checked_statically() {
        use marqdo_ir::callable::{
            CallableDescriptor, CallableId, CallableOrigin, CallableSignature,
        };
        let module = make_module(vec![function(
            "main",
            &[],
            SignatureKind::Explicit,
            vec![return_call("solver.add(a=1)")],
        )]);
        let mut program = marqdo_ir::Program::default();
        program.modules.insert(module.id.clone(), module.clone());
        program.external_callables.push(CallableDescriptor {
            id: CallableId("plugin:demo:solver.add".into()),
            public_name: "solver.add".into(),
            owner_package: "demo".into(),
            signature: CallableSignature {
                params: vec![
                    Param {
                        name: "a".into(),
                        span: Span::new(0, 0),
                        type_annotation: None,
                        default: None,
                    },
                    Param {
                        name: "b".into(),
                        span: Span::new(0, 0),
                        type_annotation: None,
                        default: None,
                    },
                ],
                kind: SignatureKind::Explicit,
            },
            effects_requested: vec!["host".into()],
            origin: CallableOrigin::AbiPlugin {
                package: "demo".into(),
                export: "solver.add".into(),
            },
            implementation_digest: None,
            source_span: None,
        });
        let diagnostics = validate_program(&program);
        assert!(diagnostics.iter().any(|(_, d)| d.code == codes::CALL_ARITY));
    }

    #[test]
    fn inferred_signature_requires_named_arguments() {
        let callee = function("f", &["n"], SignatureKind::Inferred, vec![]);
        let module = make_module(vec![
            function(
                "main",
                &[],
                SignatureKind::Explicit,
                vec![return_call("f(1)")],
            ),
            callee,
        ]);
        let diagnostics = validate_module(&module);
        assert!(diagnostics.iter().any(|d| d.code == codes::CALL_ARITY));
    }

    #[test]
    fn explicit_signature_allows_positional_and_checks_arity() {
        let callee = function("f", &["a", "b"], SignatureKind::Explicit, vec![]);
        let module = make_module(vec![
            function(
                "main",
                &[],
                SignatureKind::Explicit,
                vec![return_call("f(1, 2)")],
            ),
            callee,
        ]);
        assert!(validate_module(&module).is_empty());
        let module = make_module(vec![
            function(
                "main",
                &[],
                SignatureKind::Explicit,
                vec![return_call("f(1)")],
            ),
            function("f", &["a", "b"], SignatureKind::Explicit, vec![]),
        ]);
        assert!(validate_module(&module)
            .iter()
            .any(|d| d.code == codes::CALL_ARITY));
    }

    #[test]
    fn builtin_arity_is_checked() {
        let module = make_module(vec![function(
            "main",
            &[],
            SignatureKind::Explicit,
            vec![return_call("打印()")],
        )]);
        assert!(validate_module(&module)
            .iter()
            .any(|d| d.code == codes::CALL_ARITY));
    }
}
