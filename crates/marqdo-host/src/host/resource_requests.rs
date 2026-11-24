//! Closed compatibility catalogue for non-streaming resource operations (ADR-0062).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{ErrorCategory, ExecutionContext, RuntimeError, SandboxMode, StdHost, Value};

impl StdHost {
    pub(super) fn request_compatibility_operation(
        &mut self,
        operation: &str,
        args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        if self.mode != SandboxMode::Project {
            return Err(RuntimeError::new(
                ErrorCategory::PermissionDenied,
                ctx.span,
                "resource operations are available only in project mode",
            ));
        }
        if args.iter().any(Value::contains_memory) || args.iter().any(Value::contains_resource) {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                ctx.span,
                "resource operations accept only Text path arguments",
            ));
        }
        match operation {
            "process.run" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `process.run` expects 2 arguments, got {}",
                            args.len()
                        ),
                    ));
                }
                let program_type = args[0].type_name();
                let Value::Text(program) = args[0].clone() else {
                    return Err(RuntimeError::type_error(
                    ctx.span,
                    format!(
                        "resource operation `process.run` expects a Text program, got {program_type}"
                    ),
                ));
                };
                let args_type = args[1].type_name();
                let Value::List(values) = args[1].clone() else {
                    return Err(RuntimeError::type_error(
                    ctx.span,
                    format!(
                        "resource operation `process.run` expects a List of Text arguments, got {args_type}"
                    ),
                ));
                };
                let mut program_args = Vec::with_capacity(values.len());
                for value in values {
                    let value_type = value.type_name();
                    let Value::Text(value) = value else {
                        return Err(RuntimeError::type_error(
                            ctx.span,
                            format!("process arguments must be Text, got {value_type}"),
                        ));
                    };
                    program_args.push(value);
                }
                self.capabilities.check_process(&program, ctx.span)?;
                let result =
                    crate::process::run_process(self.fs.root(), &program, &program_args, ctx)?;
                self.observe_effect("process.run", "ok", ctx);
                Ok(result)
            }
            "regex.match" | "regex.captures" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `{operation}` expects 2 arguments, got {}",
                            args.len()
                        ),
                    ));
                }
                let pattern_type = args[0].type_name();
                let Value::Text(pattern) = args[0].clone() else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("{operation} pattern must be Text, got {pattern_type}"),
                    ));
                };
                let text_type = args[1].type_name();
                let Value::Text(text) = args[1].clone() else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("{operation} text must be Text, got {text_type}"),
                    ));
                };
                let regex = match regex::Regex::new(&pattern) {
                    Ok(regex) => regex,
                    Err(error) => {
                        return Ok(result_error(
                            "regex.invalid",
                            &format!("invalid regular expression: {error}"),
                        ))
                    }
                };
                self.observe_effect(operation, "ok", ctx);
                let mut map = BTreeMap::new();
                map.insert("ok".to_string(), Value::Bool(true));
                if operation == "regex.match" {
                    map.insert("value".to_string(), Value::Bool(regex.is_match(&text)));
                } else {
                    let mut payload = BTreeMap::new();
                    let captures = regex.captures(&text);
                    payload.insert("matched".to_string(), Value::Bool(captures.is_some()));
                    let groups = match captures {
                        Some(captures) => captures
                            .iter()
                            .map(|group| match group {
                                Some(group) => Value::text(group.as_str()),
                                None => Value::None,
                            })
                            .collect(),
                        None => Vec::new(),
                    };
                    payload.insert("groups".to_string(), Value::List(groups));
                    map.insert("value".to_string(), Value::Map(payload));
                }
                Ok(Value::Map(map))
            }
            "compress.gzip" => {
                if args.len() != 1 {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `compress.gzip` expects 1 argument, got {}",
                            args.len()
                        ),
                    ));
                }
                let text_type = args[0].type_name();
                let Value::Text(text) = args[0].clone() else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("compress.gzip expects Text, got {text_type}"),
                    ));
                };
                if text.len() > 8 * 1024 * 1024 {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceExhausted,
                        ctx.span,
                        "compress.gzip input exceeds 8 MiB",
                    ));
                }
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(text.as_bytes()).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("gzip failed: {error}"),
                    )
                })?;
                let bytes = encoder.finish().map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("gzip failed: {error}"),
                    )
                })?;
                let values = bytes
                    .into_iter()
                    .map(|byte| Value::int(byte as i64))
                    .collect();
                self.observe_effect("compress.gzip", "ok", ctx);
                let mut map = BTreeMap::new();
                map.insert("ok".to_string(), Value::Bool(true));
                map.insert("value".to_string(), Value::List(values));
                Ok(Value::Map(map))
            }
            "compress.gunzip" => {
                if args.len() != 1 {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `compress.gunzip` expects 1 argument, got {}",
                            args.len()
                        ),
                    ));
                }
                let list_type = args[0].type_name();
                let Value::List(values) = args[0].clone() else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("compress.gunzip expects List<Int>, got {list_type}"),
                    ));
                };
                if values.len() > 8 * 1024 * 1024 {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceExhausted,
                        ctx.span,
                        "compress.gunzip input exceeds 8 MiB",
                    ));
                }
                let mut bytes = Vec::with_capacity(values.len());
                for value in values {
                    let Value::Int(value) = value else {
                        return Err(RuntimeError::type_error(
                            ctx.span,
                            "compress.gunzip bytes must be Int",
                        ));
                    };
                    let Some(byte) = value.to_string().parse::<u8>().ok() else {
                        return Err(RuntimeError::new(
                            ErrorCategory::Type,
                            ctx.span,
                            format!("`{value}` is not a byte in 0..=255"),
                        ));
                    };
                    bytes.push(byte);
                }
                let mut decoder = flate2::read::GzDecoder::new(bytes.as_slice());
                let mut output = Vec::new();
                if let Err(error) = decoder
                    .by_ref()
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut output)
                {
                    return Ok(result_error(
                        "compress.invalid",
                        &format!("gunzip failed: {error}"),
                    ));
                }
                if output.len() > 8 * 1024 * 1024 {
                    return Ok(result_error(
                        "compress.too_large",
                        "gunzip output exceeds 8 MiB",
                    ));
                }
                let text = match String::from_utf8(output) {
                    Ok(text) => text,
                    Err(_) => {
                        return Ok(result_error(
                            "compress.not_utf8",
                            "gunzip output is not UTF-8",
                        ))
                    }
                };
                self.observe_effect("compress.gunzip", "ok", ctx);
                let mut map = BTreeMap::new();
                map.insert("ok".to_string(), Value::Bool(true));
                map.insert("value".to_string(), Value::text(text));
                Ok(Value::Map(map))
            }
            "random.bytes" => {
                if args.len() != 1 {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `random.bytes` expects 1 argument, got {}",
                            args.len()
                        ),
                    ));
                }
                let count = match &args[0] {
                    Value::Int(value) => value
                        .to_string()
                        .parse::<usize>()
                        .ok()
                        .filter(|count| (1..=65536).contains(count)),
                    _ => None,
                };
                let Some(count) = count else {
                    return Err(RuntimeError::new(
                        ErrorCategory::Type,
                        ctx.span,
                        "random.bytes requires an Int in 1..=65536",
                    ));
                };
                self.capabilities.check_random(ctx.span)?;
                let mut bytes = vec![0u8; count];
                getrandom::fill(&mut bytes).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("OS entropy is unavailable: {error}"),
                    )
                })?;
                let values = bytes
                    .into_iter()
                    .map(|byte| Value::int(byte as i64))
                    .collect();
                self.observe_effect("random.bytes", "ok", ctx);
                let mut map = BTreeMap::new();
                map.insert("ok".to_string(), Value::Bool(true));
                map.insert("value".to_string(), Value::List(values));
                Ok(Value::Map(map))
            }
            "env.get" => {
                if args.len() != 1 {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `env.get` expects 1 argument, got {}",
                            args.len()
                        ),
                    ));
                }
                let value_type = args[0].type_name();
                let Value::Text(name) = args[0].clone() else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!(
                            "resource operation `env.get` expects a Text name, got {value_type}"
                        ),
                    ));
                };
                self.capabilities.check_env(&name, ctx.span)?;
                let mut map = BTreeMap::new();
                match std::env::var(&name) {
                    Ok(value) => {
                        map.insert("ok".to_string(), Value::Bool(true));
                        map.insert("value".to_string(), Value::text(value));
                    }
                    Err(std::env::VarError::NotPresent) => {
                        map.insert("ok".to_string(), Value::Bool(false));
                        map.insert("code".to_string(), Value::text("env.not_found"));
                        map.insert(
                            "message".to_string(),
                            Value::text(format!("environment variable `{name}` is not set")),
                        );
                    }
                    Err(std::env::VarError::NotUnicode(_)) => {
                        map.insert("ok".to_string(), Value::Bool(false));
                        map.insert("code".to_string(), Value::text("env.not_unicode"));
                        map.insert(
                            "message".to_string(),
                            Value::text(format!(
                                "environment variable `{name}` is not valid UTF-8"
                            )),
                        );
                    }
                }
                self.observe_effect("env.get", "ok", ctx);
                Ok(Value::Map(map))
            }
            "clock.now" => {
                if !args.is_empty() {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `clock.now` expects 0 arguments, got {}",
                            args.len()
                        ),
                    ));
                }
                let millis = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| {
                        RuntimeError::new(
                            ErrorCategory::HostError,
                            ctx.span,
                            format!("system clock is before the Unix epoch: {error}"),
                        )
                    })?
                    .as_millis()
                    .min(i64::MAX as u128) as i64;
                let mut map = BTreeMap::new();
                map.insert("ok".to_string(), Value::Bool(true));
                map.insert("value".to_string(), Value::int(millis));
                self.observe_effect("clock.now", "ok", ctx);
                Ok(Value::Map(map))
            }
            "fs.read_at" => {
                if args.len() != 3 {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `fs.read_at` expects 3 arguments, got {}",
                            args.len()
                        ),
                    ));
                }
                let path_type = args[0].type_name();
                let Value::Text(path) = args[0].clone() else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("fs.read_at path must be Text, got {path_type}"),
                    ));
                };
                let offset = match &args[1] {
                    Value::Int(value) => value.to_string(),
                    other => {
                        return Err(RuntimeError::type_error(
                            ctx.span,
                            format!("fs.read_at offset must be Int, got {}", other.type_name()),
                        ))
                    }
                };
                let length = match &args[2] {
                    Value::Int(value) => value.to_string(),
                    other => {
                        return Err(RuntimeError::type_error(
                            ctx.span,
                            format!("fs.read_at length must be Int, got {}", other.type_name()),
                        ))
                    }
                };
                self.capabilities.check_read(&path, ctx.span)?;
                let result = self
                    .fs
                    .request("read_at", &[path, offset, length], ctx.span)?;
                self.observe_effect("fs.read_at", "ok", ctx);
                Ok(result)
            }
            "fs.append" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `fs.append` expects 2 arguments, got {}",
                            args.len()
                        ),
                    ));
                }
                let path_type = args[0].type_name();
                let Value::Text(path) = args[0].clone() else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("fs.append path must be Text, got {path_type}"),
                    ));
                };
                let text_type = args[1].type_name();
                let Value::Text(text) = args[1].clone() else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("fs.append text must be Text, got {text_type}"),
                    ));
                };
                self.capabilities.check_write(&path, ctx.span)?;
                let result = self.fs.request("append", &[path, text], ctx.span)?;
                self.observe_effect("fs.append", "ok", ctx);
                Ok(result)
            }
            "fs.walk" => {
                if args.len() != 1 {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                            "resource operation `fs.walk` expects 1 argument, got {}",
                            args.len()
                        ),
                    ));
                }
                let path_type = args[0].type_name();
                let Value::Text(path) = args[0].clone() else {
                    return Err(RuntimeError::type_error(
                        ctx.span,
                        format!("fs.walk path must be Text, got {path_type}"),
                    ));
                };
                self.capabilities.check_read(&path, ctx.span)?;
                let result = self.fs.request("walk", &[path], ctx.span)?;
                self.observe_effect("fs.walk", "ok", ctx);
                Ok(result)
            }
            "fs.exists" | "fs.metadata" | "fs.list" | "fs.mkdir" | "fs.remove" | "fs.rename" => {
                let name = &operation["fs.".len()..];
                let expected = if name == "rename" { 2 } else { 1 };
                if args.len() != expected {
                    return Err(RuntimeError::new(
                        ErrorCategory::Arity,
                        ctx.span,
                        format!(
                        "resource operation `{operation}` expects {expected} argument(s), got {}",
                        args.len()
                    ),
                    ));
                }
                let mut paths = Vec::with_capacity(args.len());
                for value in args {
                    let value_type = value.type_name();
                    let Value::Text(path) = value else {
                        return Err(RuntimeError::type_error(
                            ctx.span,
                            format!(
                            "resource operation `{operation}` expects Text paths, got {value_type}"
                        ),
                        ));
                    };
                    paths.push(path);
                }
                match name {
                    "exists" | "metadata" | "list" => {
                        self.capabilities.check_read(&paths[0], ctx.span)?;
                    }
                    "mkdir" | "remove" => {
                        self.capabilities.check_write(&paths[0], ctx.span)?;
                    }
                    "rename" => {
                        self.capabilities.check_write(&paths[0], ctx.span)?;
                        self.capabilities.check_write(&paths[1], ctx.span)?;
                    }
                    _ => unreachable!(),
                }
                let result = self.fs.request(name, &paths, ctx.span)?;
                self.observe_effect("resource.request", "ok", ctx);
                Ok(result)
            }
            _ => {
                let message = match operation.strip_prefix("fs.") {
                    Some(name) => format!("unsupported filesystem operation `{name}`"),
                    None => format!("unsupported resource operation `{operation}`"),
                };
                Err(RuntimeError::new(
                    ErrorCategory::ResourceUnsupported,
                    ctx.span,
                    message,
                ))
            }
        }
    }
}

fn result_error(code: &str, message: &str) -> Value {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(false));
    map.insert("code".to_string(), Value::text(code));
    map.insert("message".to_string(), Value::text(message));
    Value::Map(map)
}
