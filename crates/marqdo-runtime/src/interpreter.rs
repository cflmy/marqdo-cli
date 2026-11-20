//! The tree-walk interpreter.

use std::collections::BTreeMap;

use marqdo_expression::{BinaryOp, Expr, RecordEntry};
use marqdo_host::{ExecutionContext, Host, HostOp};
use marqdo_ir::{builtins, CallTarget, Function, FunctionId, ModuleId, Program, Stmt};
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError, Value};

use crate::builtins_impl::FrameView;

use crate::value_ops;

/// One already-evaluated call argument.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedArg {
    /// Argument name for named arguments; `None` for positional.
    pub name: Option<String>,
    /// Evaluated value.
    pub value: Value,
    /// Source span of the argument.
    pub span: Span,
}

impl EvaluatedArg {
    /// Named argument constructor.
    pub fn named(name: impl Into<String>, value: Value, span: Span) -> Self {
        Self {
            name: Some(name.into()),
            value,
            span,
        }
    }

    /// Positional argument constructor.
    pub fn positional(value: Value, span: Span) -> Self {
        Self {
            name: None,
            value,
            span,
        }
    }
}

/// Successful run result.
#[derive(Debug, Clone, PartialEq)]
pub struct RunOutcome {
    /// Value returned by the entry function (`Unit` when it falls through).
    pub value: Value,
}

/// Execution frame for one function call.
struct Frame {
    function: FunctionId,
    namespace: Option<String>,
    locals: BTreeMap<String, Value>,
}

/// Flow of one statement list.
enum Flow {
    Next,
    Return(Value),
}

/// Marqdo tree-walk interpreter.
pub struct Interpreter<'a> {
    program: &'a Program,
    host: &'a mut dyn Host,
    depth: usize,
}

/// Run a named function with already-evaluated arguments.
pub fn run(
    program: &Program,
    host: &mut dyn Host,
    module: &ModuleId,
    function: &str,
    args: Vec<EvaluatedArg>,
) -> Result<RunOutcome, RuntimeError> {
    let mut interpreter = Interpreter {
        program,
        host,
        depth: 0,
    };
    let value = interpreter.call_by_name(module, function, args, None)?;
    Ok(RunOutcome { value })
}

impl<'a> Interpreter<'a> {
    /// Call a function in a module by canonical or unique local name.
    pub fn call_by_name(
        &mut self,
        module: &ModuleId,
        name: &str,
        args: Vec<EvaluatedArg>,
        call_span: Option<Span>,
    ) -> Result<Value, RuntimeError> {
        let program = self.program;
        let Some(target_module) = program.module(module) else {
            return Err(RuntimeError::new(
                ErrorCategory::UnknownSymbol,
                call_span,
                format!("unknown module `{module}`"),
            ));
        };
        let Some(function) = target_module.function(name) else {
            return Err(RuntimeError::new(
                ErrorCategory::UnknownSymbol,
                call_span,
                format!("unknown function `{name}` in module `{module}`"),
            ));
        };
        self.call_function(program, function, args, call_span)
    }

    fn call_function(
        &mut self,
        program: &'a Program,
        function: &'a Function,
        args: Vec<EvaluatedArg>,
        call_span: Option<Span>,
    ) -> Result<Value, RuntimeError> {
        let display = function.id.name.clone();
        if self.depth + 1 > 0 {
            self.host.check_stack(self.depth + 1, call_span)?;
        }
        let locals = self.bind_args(function, args, call_span)?;
        let mut frame = Frame {
            function: function.id.clone(),
            namespace: function.namespace.clone(),
            locals,
        };
        self.depth += 1;
        let result = self.exec_stmts(program, &function.body, &mut frame);
        self.depth -= 1;
        match result {
            Ok(Flow::Next) => Ok(Value::Unit),
            Ok(Flow::Return(value)) => Ok(value),
            Err(mut error) => {
                error.push_frame(display, call_span);
                Err(error)
            }
        }
    }

    fn bind_args(
        &mut self,
        function: &Function,
        args: Vec<EvaluatedArg>,
        call_span: Option<Span>,
    ) -> Result<BTreeMap<String, Value>, RuntimeError> {
        let mut provided: BTreeMap<String, Value> = BTreeMap::new();
        let mut positional = 0usize;
        for arg in args {
            match &arg.name {
                Some(name) => {
                    if !function.params.iter().any(|param| &param.name == name) {
                        return Err(RuntimeError::new(
                            ErrorCategory::Arity,
                            Some(arg.span),
                            format!("`{}` has no parameter named `{name}`", function.id.name),
                        ));
                    }
                    if provided.insert(name.clone(), arg.value).is_some() {
                        return Err(RuntimeError::new(
                            ErrorCategory::Arity,
                            Some(arg.span),
                            format!("duplicate argument `{name}`"),
                        ));
                    }
                }
                None => {
                    if function.signature != marqdo_ir::SignatureKind::Explicit {
                        return Err(RuntimeError::new(
                            ErrorCategory::Arity,
                            Some(arg.span),
                            format!(
                                "`{}` has an inferred signature and only accepts named arguments",
                                function.id.name
                            ),
                        ));
                    }
                    let Some(param) = function.params.get(positional) else {
                        return Err(RuntimeError::new(
                            ErrorCategory::Arity,
                            Some(arg.span),
                            format!(
                                "`{}` accepts at most {} positional argument(s)",
                                function.id.name,
                                function.params.len()
                            ),
                        ));
                    };
                    provided.insert(param.name.clone(), arg.value);
                    positional += 1;
                }
            }
        }
        let mut locals = BTreeMap::new();
        for param in &function.params {
            match provided.remove(&param.name) {
                Some(value) => {
                    locals.insert(param.name.clone(), value);
                }
                None => {
                    if let Some(default) = &param.default {
                        // Defaults are compile-time constants; evaluate them in
                        // an empty frame so they cannot reach caller state.
                        let mut empty = Frame {
                            function: function.id.clone(),
                            namespace: function.namespace.clone(),
                            locals: BTreeMap::new(),
                        };
                        let value = self.eval_expr(default, &mut empty)?;
                        locals.insert(param.name.clone(), value);
                    } else {
                        return Err(RuntimeError::new(
                            ErrorCategory::Arity,
                            call_span,
                            format!(
                                "`{}` is missing required argument `{}`",
                                function.id.name, param.name
                            ),
                        ));
                    }
                }
            }
        }
        Ok(locals)
    }

    fn exec_stmts(
        &mut self,
        program: &'a Program,
        stmts: &'a [Stmt],
        frame: &mut Frame,
    ) -> Result<Flow, RuntimeError> {
        for stmt in stmts {
            self.host.consume_fuel(1, Some(stmt.span()))?;
            self.host.check_deadline(Some(stmt.span()))?;
            match stmt {
                Stmt::Assign { name, value, .. } => {
                    let value = self.eval_expr(value, frame)?;
                    frame.locals.insert(name.clone(), value);
                }
                Stmt::Eval { expr, .. } => {
                    self.eval_expr(expr, frame)?;
                }
                Stmt::Call { call, span } => {
                    self.exec_call_site(program, call, *span, frame)?;
                }
                Stmt::Return { value, .. } => {
                    let value = self.eval_expr(value, frame)?;
                    return Ok(Flow::Return(value));
                }
                Stmt::Branch { arms, .. } => {
                    for arm in arms {
                        self.host.consume_fuel(1, Some(arm.condition_span))?;
                        let condition = self.eval_expr(&arm.condition, frame)?;
                        let Value::Bool(condition) = condition else {
                            return Err(RuntimeError::type_error(
                                Some(arm.condition_span),
                                format!(
                                    "branch condition must be Bool, got {}",
                                    condition.type_name()
                                ),
                            ));
                        };
                        if condition {
                            match self.exec_stmts(program, &arm.body, frame)? {
                                Flow::Next => break,
                                Flow::Return(value) => return Ok(Flow::Return(value)),
                            }
                        }
                    }
                }
                Stmt::Loop {
                    condition,
                    condition_span,
                    body,
                    span,
                } => loop {
                    self.host.consume_fuel(1, Some(*span))?;
                    self.host.check_deadline(Some(*condition_span))?;
                    let value = self.eval_expr(condition, frame)?;
                    let Value::Bool(value) = value else {
                        return Err(RuntimeError::type_error(
                            Some(*condition_span),
                            format!("loop condition must be Bool, got {}", value.type_name()),
                        ));
                    };
                    if !value {
                        break;
                    }
                    match self.exec_stmts(program, body, frame)? {
                        Flow::Next => {}
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                    }
                },
            }
        }
        Ok(Flow::Next)
    }

    fn exec_call_site(
        &mut self,
        program: &'a Program,
        call: &'a marqdo_ir::CallSite,
        span: Span,
        frame: &mut Frame,
    ) -> Result<Value, RuntimeError> {
        let args = self.eval_args(&call.args, frame)?;
        let (module, name) = match &call.target {
            CallTarget::Function { name } => (frame.function.module.clone(), name.clone()),
            CallTarget::Qualified {
                module_alias,
                function,
            } => {
                let Some(target) = program.resolve_import(&frame.function.module, module_alias)
                else {
                    return Err(RuntimeError::new(
                        ErrorCategory::UnknownSymbol,
                        Some(span),
                        format!("unknown imported module alias `{module_alias}`"),
                    ));
                };
                (target.clone(), function.clone())
            }
            CallTarget::Resolved { module, function } => (module.clone(), function.clone()),
        };
        self.call_by_name(&module, &name, args, Some(span))
    }

    fn eval_args(
        &mut self,
        args: &[marqdo_expression::Arg],
        frame: &mut Frame,
    ) -> Result<Vec<EvaluatedArg>, RuntimeError> {
        let mut evaluated = Vec::with_capacity(args.len());
        for arg in args {
            let value = self.eval_expr(&arg.value, frame)?;
            evaluated.push(EvaluatedArg {
                name: arg.name.clone(),
                value,
                span: arg.span,
            });
        }
        Ok(evaluated)
    }

    fn eval_expr(&mut self, expr: &Expr, frame: &mut Frame) -> Result<Value, RuntimeError> {
        self.host.consume_fuel(1, Some(expr.span()))?;
        match expr {
            Expr::Int(value) => Ok(Value::Int(value.clone())),
            Expr::Float(value) => Ok(Value::Float(*value)),
            Expr::Text(value) => Ok(Value::Text(value.clone())),
            Expr::Bool(value) => Ok(Value::Bool(*value)),
            Expr::None => Ok(Value::None),
            Expr::Ident(ident) => frame.locals.get(&ident.name).cloned().ok_or_else(|| {
                RuntimeError::new(
                    ErrorCategory::UnknownSymbol,
                    Some(ident.span),
                    format!("unknown variable `{}`", ident.name),
                )
            }),
            Expr::List(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval_expr(item, frame)?);
                }
                Ok(Value::List(values))
            }
            Expr::Record(entries) => {
                let mut map = BTreeMap::new();
                for RecordEntry {
                    key,
                    key_span,
                    value,
                } in entries
                {
                    let value = self.eval_expr(value, frame)?;
                    if map.insert(key.clone(), value).is_some() {
                        return Err(RuntimeError::new(
                            ErrorCategory::Type,
                            Some(*key_span),
                            format!("duplicate record key `{key}`"),
                        ));
                    }
                }
                Ok(Value::Map(map))
            }
            Expr::Unary { op, operand, span } => {
                let value = self.eval_expr(operand, frame)?;
                value_ops::unary(*op, value, Some(*span))
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let left_value = self.eval_expr(left, frame)?;
                    let Value::Bool(left_bool) = left_value else {
                        return Err(RuntimeError::type_error(
                            Some(*span),
                            format!(
                                "`{}` expects Bool, got {}",
                                if *op == BinaryOp::And { "and" } else { "or" },
                                left_value.type_name()
                            ),
                        ));
                    };
                    if (*op == BinaryOp::And && !left_bool) || (*op == BinaryOp::Or && left_bool) {
                        return Ok(Value::Bool(left_bool));
                    }
                    let right_value = self.eval_expr(right, frame)?;
                    let Value::Bool(right_bool) = right_value else {
                        return Err(RuntimeError::type_error(
                            Some(*span),
                            format!(
                                "`{}` expects Bool, got {}",
                                if *op == BinaryOp::And { "and" } else { "or" },
                                right_value.type_name()
                            ),
                        ));
                    };
                    return Ok(Value::Bool(right_bool));
                }
                let left_value = self.eval_expr(left, frame)?;
                let right_value = self.eval_expr(right, frame)?;
                value_ops::binary(*op, left_value, right_value, Some(*span))
            }
            Expr::Call { callee, args, span } => {
                let program = self.program;
                self.eval_call(program, callee, args, *span, frame)
            }
            Expr::Member { base, name, span } => {
                let value = self.eval_expr(base, frame)?;
                member_of(value, name, Some(*span))
            }
            Expr::Index { base, index, span } => {
                let base_value = self.eval_expr(base, frame)?;
                let index_value = self.eval_expr(index, frame)?;
                index_value_of(base_value, index_value, Some(*span))
            }
        }
    }

    fn eval_call(
        &mut self,
        program: &'a Program,
        callee: &Expr,
        args: &[marqdo_expression::Arg],
        span: Span,
        frame: &mut Frame,
    ) -> Result<Value, RuntimeError> {
        match callee {
            Expr::Ident(ident) => {
                if let Some(value) = frame.locals.get(&ident.name).cloned() {
                    let args = self.eval_args(args, frame)?;
                    return self.call_value(program, value, args, Some(span));
                }
                if let Some(builtin) = builtins::find(&ident.name) {
                    let args = self.eval_args(args, frame)?;
                    return self.call_builtin(builtin, args, Some(span), frame);
                }
                let args = self.eval_args(args, frame)?;
                let (module, namespace) = (frame.function.module.clone(), frame.namespace.clone());
                let Some(function) =
                    resolve_function(program, &module, namespace.as_deref(), &ident.name)
                else {
                    return Err(RuntimeError::new(
                        ErrorCategory::UnknownSymbol,
                        Some(span),
                        format!("unknown function `{}`", ident.name),
                    ));
                };
                self.call_function(program, function, args, Some(span))
            }
            Expr::Member { base, name, .. } => {
                if let Expr::Ident(alias) = base.as_ref() {
                    if let Some(target_module) =
                        program.resolve_import(&frame.function.module, &alias.name)
                    {
                        let args = self.eval_args(args, frame)?;
                        let Some(function) = resolve_function(program, target_module, None, name)
                        else {
                            return Err(RuntimeError::new(
                                ErrorCategory::UnknownSymbol,
                                Some(span),
                                format!("unknown function `{name}` in module `{target_module}`"),
                            ));
                        };
                        return self.call_function(program, function, args, Some(span));
                    }
                    let qualified = format!("{}.{name}", alias.name);
                    if let Some(builtin) = builtins::find(&qualified) {
                        let args = self.eval_args(args, frame)?;
                        return self.call_builtin(builtin, args, Some(span), frame);
                    }
                    if !frame.locals.contains_key(&alias.name) {
                        // Not a local value or imported module: try an explicitly
                        // trusted plugin namespace. Plugin calls take named
                        // arguments only in this build.
                        let evaluated = self.eval_args(args, frame)?;
                        let mut named = std::collections::BTreeMap::new();
                        for argument in evaluated {
                            let Some(argument_name) = argument.name else {
                                return Err(RuntimeError::new(
                                    ErrorCategory::Type,
                                    Some(argument.span),
                                    "plugin calls require named arguments",
                                ));
                            };
                            named.insert(argument_name, argument.value);
                        }
                        let module = frame.function.module.clone();
                        return self.host_invoke(
                            HostOp::PluginInvoke,
                            vec![
                                Value::Text(alias.name.clone()),
                                Value::Text(name.clone()),
                                Value::Map(named),
                            ],
                            Some(span),
                            &module,
                        );
                    }
                }
                let callee_value = self.eval_expr(callee, frame)?;
                let args = self.eval_args(args, frame)?;
                self.call_value(program, callee_value, args, Some(span))
            }
            other => {
                let callee_value = self.eval_expr(other, frame)?;
                let args = self.eval_args(args, frame)?;
                self.call_value(program, callee_value, args, Some(span))
            }
        }
    }

    fn call_value(
        &mut self,
        program: &'a Program,
        callee: Value,
        args: Vec<EvaluatedArg>,
        span: Option<Span>,
    ) -> Result<Value, RuntimeError> {
        match callee {
            Value::FunctionRef(reference) => {
                let id = FunctionId {
                    module: ModuleId(reference.module),
                    name: reference.name,
                };
                let Some(function) = program.function(&id) else {
                    return Err(RuntimeError::new(
                        ErrorCategory::UnknownSymbol,
                        span,
                        format!("function reference `{id}` does not exist"),
                    ));
                };
                self.call_function(program, function, args, span)
            }
            other => Err(RuntimeError::new(
                ErrorCategory::NotCallable,
                span,
                format!("{} is not callable", other.type_name()),
            )),
        }
    }

    fn call_builtin(
        &mut self,
        builtin: &'static builtins::Builtin,
        args: Vec<EvaluatedArg>,
        span: Option<Span>,
        frame: &Frame,
    ) -> Result<Value, RuntimeError> {
        let bound = bind_builtin_args(builtin, args, span)?;
        let view = FrameView::new(&frame.function.module);
        crate::builtins_impl::dispatch(self, builtin, bound, span, &view)
    }

    /// Access to the program (used by builtin implementations).
    pub(crate) fn program(&self) -> &'a Program {
        self.program
    }

    /// Call a builtin-read host operation on behalf of a frame's module.
    pub(crate) fn host_invoke(
        &mut self,
        op: HostOp,
        args: Vec<Value>,
        span: Option<Span>,
        module: &ModuleId,
    ) -> Result<Value, RuntimeError> {
        let context = ExecutionContext::new(module.0.clone(), span);
        self.host.invoke(op, args, &context)
    }
}

fn resolve_function<'p>(
    program: &'p Program,
    module: &ModuleId,
    namespace: Option<&str>,
    name: &str,
) -> Option<&'p Function> {
    let module = program.module(module)?;
    if let Some(namespace) = namespace {
        if let Some(found) = module.namespaced_function(namespace, name) {
            return Some(found);
        }
    }
    module.function(name)
}

/// Bind already-evaluated arguments to a builtin parameter list.
fn bind_builtin_args(
    builtin: &'static builtins::Builtin,
    args: Vec<EvaluatedArg>,
    span: Option<Span>,
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    let mut provided = BTreeMap::new();
    let mut positional = 0usize;
    for arg in args {
        match &arg.name {
            Some(name) => {
                if !builtin.params.iter().any(|param| param.name == name) {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        Some(arg.span),
                        format!("`{}` has no parameter named `{name}`", builtin.name),
                    ));
                }
                provided.insert(name.clone(), arg.value);
            }
            None => {
                let Some(param) = builtin.params.get(positional) else {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        Some(arg.span),
                        format!(
                            "`{}` accepts at most {} argument(s)",
                            builtin.name,
                            builtin.params.len()
                        ),
                    ));
                };
                provided.insert(param.name.to_string(), arg.value);
                positional += 1;
            }
        }
    }
    for param in builtin.params {
        if param.required && !provided.contains_key(param.name) {
            return Err(RuntimeError::new(
                ErrorCategory::Arity,
                span,
                format!(
                    "`{}` is missing required argument `{}`",
                    builtin.name, param.name
                ),
            ));
        }
    }
    Ok(provided)
}

fn member_of(value: Value, name: &str, span: Option<Span>) -> Result<Value, RuntimeError> {
    match value {
        Value::Map(map) => map.get(name).cloned().ok_or_else(|| {
            RuntimeError::new(
                ErrorCategory::UnknownField,
                span,
                format!("map has no field `{name}`"),
            )
        }),
        Value::Formula(formula) => match name {
            "latex" => Ok(Value::Text(formula.latex)),
            "display" => Ok(Value::Bool(formula.display)),
            "unit" => Ok(match formula.unit {
                Some(unit) => Value::Text(unit),
                None => Value::None,
            }),
            _ => Err(RuntimeError::new(
                ErrorCategory::UnknownField,
                span,
                format!("Formula has no field `{name}`"),
            )),
        },
        Value::Code(code) => match name {
            "language" => Ok(Value::Text(code.language)),
            "source" => Ok(Value::Text(code.source)),
            "content_hash" => Ok(Value::Text(code.content_hash)),
            "unit" => Ok(match code.unit {
                Some(unit) => Value::Text(unit),
                None => Value::None,
            }),
            _ => Err(RuntimeError::new(
                ErrorCategory::UnknownField,
                span,
                format!("Code has no field `{name}`"),
            )),
        },
        other => Err(RuntimeError::new(
            ErrorCategory::UnknownField,
            span,
            format!("{} has no fields", other.type_name()),
        )),
    }
}

fn index_value_of(base: Value, index: Value, span: Option<Span>) -> Result<Value, RuntimeError> {
    match (base, index) {
        (Value::List(items), Value::Int(index)) => {
            let len = items.len() as i64;
            let Some(position) = resolve_index(&index, len) else {
                return Err(RuntimeError::new(
                    ErrorCategory::IndexOutOfBounds,
                    span,
                    format!("list index {index} is out of bounds for length {len}"),
                ));
            };
            Ok(items[position].clone())
        }
        (Value::Text(text), Value::Int(index)) => {
            let chars: Vec<char> = text.chars().collect();
            let len = chars.len() as i64;
            let Some(position) = resolve_index(&index, len) else {
                return Err(RuntimeError::new(
                    ErrorCategory::IndexOutOfBounds,
                    span,
                    format!("text index {index} is out of bounds for length {len}"),
                ));
            };
            Ok(Value::Text(chars[position].to_string()))
        }
        (Value::Map(map), Value::Text(key)) => map.get(&key).cloned().ok_or_else(|| {
            RuntimeError::new(
                ErrorCategory::UnknownField,
                span,
                format!("map has no key `{key}`"),
            )
        }),
        (base, index) => Err(RuntimeError::new(
            ErrorCategory::Type,
            span,
            format!(
                "cannot index {} with {}",
                base.type_name(),
                index.type_name()
            ),
        )),
    }
}

fn resolve_index(index: &num_bigint::BigInt, len: i64) -> Option<usize> {
    use num_traits::ToPrimitive;
    let index = index.to_i64()?;
    let position = if index < 0 { len + index } else { index };
    if position < 0 || position >= len {
        None
    } else {
        Some(position as usize)
    }
}
