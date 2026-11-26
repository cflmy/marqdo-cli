//! Implementations of the v1 builtins.
//!
//! Pure builtins run in the interpreter; effectful builtins and kernels go
//! through the capability host. `ref` resolves inert document resources and
//! never evaluates them.

use std::collections::BTreeMap;

use marqdo_host_contract::{HostOp, ResourceRead};
use marqdo_ir::builtins::Builtin;
use marqdo_ir::Resource;
use marqdo_span::Span;
use marqdo_value::{
    float_to_bigint, CodeValue, ErrorCategory, FormulaValue, FunctionRef, ResourceHandle,
    RuntimeError, Table, Value,
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
            // Compat-C (ADR-0031): the legacy HostOp::Print path is gone. On
            // the official CLI the loader routes 打印/print to stdlib io.写值;
            // reaching this builtin means the verified bundle was unavailable.
            let _ = take(&bound, "值", builtin, span)?;
            Err(RuntimeError::new(
                ErrorCategory::KernelUnavailable,
                span,
                "打印 requires the bundled stdlib print module; install the official CLI bundle",
            ))
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
        "memory.new" => {
            let initial_pages = page_count(take(&bound, "initial_pages", builtin, span)?, span)?;
            let max_pages = page_count(take(&bound, "max_pages", builtin, span)?, span)?;
            let handle = interpreter
                .memory_mut()
                .create(initial_pages, max_pages, span)?;
            interpreter.observe_memory_effect("memory.new", &module, span)?;
            Ok(Value::Memory(handle))
        }
        "memory.size" => {
            let memory = take(&bound, "memory", builtin, span)?;
            let memory_type = memory.type_name();
            let Value::Memory(handle) = memory else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("memory.size expects a Memory value, got {memory_type}"),
                ));
            };
            let pages = interpreter.memory().size(handle, span)?;
            Ok(pages_value(pages, span)?)
        }
        "memory.grow" => {
            let memory = take(&bound, "memory", builtin, span)?;
            let memory_type = memory.type_name();
            let Value::Memory(handle) = memory else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("memory.grow expects a Memory value, got {memory_type}"),
                ));
            };
            let pages = page_count(take(&bound, "pages", builtin, span)?, span)?;
            let old_pages = interpreter.memory_mut().grow(handle, pages, span)?;
            interpreter.observe_memory_effect("memory.grow", &module, span)?;
            Ok(pages_value(old_pages, span)?)
        }
        "list.append" => {
            let list = take(&bound, "list", builtin, span)?;
            let value = take(&bound, "value", builtin, span)?;
            let list_type = list.type_name();
            let Value::List(mut items) = list else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("list.append expects a List, got {list_type}"),
                ));
            };
            items.push(value);
            Ok(Value::List(items))
        }
        "list.concat" => {
            let left = take(&bound, "left", builtin, span)?;
            let right = take(&bound, "right", builtin, span)?;
            let left_type = left.type_name();
            let Value::List(mut items) = left else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("list.concat left operand must be a List, got {left_type}"),
                ));
            };
            let right_type = right.type_name();
            let Value::List(other) = right else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("list.concat right operand must be a List, got {right_type}"),
                ));
            };
            items.extend(other);
            Ok(Value::List(items))
        }
        "map.keys" => {
            let value = take(&bound, "map", builtin, span)?;
            let value_type = value.type_name();
            let Value::Map(map) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("map.keys expects a Map, got {value_type}"),
                ));
            };
            Ok(Value::List(
                map.keys().cloned().map(Value::Text).collect::<Vec<_>>(),
            ))
        }
        "map.has" => {
            let value = take(&bound, "map", builtin, span)?;
            let key = map_key(take(&bound, "key", builtin, span)?, "map.has", span)?;
            let value_type = value.type_name();
            let Value::Map(map) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("map.has expects a Map, got {value_type}"),
                ));
            };
            Ok(Value::Bool(map.contains_key(&key)))
        }
        "map.get" => {
            let value = take(&bound, "map", builtin, span)?;
            let key = map_key(take(&bound, "key", builtin, span)?, "map.get", span)?;
            let value_type = value.type_name();
            let Value::Map(map) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("map.get expects a Map, got {value_type}"),
                ));
            };
            Ok(map.get(&key).cloned().unwrap_or(Value::None))
        }
        "map.set" => {
            let value = take(&bound, "map", builtin, span)?;
            let key = map_key(take(&bound, "key", builtin, span)?, "map.set", span)?;
            let new_value = take(&bound, "value", builtin, span)?;
            let value_type = value.type_name();
            let Value::Map(mut map) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("map.set expects a Map, got {value_type}"),
                ));
            };
            map.insert(key, new_value);
            Ok(Value::Map(map))
        }
        "map.remove" => {
            let value = take(&bound, "map", builtin, span)?;
            let key = map_key(take(&bound, "key", builtin, span)?, "map.remove", span)?;
            let value_type = value.type_name();
            let Value::Map(mut map) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("map.remove expects a Map, got {value_type}"),
                ));
            };
            map.remove(&key);
            Ok(Value::Map(map))
        }
        "text.codepoints" => {
            let value = take(&bound, "text", builtin, span)?;
            let value_type = value.type_name();
            let Value::Text(text) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("text.codepoints expects Text, got {value_type}"),
                ));
            };
            Ok(Value::List(
                text.chars()
                    .map(|character| Value::int(i64::from(character as u32)))
                    .collect(),
            ))
        }
        "text.from_codepoint" => {
            let value = take(&bound, "codepoint", builtin, span)?;
            let value_type = value.type_name();
            let Value::Int(codepoint) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("text.from_codepoint expects an Int, got {value_type}"),
                ));
            };
            let Some(codepoint) = num_traits::ToPrimitive::to_u32(&codepoint) else {
                return Ok(Value::None);
            };
            Ok(match char::from_u32(codepoint) {
                Some(character) => Value::Text(character.to_string()),
                None => Value::None,
            })
        }
        "table.columns" => {
            let value = take(&bound, "table", builtin, span)?;
            let value_type = value.type_name();
            let Value::Table(table) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("table.columns expects a Table, got {value_type}"),
                ));
            };
            Ok(Value::List(
                table.header.into_iter().map(Value::Text).collect(),
            ))
        }
        "table.row" => {
            let value = take(&bound, "table", builtin, span)?;
            let row = take(&bound, "row", builtin, span)?;
            let value_type = value.type_name();
            let Value::Table(table) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("table.row expects a Table, got {value_type}"),
                ));
            };
            let position = table_index(row, table.rows.len(), "row", span)?;
            Ok(Value::List(table.rows[position].clone()))
        }
        "table.cell" => {
            let value = take(&bound, "table", builtin, span)?;
            let row = take(&bound, "row", builtin, span)?;
            let column = take(&bound, "column", builtin, span)?;
            let value_type = value.type_name();
            let Value::Table(table) = value else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("table.cell expects a Table, got {value_type}"),
                ));
            };
            let row = table_index(row, table.rows.len(), "row", span)?;
            let column = table_index(column, table.header.len(), "column", span)?;
            Ok(table.rows[row][column].clone())
        }
        "resource.open" => {
            let kind = expect_text_value(take(&bound, "kind", builtin, span)?, span)?;
            let descriptor = expect_text_value(take(&bound, "descriptor", builtin, span)?, span)?;
            let rights = rights_list(take(&bound, "rights", builtin, span)?, span)?;
            interpreter.resource_open(&kind, &descriptor, &rights, &module, span)
        }
        "resource.read" => {
            let handle = resource_handle(take(&bound, "handle", builtin, span)?, span)?;
            let memory = memory_handle(
                take(&bound, "memory", builtin, span)?,
                "resource.read",
                span,
            )?;
            let offset = take(&bound, "offset", builtin, span)?;
            let offset_int = int_value(offset, "resource.read offset", span)?;
            let offset_value = Value::Int(offset_int.clone());
            let max_bytes = positive_count(
                take(&bound, "max_bytes", builtin, span)?,
                "resource.read max_bytes",
                span,
            )?;
            let available = interpreter.memory().byte_len(memory, span)?;
            let start = non_negative_usize(&offset_value, "resource.read offset", span)?;
            if start >= available {
                return Err(RuntimeError::new(
                    ErrorCategory::IndexOutOfBounds,
                    span,
                    format!("resource.read offset {start} is at or beyond the Memory length {available}"),
                ));
            }
            let limit = max_bytes.min(available - start);
            let result = interpreter.resource_read(handle, limit, &module, span);
            match result {
                Ok(read) => {
                    interpreter
                        .memory_mut()
                        .write_bytes(memory, &offset_int, &read.bytes, span)?;
                    Ok(resource_ok(resource_read_payload(&read)))
                }
                Err(error) => resource_recoverable(error),
            }
        }
        "resource.write" => {
            let handle = resource_handle(take(&bound, "handle", builtin, span)?, span)?;
            let memory = memory_handle(
                take(&bound, "memory", builtin, span)?,
                "resource.write",
                span,
            )?;
            let offset = take(&bound, "offset", builtin, span)?;
            let offset_int = int_value(offset, "resource.write offset", span)?;
            let length = non_negative_usize(
                &take(&bound, "length", builtin, span)?,
                "resource.write length",
                span,
            )?;
            let bytes = interpreter
                .memory()
                .read_bytes(memory, &offset_int, length, span)?;
            match interpreter.resource_write(handle, &bytes, &module, span) {
                Ok(count) => Ok(resource_ok(resource_write_payload(count))),
                Err(error) => resource_recoverable(error),
            }
        }
        "resource.close" => {
            let handle = resource_handle(take(&bound, "handle", builtin, span)?, span)?;
            match interpreter.resource_close(handle, &module, span) {
                Ok(()) => Ok(resource_ok(Value::None)),
                Err(error) => resource_recoverable(error),
            }
        }
        "resource.accept" => {
            let handle = resource_handle(take(&bound, "handle", builtin, span)?, span)?;
            interpreter.resource_accept(handle, &module, span)
        }
        "resource.request" => {
            let operation = expect_text_value(take(&bound, "operation", builtin, span)?, span)?;
            let value_type = bound["arguments"].type_name();
            let Value::List(arguments) = take(&bound, "arguments", builtin, span)? else {
                return Err(RuntimeError::type_error(
                    span,
                    format!("resource.request arguments must be a List, got {value_type}"),
                ));
            };
            interpreter.resource_request(&operation, arguments, &module, span)
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

/// Validate a page-count argument: an `Int` in the non-negative `u64` range.
fn page_count(value: Value, span: Option<Span>) -> Result<u64, RuntimeError> {
    let value_type = value.type_name();
    let Value::Int(count) = value else {
        return Err(RuntimeError::type_error(
            span,
            format!("memory page count must be an Int, got {value_type}"),
        ));
    };
    num_traits::ToPrimitive::to_u64(&count).ok_or_else(|| {
        RuntimeError::new(
            ErrorCategory::MemoryFault,
            span,
            format!("memory page count {count} must be a non-negative integer"),
        )
    })
}

/// Convert a page count back to a language `Int`.
fn pages_value(pages: u64, span: Option<Span>) -> Result<Value, RuntimeError> {
    i64::try_from(pages).map(Value::int).map_err(|_| {
        RuntimeError::new(
            ErrorCategory::MemoryFault,
            span,
            format!("page count {pages} does not fit an Int"),
        )
    })
}

/// Require a `Text` map key and return it.
fn map_key(value: Value, builtin: &str, span: Option<Span>) -> Result<String, RuntimeError> {
    let value_type = value.type_name();
    match value {
        Value::Text(key) => Ok(key),
        _ => Err(RuntimeError::type_error(
            span,
            format!("{builtin} key must be Text, got {value_type}"),
        )),
    }
}

/// Require a Text value.
fn expect_text_value(value: Value, span: Option<Span>) -> Result<String, RuntimeError> {
    let value_type = value.type_name();
    let Value::Text(text) = value else {
        return Err(RuntimeError::type_error(
            span,
            format!("expected Text, got {value_type}"),
        ));
    };
    Ok(text)
}

/// Require a List of Text rights.
fn rights_list(value: Value, span: Option<Span>) -> Result<Vec<String>, RuntimeError> {
    let value_type = value.type_name();
    let Value::List(items) = value else {
        return Err(RuntimeError::type_error(
            span,
            format!("resource rights must be a List, got {value_type}"),
        ));
    };
    let mut rights = Vec::with_capacity(items.len());
    for item in items {
        let item_type = item.type_name();
        let Value::Text(right) = item else {
            return Err(RuntimeError::type_error(
                span,
                format!("resource rights must contain Text, got {item_type}"),
            ));
        };
        rights.push(right);
    }
    Ok(rights)
}

/// Require an opaque resource handle.
fn resource_handle(value: Value, span: Option<Span>) -> Result<ResourceHandle, RuntimeError> {
    let value_type = value.type_name();
    let Value::Resource(handle) = value else {
        return Err(RuntimeError::type_error(
            span,
            format!("expected a Resource handle, got {value_type}"),
        ));
    };
    Ok(handle)
}

/// Require an opaque Memory handle.
fn memory_handle(
    value: Value,
    builtin: &str,
    span: Option<Span>,
) -> Result<marqdo_value::MemoryHandle, RuntimeError> {
    let value_type = value.type_name();
    let Value::Memory(handle) = value else {
        return Err(RuntimeError::type_error(
            span,
            format!("{builtin} memory must be a Memory value, got {value_type}"),
        ));
    };
    Ok(handle)
}

/// Require an Int value.
fn int_value(
    value: Value,
    what: &str,
    span: Option<Span>,
) -> Result<num_bigint::BigInt, RuntimeError> {
    let value_type = value.type_name();
    let Value::Int(value) = value else {
        return Err(RuntimeError::type_error(
            span,
            format!("{what} must be an Int, got {value_type}"),
        ));
    };
    Ok(value)
}

/// Require a non-negative usize.
fn non_negative_usize(
    value: &Value,
    what: &str,
    span: Option<Span>,
) -> Result<usize, RuntimeError> {
    let value_type = value.type_name();
    let Value::Int(value) = value else {
        return Err(RuntimeError::type_error(
            span,
            format!("{what} must be an Int, got {value_type}"),
        ));
    };
    num_traits::ToPrimitive::to_usize(value).ok_or_else(|| {
        RuntimeError::new(
            ErrorCategory::IndexOutOfBounds,
            span,
            format!("{what} {value} must be a non-negative integer in range"),
        )
    })
}

/// Require a strictly positive byte count.
fn positive_count(value: Value, what: &str, span: Option<Span>) -> Result<usize, RuntimeError> {
    let count = non_negative_usize(&value, what, span)?;
    if count == 0 {
        return Err(RuntimeError::type_error(
            span,
            format!("{what} must be greater than zero"),
        ));
    }
    Ok(count)
}

fn resource_ok(value: Value) -> Value {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(true));
    map.insert("value".to_string(), value);
    Value::Map(map)
}

fn resource_err(code: &str, message: &str) -> Value {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(false));
    map.insert("code".to_string(), Value::Text(code.to_string()));
    map.insert("message".to_string(), Value::Text(message.to_string()));
    Value::Map(map)
}

fn resource_read_payload(read: &ResourceRead) -> Value {
    let mut map = BTreeMap::new();
    map.insert("count".to_string(), Value::int(read.bytes.len() as i64));
    map.insert("eof".to_string(), Value::Bool(read.eof));
    Value::Map(map)
}

fn resource_write_payload(count: usize) -> Value {
    let mut map = BTreeMap::new();
    map.insert("count".to_string(), Value::int(count as i64));
    Value::Map(map)
}

/// Convert a closed-handle failure into a recoverable Result; other errors are
/// fatal (ADR-0025).
fn resource_recoverable(error: RuntimeError) -> Result<Value, RuntimeError> {
    if error.category == ErrorCategory::ResourceClosed {
        return Ok(resource_err("resource.closed", &error.message));
    }
    Err(error)
}

/// Validate a non-negative table row/column coordinate.
fn table_index(
    value: Value,
    len: usize,
    what: &str,
    span: Option<Span>,
) -> Result<usize, RuntimeError> {
    let value_type = value.type_name();
    let Value::Int(index) = value else {
        return Err(RuntimeError::type_error(
            span,
            format!("table {what} must be an Int, got {value_type}"),
        ));
    };
    let Some(position) = num_traits::ToPrimitive::to_usize(&index) else {
        return Err(RuntimeError::new(
            ErrorCategory::IndexOutOfBounds,
            span,
            format!("table {what} {index} is out of bounds for length {len}"),
        ));
    };
    if position >= len {
        return Err(RuntimeError::new(
            ErrorCategory::IndexOutOfBounds,
            span,
            format!("table {what} {position} is out of bounds for length {len}"),
        ));
    }
    Ok(position)
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
