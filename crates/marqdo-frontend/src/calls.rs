//! Static call resolution and binding (RFC-EXT-0001 §4, ADR-0054).
//!
//! This module owns the *single* resolution algorithm used by `check`, `run`,
//! `inspect` and Analysis: it walks every statically visible call site exactly
//! once, resolves it against [`marqdo_ir::callable::CallableRegistry`] and
//! records a [`marqdo_ir::call_binding::BoundTarget`] in one
//! [`marqdo_ir::call_binding::CallBindingTable`]. The runtime consumes that
//! table and never re-resolves a name.
//!
//! Resolution priority is explicit and order-independent:
//!
//! | call form | rule |
//! |---|---|
//! | `f(...)` | local variable (dynamic) → official alias/plugin-fixed builtin → module function (current namespace first, then canonical, then unique local) |
//! | `alias.f(...)` | explicit import alias → builtin dotted prefix → exact plugin export |
//! | `some_value.f(...)` | dynamic language value only |
//!
//! Import aliases that collide with a builtin dotted namespace or a visible
//! plugin namespace are rejected with `ext.import_namespace_conflict`; declared
//! plugin exports that collide with each other or with builtins are rejected
//! when the registry is built. The previous "try builtin, then guess plugin at
//! runtime" behaviour is gone.
//!
//! `validate_module` keeps the standalone frontend API (a single document with
//! no program graph) for AST/conformance tests: unresolved imports are
//! *deferred* there because only the composition root can bind a whole program.

use std::collections::{BTreeMap, BTreeSet};

use marqdo_expression::{Arg, Expr, Ident};
use marqdo_ir::builtins;
use marqdo_ir::call_binding::{BoundTarget, CallBindingTable, CallSiteKey};
use marqdo_ir::callable::{CallableDescriptor, CallableRegistry};
use marqdo_ir::{
    Function, FunctionId, Module, ModuleId, PlaceTarget, Program, SignatureKind, Stmt,
};
use marqdo_span::Span;
use marqdo_value::{codes, Diagnostic};

/// Build the callable signature table for one module (diagnostics helper).
pub fn signature_table(
    module: &Module,
) -> BTreeMap<String, marqdo_ir::callable::CallableSignature> {
    let mut table = BTreeMap::new();
    for function in &module.functions {
        table.insert(
            function.id.name.clone(),
            marqdo_ir::callable::CallableSignature {
                params: function.params.clone(),
                kind: function.signature,
            },
        );
    }
    table
}

/// Result of binding a whole program.
#[derive(Debug, Clone, Default)]
pub struct BoundProgram {
    /// One deterministic binding per static call site.
    pub bindings: CallBindingTable,
    /// Call-resolution diagnostics, sorted later by the caller.
    pub diagnostics: Vec<(ModuleId, Diagnostic)>,
}

/// Validate every call in a single module (standalone frontend entry).
///
/// Imports and plugin exports cannot be resolved without a program graph, so
/// those call sites are deferred to the composition-root binding pass instead
/// of producing false errors or a second priority order.
pub fn validate_module(module: &Module) -> Vec<Diagnostic> {
    let mut program = Program::default();
    program.modules.insert(module.id.clone(), module.clone());
    program.main_module = Some(module.id.clone());
    let registry = CallableRegistry::try_from_program(&program)
        .expect("a single module cannot contain duplicate external declarations");
    bind_with(module, &program, &registry, true)
        .diagnostics
        .into_iter()
        .map(|(_, diagnostic)| diagnostic)
        .collect()
}

/// Validate every call in a loaded program (tests and standalone frontend API).
///
/// The CLI composition root uses [`bind_program_calls`] and keeps the binding
/// table; this wrapper keeps the historical API for frontend tests.
pub fn validate_program(program: &Program) -> Vec<(ModuleId, Diagnostic)> {
    let registry = match CallableRegistry::try_from_program(program) {
        Ok(registry) => registry,
        Err(_) => CallableRegistry::from_program(program),
    };
    bind_program_calls_with(program, &registry, false).diagnostics
}

/// Resolve every call site in `program` against `registry` exactly once.
///
/// This is the only binding entry point used by the composition root. The
/// returned table must be installed on [`Program::set_call_bindings`] before
/// execution.
pub fn bind_program_calls(program: &Program, registry: &CallableRegistry) -> BoundProgram {
    bind_program_calls_with(program, registry, false)
}

fn bind_program_calls_with(
    program: &Program,
    registry: &CallableRegistry,
    standalone: bool,
) -> BoundProgram {
    let mut result = BoundProgram::default();
    for module in program.modules.values() {
        let mut bound = bind_with(module, program, registry, standalone);
        // Conflict diagnostics are produced once per module import, before
        // any call site; keep them in front of call diagnostics.
        result.diagnostics.append(&mut bound.diagnostics);
        for (key, target) in bound.bindings.iter() {
            result.bindings.insert(key.clone(), target.clone());
        }
    }
    result
}

fn bind_with(
    module: &Module,
    program: &Program,
    registry: &CallableRegistry,
    standalone: bool,
) -> BoundProgram {
    let mut conflicted = BTreeSet::new();
    let mut diagnostics = Vec::new();
    // Reject import aliases that collide with a fixed builtin dotted namespace
    // or a visible plugin namespace. `alias.f(...)` must have one target.
    for import in &module.imports {
        let builtin_conflict = registry.has_builtin_dotted_prefix(&import.alias);
        let plugin_conflict = registry.has_external_dotted_prefix(&import.alias);
        if builtin_conflict || plugin_conflict {
            conflicted.insert(import.alias.clone());
            let owner = if builtin_conflict {
                "a builtin dotted namespace"
            } else {
                "a declared plugin export namespace"
            };
            diagnostics.push((
                module.id.clone(),
                Diagnostic::error(
                    codes::EXT_IMPORT_NAMESPACE_CONFLICT,
                    Some(import.span),
                    format!("import alias `{}` conflicts with {owner}", import.alias),
                )
                .with_hint(format!(
                    "rename the import alias, for example `{}_lib`, so `{}.…` has one meaning",
                    import.alias, import.alias
                )),
            ));
        }
    }
    let mut bound = BoundProgram::default();
    for function in &module.functions {
        let mut locals: BTreeSet<String> = function
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect();
        collect_assigned(&function.body, &mut locals);
        let mut binder = Binder {
            module,
            program,
            registry,
            function_id: function.id.clone(),
            namespace: function.namespace.clone(),
            locals: &locals,
            standalone,
            conflicted: &conflicted,
            bindings: CallBindingTable::default(),
            diagnostics: Vec::new(),
        };
        for param in &function.params {
            // Signature defaults are compile-time expressions evaluated by the
            // runtime in the callee's own frame; bind their calls with the
            // callee identity so the runtime lookup matches.
            if let Some(default) = &param.default {
                binder.expr(default);
            }
        }
        binder.stmts(&function.body);
        for (key, target) in binder.bindings.iter() {
            bound.bindings.insert(key.clone(), target.clone());
        }
        diagnostics.extend(
            binder
                .diagnostics
                .into_iter()
                .map(|diagnostic| (module.id.clone(), diagnostic)),
        );
    }
    BoundProgram {
        bindings: bound.bindings,
        diagnostics,
    }
}

fn collect_assigned(stmts: &[Stmt], out: &mut BTreeSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { target, .. } => {
                if let PlaceTarget::Local { name, .. } = target {
                    out.insert(name.clone());
                }
            }
            Stmt::Branch { arms, .. } => {
                for arm in arms {
                    collect_assigned(&arm.body, out);
                }
            }
            Stmt::Loop { body, .. } => collect_assigned(body, out),
            Stmt::Eval { .. } | Stmt::Return { .. } => {}
        }
    }
}

struct Binder<'a> {
    module: &'a Module,
    program: &'a Program,
    registry: &'a CallableRegistry,
    function_id: FunctionId,
    namespace: Option<String>,
    locals: &'a BTreeSet<String>,
    standalone: bool,
    conflicted: &'a BTreeSet<String>,
    bindings: CallBindingTable,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Binder<'a> {
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

    fn bind(&mut self, span: Span, target: BoundTarget) {
        let key = CallSiteKey::new(self.function_id.clone(), span);
        self.bindings.insert(key, target);
    }

    fn diagnostic(&mut self, code: &str, span: Span, message: String, hint: Option<String>) {
        let mut diagnostic = Diagnostic::error(code, Some(span), message);
        if let Some(hint) = hint {
            diagnostic = diagnostic.with_hint(hint);
        }
        self.diagnostics.push(diagnostic);
    }

    fn call(&mut self, callee: &Expr, args: &[Arg], span: Span) {
        match callee {
            Expr::Ident(Ident {
                name,
                span: name_span,
            }) => {
                if self.locals.contains(name) {
                    self.bind(span, BoundTarget::DynamicValue);
                    return;
                }
                if let Some(target) = self.registry.alias_target(name).cloned() {
                    if let Some(descriptor) = self.registry.builtin(name) {
                        self.validate_signature(
                            &descriptor.signature,
                            args,
                            name,
                            span,
                            *name_span,
                        );
                    }
                    self.bind(span, BoundTarget::Marqdo { function: target });
                    return;
                }
                if let Some(builtin) = builtins::find(name) {
                    if let Err(message) = builtins::validate_args(builtin, args) {
                        self.diagnostic(
                            codes::CALL_ARITY,
                            span,
                            message,
                            Some("check the builtin parameter list".to_string()),
                        );
                    }
                    self.bind(
                        span,
                        BoundTarget::Builtin {
                            canonical_name: builtin.name.to_string(),
                        },
                    );
                    return;
                }
                match self.resolve_module_function(name) {
                    Resolution::One(function) => {
                        if let Some(target) = self.program.function(&function) {
                            let signature = marqdo_ir::callable::CallableSignature {
                                params: target.params.clone(),
                                kind: target.signature,
                            };
                            self.validate_signature(&signature, args, name, span, *name_span);
                        }
                        self.bind(span, BoundTarget::Marqdo { function });
                    }
                    Resolution::Ambiguous => {
                        self.diagnostic(
                            codes::SYMBOL_UNKNOWN,
                            *name_span,
                            format!("ambiguous function name `{name}`"),
                            Some(
                                "qualify it with its namespace, for example `namespace.函数(...)`"
                                    .to_string(),
                            ),
                        );
                    }
                    Resolution::None => {
                        if self.standalone
                            && self
                                .module
                                .imports
                                .iter()
                                .any(|import| import.alias == *name)
                        {
                            // A local may hold import-related values in the
                            // standalone frontend API; defer.
                            return;
                        }
                        self.diagnostic(
                            codes::SYMBOL_UNKNOWN,
                            *name_span,
                            format!("unknown function `{name}`"),
                            Some(
                                "define it as a level-2 heading, call a builtin, or load the \
                                 plugin that declares it"
                                    .to_string(),
                            ),
                        );
                    }
                }
            }
            Expr::Member { base, name, .. } => {
                if let Expr::Ident(Ident {
                    name: alias,
                    span: alias_span,
                }) = base.as_ref()
                {
                    if self.locals.contains(alias) {
                        self.bind(span, BoundTarget::DynamicValue);
                        return;
                    }
                    if self.conflicted.contains(alias) {
                        // The import conflict diagnostic already tells the user
                        // that `alias.…` has no single meaning; do not cascade.
                        return;
                    }
                    if let Some(target_module) =
                        self.program.resolve_import(&self.module.id, alias).cloned()
                    {
                        if let Some(function) = self.registry.function_id(&target_module, name) {
                            if let Some(target) = self.program.function(&function) {
                                let signature = marqdo_ir::callable::CallableSignature {
                                    params: target.params.clone(),
                                    kind: target.signature,
                                };
                                self.validate_signature(
                                    &signature,
                                    args,
                                    &format!("{alias}.{name}"),
                                    span,
                                    span,
                                );
                            }
                            self.bind(span, BoundTarget::Marqdo { function });
                        } else {
                            self.diagnostic(
                                codes::SYMBOL_UNKNOWN,
                                span,
                                format!("module `{alias}` has no function `{name}`"),
                                Some(format!(
                                    "check `inspect --signature` on the imported module `{alias}`"
                                )),
                            );
                        }
                        return;
                    }
                    let qualified = format!("{alias}.{name}");
                    if let Some(builtin) = builtins::find(&qualified) {
                        if let Err(message) = builtins::validate_args(builtin, args) {
                            self.diagnostic(
                                codes::CALL_ARITY,
                                span,
                                message,
                                Some("check the builtin parameter list".to_string()),
                            );
                        }
                        self.bind(
                            span,
                            BoundTarget::Builtin {
                                canonical_name: builtin.name.to_string(),
                            },
                        );
                        return;
                    }
                    if let Some(descriptor) = self.registry.external(&qualified) {
                        self.validate_signature(
                            &descriptor.signature,
                            args,
                            &qualified,
                            span,
                            span,
                        );
                        let param_names = descriptor
                            .signature
                            .params
                            .iter()
                            .map(|param| param.name.clone())
                            .collect();
                        let target = match &descriptor.origin {
                            marqdo_ir::callable::CallableOrigin::AbiPlugin { package, export } => {
                                BoundTarget::AbiPlugin {
                                    package: package.clone(),
                                    export: export.clone(),
                                    digest: descriptor
                                        .implementation_digest
                                        .clone()
                                        .unwrap_or_default(),
                                    param_names,
                                }
                            }
                            _ => BoundTarget::DynamicValue,
                        };
                        self.bind(span, target);
                        return;
                    }
                    if self.standalone {
                        // Without a program graph an import or plugin export
                        // cannot be distinguished statically; defer instead of
                        // emitting a false error.
                        return;
                    }
                    self.diagnostic(
                        codes::SYMBOL_UNKNOWN,
                        *alias_span,
                        format!("unknown function or plugin export `{qualified}`"),
                        Some(
                            "declare the export in a plugin manifest and pass `--plugin` for \
                             `run`, or fix the module import alias"
                                .to_string(),
                        ),
                    );
                    return;
                }
                self.bind(span, BoundTarget::DynamicValue);
                self.expr(base);
            }
            other => {
                self.bind(span, BoundTarget::DynamicValue);
                self.expr(other);
            }
        }
    }

    fn validate_signature(
        &mut self,
        signature: &marqdo_ir::callable::CallableSignature,
        args: &[Arg],
        display: &str,
        call_span: Span,
        name_span: Span,
    ) {
        if let Err(message) = signature.validate_args(args, display) {
            // Positional overflow and missing arguments point at the call or
            // the function name respectively; keep messages deterministic.
            let span = if message.contains("missing required") {
                name_span
            } else {
                call_span
            };
            self.diagnostic(
                codes::CALL_ARITY,
                span,
                message,
                Some("check `inspect --signature` or the plugin manifest signature".to_string()),
            );
        }
    }

    fn resolve_module_function(&self, name: &str) -> Resolution {
        if let Some(namespace) = &self.namespace {
            let canonical = format!("{namespace}.{name}");
            if let Some(function) = self
                .module
                .functions
                .iter()
                .find(|function| function.id.name == canonical)
            {
                return Resolution::One(function.id.clone());
            }
        }
        if let Some(function) = self
            .module
            .functions
            .iter()
            .find(|function| function.id.name == name)
        {
            return Resolution::One(function.id.clone());
        }
        let mut matches = self
            .module
            .functions
            .iter()
            .filter(|function| function.local_name == name);
        match (matches.next(), matches.next()) {
            (Some(function), None) => Resolution::One(function.id.clone()),
            (None, _) => Resolution::None,
            (Some(_), Some(_)) => Resolution::Ambiguous,
        }
    }
}

enum Resolution {
    One(FunctionId),
    Ambiguous,
    None,
}

/// Whether a descriptor is a statically declared ABI plugin export.
pub fn is_abi_export(descriptor: &CallableDescriptor) -> bool {
    matches!(
        descriptor.origin,
        marqdo_ir::callable::CallableOrigin::AbiPlugin { .. }
    )
}

/// Whether a function signature requires explicit named/positional checking.
pub fn _signature_kind(signature: &marqdo_ir::callable::CallableSignature) -> SignatureKind {
    signature.kind
}

/// Helper used by tests and by `inspect` to render a function body trace.
#[allow(dead_code)]
fn _unused(_: &Function, _: &Program) {}

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
            implementation_digest: Some("a".repeat(64)),
            source_span: None,
        });
        let registry = CallableRegistry::try_from_program(&program).unwrap();
        let bound = bind_program_calls(&program, &registry);
        assert!(bound
            .diagnostics
            .iter()
            .any(|(_, d)| d.code == codes::CALL_ARITY));
        // A valid positional/named mix binds to the exact export identity.
        let module = make_module(vec![function(
            "main",
            &[],
            SignatureKind::Explicit,
            vec![return_call("solver.add(1, b=2)")],
        )]);
        program.modules.insert(module.id.clone(), module);
        let registry = CallableRegistry::try_from_program(&program).unwrap();
        let bound = bind_program_calls(&program, &registry);
        assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
        let target = bound.bindings.iter().next().unwrap().1;
        match target {
            BoundTarget::AbiPlugin {
                package, export, ..
            } => {
                assert_eq!(package, "demo");
                assert_eq!(export, "solver.add");
            }
            other => panic!("unexpected target {other:?}"),
        }
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

    #[test]
    fn import_alias_conflicting_with_builtin_namespace_is_rejected() {
        let module = Module {
            id: ModuleId::from("index.mq.md"),
            units: vec![],
            functions: vec![function(
                "main",
                &[],
                SignatureKind::Explicit,
                vec![return_call("math.eval(x=1)")],
            )],
            imports: vec![marqdo_ir::ImportDecl {
                alias: "math".into(),
                target: "./math.mq.md".into(),
                span: Span::new(0, 1),
            }],
            resources: vec![],
            anchors: std::collections::BTreeMap::new(),
        };
        let diagnostics = validate_module(&module);
        assert!(diagnostics
            .iter()
            .any(|d| d.code == codes::EXT_IMPORT_NAMESPACE_CONFLICT));
        // Standalone mode defers unresolved imports, but the composition root
        // run is covered by the CLI black-box CALL-01 test.
    }
}
