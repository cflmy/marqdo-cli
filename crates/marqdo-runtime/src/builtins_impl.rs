//! Implementations of the v1 builtins.
//!
//! Pure builtins run in the interpreter; effectful builtins and kernels go
//! through the capability host. `ref` resolves inert document resources and
//! never evaluates them.

use std::collections::BTreeMap;

use marqdo_host::HostOp;
use marqdo_ir::builtins::Builtin;
use marqdo_ir::Resource;
use marqdo_span::Span;
use marqdo_value::{
    float_to_bigint, CodeValue, ErrorCategory, FormulaValue, FunctionRef, RuntimeError, Table,
    Value,
};

use crate::interpreter::Interpreter;

pub(crate) fn dispatch(
    interpreter: &mut Interpreter<'_>,
    builtin: &'static Builtin,
    bound: BTreeMap<String, Value>,
    span: Option<Span>,
    frame: &FrameView<'_>,
) -> Result<Value, RuntimeError> {
    let module = frame.module().clone();
    match builtin.name {
        "打印" => {
            let value = take(&bound, "值", builtin, span)?;
            interpreter
                .host_invoke(HostOp::Print, vec![value], span, &module)
                .map(|_| Value::Unit)
        }
        "长度" => {
            let value = take(&bound, "值", builtin, span)?;
            let length = match &value {
                Value::Text(text) => text.chars().count(),
                Value::List(items) => items.len(),
                Value::Map(map) => map.len(),
                Value::Table(table) => table.rows.len(),
                other => {
                    return Err(RuntimeError::type_error(
                        span,
                        format!("长度 is not defined for {}", other.type_name()),
                    ))
                }
            };
            Ok(Value::int(length as i64))
        }
        "类型" => {
            let value = take(&bound, "值", builtin, span)?;
            Ok(Value::text(value.type_name()))
        }
        "文本" => {
            let value = take(&bound, "值", builtin, span)?;
            match value {
                Value::Text(text) => Ok(Value::Text(text)),
                Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::None | Value::Unit => {
                    Ok(Value::Text(value.display()))
                }
                other => Err(RuntimeError::type_error(
                    span,
                    format!("cannot convert {} to Text explicitly", other.type_name()),
                )),
            }
        }
        "整数" => {
            let value = take(&bound, "值", builtin, span)?;
            match value {
                Value::Int(int) => Ok(Value::Int(int)),
                Value::Float(float) => float_to_bigint(float).map(Value::Int).ok_or_else(|| {
                    RuntimeError::type_error(span, "Float is not integral and cannot become Int")
                }),
                Value::Text(text) => text.trim().parse().map(Value::Int).map_err(|_| {
                    RuntimeError::type_error(span, format!("`{text}` is not a decimal integer"))
                }),
                other => Err(RuntimeError::type_error(
                    span,
                    format!("cannot convert {} to Int", other.type_name()),
                )),
            }
        }
        "小数" => {
            let value = take(&bound, "值", builtin, span)?;
            match value {
                Value::Float(float) => Ok(Value::Float(float)),
                Value::Int(int) => {
                    use num_traits::ToPrimitive;
                    int.to_f64().map(Value::Float).ok_or_else(|| {
                        RuntimeError::new(
                            ErrorCategory::ResourceExhausted,
                            span,
                            "integer is too large to convert to a Float",
                        )
                    })
                }
                Value::Text(text) => text.trim().parse::<f64>().map(Value::Float).map_err(|_| {
                    RuntimeError::type_error(span, format!("`{text}` is not a decimal number"))
                }),
                other => Err(RuntimeError::type_error(
                    span,
                    format!("cannot convert {} to Float", other.type_name()),
                )),
            }
        }
        "表" => {
            let header = take(&bound, "表头", builtin, span)?;
            let rows = take(&bound, "行", builtin, span)?;
            table(header, rows, span)
        }
        "ref" => {
            let target = take(&bound, "目标", builtin, span)?;
            let Value::Text(target) = target else {
                return Err(RuntimeError::type_error(span, "ref target must be Text"));
            };
            resolve_ref(interpreter, &module, &target, span)
        }
        "math.parse" => {
            let formula = take(&bound, "formula", builtin, span)?;
            let text = formula_text(formula, span)?;
            interpreter.host_invoke(HostOp::MathParse, vec![Value::Text(text)], span, &module)
        }
        "math.eval" => {
            let formula = take(&bound, "formula", builtin, span)?;
            let bindings = take(&bound, "bindings", builtin, span)?;
            let mut args = vec![formula, bindings];
            if let Some(target) = bound.get("target") {
                args.push(target.clone());
            }
            interpreter.host_invoke(HostOp::MathEval, args, span, &module)
        }
        "math.substitute" => {
            let formula = take(&bound, "formula", builtin, span)?;
            let bindings = take(&bound, "bindings", builtin, span)?;
            interpreter.host_invoke(
                HostOp::MathSubstitute,
                vec![formula, bindings],
                span,
                &module,
            )
        }
        "math.simplify" => {
            let formula = take(&bound, "formula", builtin, span)?;
            interpreter.host_invoke(HostOp::MathSimplify, vec![formula], span, &module)
        }
        "math.differentiate" => {
            let formula = take(&bound, "formula", builtin, span)?;
            let variable = take(&bound, "变量", builtin, span)?;
            interpreter.host_invoke(
                HostOp::MathDifferentiate,
                vec![formula, variable],
                span,
                &module,
            )
        }
        "foreign.run" => {
            let code = take(&bound, "ref", builtin, span)?;
            let mut args = vec![code];
            if let Some(input) = bound.get("input") {
                args.push(input.clone());
            }
            interpreter.host_invoke(HostOp::ForeignRun, args, span, &module)
        }
        "foreign.call" => {
            let code = take(&bound, "ref", builtin, span)?;
            let input = take(&bound, "input", builtin, span)?;
            interpreter.host_invoke(HostOp::ForeignCall, vec![code, input], span, &module)
        }
        "fs.read" => {
            let path = take(&bound, "path", builtin, span)?;
            interpreter.host_invoke(HostOp::FsRead, vec![path], span, &module)
        }
        "fs.write" => {
            let path = take(&bound, "path", builtin, span)?;
            let contents = take(&bound, "contents", builtin, span)?;
            let Value::Text(text) = contents else {
                return Err(RuntimeError::type_error(
                    span,
                    "fs.write contents must be Text",
                ));
            };
            interpreter.host_invoke(
                HostOp::FsWrite,
                vec![path, Value::Text(text)],
                span,
                &module,
            )
        }
        other => Err(RuntimeError::new(
            ErrorCategory::KernelUnavailable,
            span,
            format!("builtin `{other}` is not implemented in this runtime"),
        )),
    }
}

fn take(
    bound: &BTreeMap<String, Value>,
    name: &str,
    builtin: &Builtin,
    span: Option<Span>,
) -> Result<Value, RuntimeError> {
    bound.get(name).cloned().ok_or_else(|| {
        RuntimeError::new(
            ErrorCategory::Arity,
            span,
            format!("`{}` is missing argument `{name}`", builtin.name),
        )
    })
}

fn formula_text(value: Value, span: Option<Span>) -> Result<String, RuntimeError> {
    match value {
        Value::Formula(formula) => Ok(formula.latex),
        Value::Text(text) => Ok(text),
        other => Err(RuntimeError::type_error(
            span,
            format!(
                "formula argument must be Formula or Text, got {}",
                other.type_name()
            ),
        )),
    }
}

fn table(header: Value, rows: Value, span: Option<Span>) -> Result<Value, RuntimeError> {
    let Value::List(header_values) = header else {
        return Err(RuntimeError::type_error(
            span,
            "table header must be a List",
        ));
    };
    let mut header_names = Vec::with_capacity(header_values.len());
    for value in header_values {
        match value {
            Value::Text(name) => header_names.push(name),
            other => {
                return Err(RuntimeError::type_error(
                    span,
                    format!("table header cells must be Text, got {}", other.type_name()),
                ))
            }
        }
    }
    let Value::List(row_values) = rows else {
        return Err(RuntimeError::type_error(span, "table rows must be a List"));
    };
    let mut table_rows = Vec::with_capacity(row_values.len());
    for row in row_values {
        let Value::List(cells) = row else {
            return Err(RuntimeError::type_error(span, "table row must be a List"));
        };
        if cells.len() != header_names.len() {
            return Err(RuntimeError::type_error(
                span,
                format!(
                    "table row has {} cells but header has {}",
                    cells.len(),
                    header_names.len()
                ),
            ));
        }
        table_rows.push(cells);
    }
    Ok(Value::Table(Table {
        header: header_names,
        rows: table_rows,
    }))
}

/// Resolve `ref("#anchor")` to a function reference or an inert resource.
fn resolve_ref(
    interpreter: &Interpreter<'_>,
    module_id: &marqdo_ir::ModuleId,
    target: &str,
    span: Option<Span>,
) -> Result<Value, RuntimeError> {
    let program = interpreter.program();
    let Some(module) = program.module(module_id) else {
        return Err(RuntimeError::new(
            ErrorCategory::UnknownSymbol,
            span,
            format!("unknown module `{module_id}`"),
        ));
    };
    let Some(fragment) = target.strip_prefix('#') else {
        return Err(RuntimeError::new(
            ErrorCategory::UnknownSymbol,
            span,
            format!("ref target `{target}` must be a `#heading` anchor"),
        ));
    };
    let Some(unit_id) = module.anchors.get(fragment).cloned() else {
        return Err(RuntimeError::new(
            ErrorCategory::UnknownSymbol,
            span,
            format!("no heading matches `{target}`"),
        ));
    };
    let resources: Vec<&Resource> = module.resources_of(&unit_id).collect();
    match resources.len() {
        1 => Ok(match resources[0] {
            Resource::Formula {
                latex,
                display,
                span: resource_span,
                ..
            } => Value::Formula(FormulaValue {
                latex: latex.clone(),
                unit: Some(unit_id.to_string()),
                display: *display,
                span: Some(*resource_span),
            }),
            Resource::Code {
                language,
                source,
                content_hash,
                span: resource_span,
                ..
            } => Value::Code(CodeValue {
                language: language.clone(),
                source: source.clone(),
                unit: Some(unit_id.to_string()),
                content_hash: content_hash.clone(),
                span: Some(*resource_span),
            }),
        }),
        0 => {
            if let Some(function) = module.functions.iter().find(|f| f.unit == unit_id) {
                return Ok(Value::FunctionRef(FunctionRef {
                    module: function.id.module.to_string(),
                    name: function.id.name.clone(),
                }));
            }
            Err(RuntimeError::new(
                ErrorCategory::UnknownSymbol,
                span,
                format!("`{target}` contains no formula, code block or function"),
            ))
        }
        count => Err(RuntimeError::new(
            ErrorCategory::RefAmbiguous,
            span,
            format!(
                "`{target}` contains {count} resources; select one explicitly instead of relying on the first"
            ),
        )),
    }
}

/// Small view used by builtin dispatch to avoid borrowing the whole frame.
pub(crate) struct FrameView<'f> {
    module: &'f marqdo_ir::ModuleId,
}

impl<'f> FrameView<'f> {
    pub(crate) fn new(module: &'f marqdo_ir::ModuleId) -> Self {
        Self { module }
    }

    pub(crate) fn module(&self) -> &'f marqdo_ir::ModuleId {
        self.module
    }
}
