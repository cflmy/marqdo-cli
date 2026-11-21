//! The tree-walk interpreter.

use std::collections::BTreeMap;

use marqdo_expression::{BinaryOp, Expr, RecordEntry};
use marqdo_host::{ExecutionContext, Host, HostOp, ResourceRead};
use marqdo_ir::{builtins, Function, FunctionId, ModuleId, PlaceTarget, Program, Stmt};
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, ResourceHandle, RuntimeError, Value};

use crate::builtins_impl::FrameView;
use crate::memory::{MemoryLimits, MemoryStore};

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
    memory: MemoryStore,
}

/// Run a named function with already-evaluated arguments.
pub fn run(
    program: &Program,
    host: &mut dyn Host,
    module: &ModuleId,
    function: &str,
    args: Vec<EvaluatedArg>,
) -> Result<RunOutcome, RuntimeError> {
    run_with_limits(
        program,
        host,
        module,
        function,
        args,
        MemoryLimits::default(),
    )
}

/// Run with explicit addressable-memory limits (ADR-0022).
pub fn run_with_limits(
    program: &Program,
    host: &mut dyn Host,
    module: &ModuleId,
    function: &str,
    args: Vec<EvaluatedArg>,
    limits: MemoryLimits,
) -> Result<RunOutcome, RuntimeError> {
    let mut interpreter = Interpreter {
        program,
        host,
        depth: 0,
        memory: MemoryStore::new(limits),
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
        self.call_by_name_from(module, name, args, call_span, None)
    }

    fn call_by_name_from(
        &mut self,
        module: &ModuleId,
        name: &str,
        args: Vec<EvaluatedArg>,
        call_span: Option<Span>,
        caller_module: Option<&ModuleId>,
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
        self.call_function(program, function, args, call_span, caller_module)
    }

    fn call_function(
        &mut self,
        program: &'a Program,
        function: &'a Function,
        args: Vec<EvaluatedArg>,
        call_span: Option<Span>,
        caller_module: Option<&ModuleId>,
    ) -> Result<Value, RuntimeError> {
        let display = function.id.name.clone();
        let trace_id = function.id.to_string();
        let trace_module = caller_module.unwrap_or(&function.id.module).canonical();
        if self.depth + 1 > 0 {
            self.host.check_stack(self.depth + 1, call_span)?;
        }
        self.host.record_semantic(
            "call.enter",
            Some(&trace_id),
            Some(&trace_module),
            call_span,
            None,
        );
        let locals = match self.bind_args(function, args, call_span) {
            Ok(locals) => locals,
            Err(mut error) => {
                if let Some(module) = caller_module {
                    error.attribute_module(module.canonical());
                }
                return Err(error);
            }
        };
        let mut frame = Frame {
            function: function.id.clone(),
            locals,
        };
        self.depth += 1;
        let result = self.exec_stmts(program, &function.body, &mut frame);
        self.depth -= 1;
        let status = match &result {
            Ok(Flow::Next) => "ok",
            Ok(Flow::Return(_)) => "return",
            Err(error) => error.code(),
        };
        self.host.record_semantic(
            "call.exit",
            Some(&trace_id),
            Some(&trace_module),
            call_span,
            Some(status),
        );
        match result {
            Ok(Flow::Next) => Ok(Value::Unit),
            Ok(Flow::Return(value)) => Ok(value),
            Err(mut error) => {
                error.attribute_module(function.id.module.canonical());
                error.push_frame(
                    display,
                    call_span,
                    caller_module.map(|module| module.canonical()),
                );
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
        let module = frame.function.module.clone();
        self.exec_stmts_inner(program, stmts, frame)
            .map_err(|mut error| {
                error.attribute_module(module.canonical());
                error
            })
    }

    fn exec_stmts_inner(
        &mut self,
        program: &'a Program,
        stmts: &'a [Stmt],
        frame: &mut Frame,
    ) -> Result<Flow, RuntimeError> {
        for stmt in stmts {
            self.host.consume_fuel(1, Some(stmt.span()))?;
            self.host.check_deadline(Some(stmt.span()))?;
            match stmt {
                Stmt::Assign { target, value, .. } => match target {
                    PlaceTarget::Local { name, .. } => {
                        let value = self.eval_expr(value, frame)?;
                        frame.locals.insert(name.clone(), value);
                    }
                    PlaceTarget::Index { base, index, span } => {
                        // Frozen order (ADR-0022): base, index, then RHS.
                        let base_value = self.eval_expr(base, frame)?;
                        let index_value = self.eval_expr(index, frame)?;
                        let byte_value = self.eval_expr(value, frame)?;
                        self.assign_memory_index(
                            base_value,
                            index_value,
                            byte_value,
                            Some(*span),
                            frame,
                        )?;
                    }
                },
                Stmt::Eval { expr, .. } => {
                    self.eval_expr(expr, frame)?;
                }
                Stmt::Return { value, span } => {
                    let value = self.eval_expr(value, frame)?;
                    let module = frame.function.module.canonical();
                    self.host.record_semantic(
                        "return",
                        Some(&frame.function.to_string()),
                        Some(&module),
                        Some(*span),
                        Some(value.type_name()),
                    );
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
                        let module = frame.function.module.canonical();
                        self.host.record_semantic(
                            "branch.eval",
                            Some(&frame.function.to_string()),
                            Some(&module),
                            Some(arm.condition_span),
                            Some(if condition { "true" } else { "false" }),
                        );
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
                    let module = frame.function.module.canonical();
                    self.host.record_semantic(
                        "loop.iteration",
                        Some(&frame.function.to_string()),
                        Some(&module),
                        Some(*condition_span),
                        Some(if value { "continue" } else { "exit" }),
                    );
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

    /// Commit one byte into an addressable region after all operand checks.
    ///
    /// The three operands are already evaluated, so a rejected value never
    /// produces a partial write (ADR-0022 §3).
    fn assign_memory_index(
        &mut self,
        base: Value,
        index: Value,
        byte: Value,
        span: Option<Span>,
        frame: &Frame,
    ) -> Result<(), RuntimeError> {
        let base_type = base.type_name();
        let Value::Memory(handle) = base else {
            return Err(RuntimeError::new(
                ErrorCategory::Type,
                span,
                format!(
                    "index assignment requires a Memory base in v1, got {base_type}; List, Map \
                     and Table values keep their existing value semantics"
                ),
            ));
        };
        let index_type = index.type_name();
        let Value::Int(offset) = index else {
            return Err(RuntimeError::type_error(
                span,
                format!("Memory index must be an Int, got {index_type}"),
            ));
        };
        let byte_type = byte.type_name();
        let byte_repr = byte.display();
        let Value::Int(byte) = byte else {
            return Err(RuntimeError::type_error(
                span,
                format!("Memory byte value must be an Int in 0..255, got {byte_type}"),
            ));
        };
        let Some(byte) = num_traits::ToPrimitive::to_u8(&byte) else {
            return Err(RuntimeError::type_error(
                span,
                format!("Memory byte value {byte_repr} is outside 0..255"),
            ));
        };
        self.memory.write(handle, &offset, byte, span)?;
        let module = frame.function.module.canonical();
        self.host.record_semantic(
            "memory.write",
            Some(&frame.function.to_string()),
            Some(&module),
            span,
            Some("byte"),
        );
        Ok(())
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
                match base_value {
                    Value::Memory(handle) => {
                        let Value::Int(offset) = index_value else {
                            return Err(RuntimeError::type_error(
                                Some(*span),
                                format!(
                                    "Memory index must be an Int, got {}",
                                    index_value.type_name()
                                ),
                            ));
                        };
                        let byte = self.memory.read(handle, &offset, Some(*span))?;
                        Ok(Value::int(i64::from(byte)))
                    }
                    other => index_value_of(other, index_value, Some(*span)),
                }
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
        // ADR-0054 / RFC-EXT-0001 §4: the frontend resolved this call site
        // exactly once. Dispatch by the recorded identity; never re-resolve a
        // name, never infer a plugin from a dotted prefix.
        let key = marqdo_ir::call_binding::CallSiteKey::new(frame.function.clone(), span);
        let Some(target) = program.call_bindings.get(&key).cloned() else {
            return Err(RuntimeError::new(
                ErrorCategory::Internal,
                Some(span),
                format!(
                    "call site `{key}` has no static binding; run a prepared program (ADR-0054)"
                ),
            ));
        };
        match target {
            marqdo_ir::call_binding::BoundTarget::Builtin { canonical_name } => {
                let Some(builtin) = builtins::find(&canonical_name) else {
                    return Err(RuntimeError::new(
                        ErrorCategory::Internal,
                        Some(span),
                        format!("bound builtin `{canonical_name}` is not registered"),
                    ));
                };
                let args = self.eval_args(args, frame)?;
                self.call_builtin(builtin, args, Some(span), frame)
            }
            marqdo_ir::call_binding::BoundTarget::Marqdo { function } => {
                let Some(target_function) = program.function(&function) else {
                    return Err(RuntimeError::new(
                        ErrorCategory::Internal,
                        Some(span),
                        format!("bound function `{function}` does not exist"),
                    ));
                };
                let args = self.eval_args(args, frame)?;
                self.call_function(
                    program,
                    target_function,
                    args,
                    Some(span),
                    Some(&frame.function.module.clone()),
                )
            }
            marqdo_ir::call_binding::BoundTarget::AbiPlugin {
                package: _,
                export,
                digest: _,
                param_names,
            } => {
                // ABI v1 transports named arguments as a map. Normalise
                // positional arguments deterministically against the static
                // manifest order; static validation already rejected unknown,
                // duplicate or missing arguments.
                let evaluated = self.eval_args(args, frame)?;
                let mut named = std::collections::BTreeMap::new();
                let mut position = 0usize;
                for argument in evaluated {
                    let name = match argument.name {
                        Some(name) => name,
                        None => {
                            let Some(name) = param_names.get(position).cloned() else {
                                return Err(RuntimeError::new(
                                    ErrorCategory::Internal,
                                    Some(span),
                                    format!(
                                        "positional argument {} was not normalised for `{export}`",
                                        position + 1
                                    ),
                                ));
                            };
                            position += 1;
                            name
                        }
                    };
                    if named.insert(name.clone(), argument.value).is_some() {
                        return Err(RuntimeError::new(
                            ErrorCategory::Arity,
                            Some(span),
                            format!("duplicate argument `{name}` for `{export}`"),
                        ));
                    }
                }
                let module = frame.function.module.clone();
                self.host_invoke(
                    HostOp::PluginInvoke,
                    vec![Value::Text(export), Value::Map(named)],
                    Some(span),
                    &module,
                )
            }
            marqdo_ir::call_binding::BoundTarget::DynamicValue => {
                let callee_value = self.eval_expr(callee, frame)?;
                let args = self.eval_args(args, frame)?;
                self.call_value(
                    program,
                    callee_value,
                    args,
                    Some(span),
                    Some(&frame.function.module.clone()),
                )
            }
        }
    }

    fn call_value(
        &mut self,
        program: &'a Program,
        callee: Value,
        args: Vec<EvaluatedArg>,
        span: Option<Span>,
        caller_module: Option<&ModuleId>,
    ) -> Result<Value, RuntimeError> {
        match callee {
            Value::FunctionRef(reference) => {
                let id = FunctionId {
                    module: ModuleId::from(reference.module),
                    name: reference.name,
                };
                let Some(function) = program.function(&id) else {
                    return Err(RuntimeError::new(
                        ErrorCategory::UnknownSymbol,
                        span,
                        format!("function reference `{id}` does not exist"),
                    ));
                };
                self.call_function(program, function, args, span, caller_module)
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

    /// Addressable-memory store for builtins and place writes.
    pub(crate) fn memory(&self) -> &MemoryStore {
        &self.memory
    }

    /// Mutable addressable-memory store for `memory.new`/`memory.grow`.
    pub(crate) fn memory_mut(&mut self) -> &mut MemoryStore {
        &mut self.memory
    }

    /// Record a semantic event for a builtin that is not a host operation.
    pub(crate) fn record_memory_event(
        &mut self,
        kind: &str,
        module: &ModuleId,
        span: Option<Span>,
        detail: &str,
    ) {
        let module = module.canonical();
        self.host
            .record_semantic(kind, None, Some(&module), span, Some(detail));
    }

    /// Open a resource/stream through the host boundary.
    pub(crate) fn resource_open(
        &mut self,
        kind: &str,
        descriptor: &str,
        rights: &[String],
        module: &ModuleId,
        span: Option<Span>,
    ) -> Result<Value, RuntimeError> {
        let context = ExecutionContext::new(module.to_string(), span);
        self.host
            .resource_open(kind, descriptor, rights, &context)
            .map_err(|mut error| {
                error.mark_callsite();
                error
            })
    }

    /// Read from a resource stream.
    pub(crate) fn resource_read(
        &mut self,
        handle: ResourceHandle,
        max_bytes: usize,
        module: &ModuleId,
        span: Option<Span>,
    ) -> Result<ResourceRead, RuntimeError> {
        let context = ExecutionContext::new(module.to_string(), span);
        self.host
            .resource_read(handle, max_bytes, &context)
            .map_err(|mut error| {
                error.mark_callsite();
                error
            })
    }

    /// Write bytes to a resource stream.
    pub(crate) fn resource_write(
        &mut self,
        handle: ResourceHandle,
        bytes: &[u8],
        module: &ModuleId,
        span: Option<Span>,
    ) -> Result<usize, RuntimeError> {
        let context = ExecutionContext::new(module.to_string(), span);
        self.host
            .resource_write(handle, bytes, &context)
            .map_err(|mut error| {
                error.mark_callsite();
                error
            })
    }

    /// Accept one inbound TCP connection from a listener handle.
    pub(crate) fn resource_accept(
        &mut self,
        handle: ResourceHandle,
        module: &ModuleId,
        span: Option<Span>,
    ) -> Result<Value, RuntimeError> {
        let context = ExecutionContext::new(module.to_string(), span);
        self.host
            .resource_accept(handle, &context)
            .map_err(|mut error| {
                error.mark_callsite();
                error
            })
    }

    /// Execute one non-streaming resource operation.
    pub(crate) fn resource_request(
        &mut self,
        operation: &str,
        args: Vec<Value>,
        module: &ModuleId,
        span: Option<Span>,
    ) -> Result<Value, RuntimeError> {
        let context = ExecutionContext::new(module.to_string(), span);
        self.host
            .resource_request(operation, args, &context)
            .map_err(|mut error| {
                error.mark_callsite();
                error
            })
    }

    /// Close a resource stream.
    pub(crate) fn resource_close(
        &mut self,
        handle: ResourceHandle,
        module: &ModuleId,
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        let context = ExecutionContext::new(module.to_string(), span);
        self.host
            .resource_close(handle, &context)
            .map_err(|mut error| {
                error.mark_callsite();
                error
            })
    }

    /// Call a builtin-read host operation on behalf of a frame's module.
    pub(crate) fn host_invoke(
        &mut self,
        op: HostOp,
        args: Vec<Value>,
        span: Option<Span>,
        module: &ModuleId,
    ) -> Result<Value, RuntimeError> {
        let context = ExecutionContext::new(module.to_string(), span);
        self.host.invoke(op, args, &context).map_err(|mut error| {
            error.mark_callsite();
            error
        })
    }
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
        Value::FunctionRef(reference) => match name {
            "module" => Ok(Value::Text(reference.module)),
            "name" => Ok(Value::Text(reference.name)),
            _ => Err(RuntimeError::new(
                ErrorCategory::UnknownField,
                span,
                format!("FunctionRef has no field `{name}`"),
            )),
        },
        Value::PluginHandle(handle) => match name {
            "plugin" => Ok(Value::Text(handle.plugin)),
            "export" => Ok(Value::Text(handle.export)),
            _ => Err(RuntimeError::new(
                ErrorCategory::UnknownField,
                span,
                format!("PluginHandle has no field `{name}`"),
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
