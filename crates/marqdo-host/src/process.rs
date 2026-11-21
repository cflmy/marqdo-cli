//! Explicitly granted process execution (ADR-0044).
//!
//! The child is **not** an OS sandbox: it inherits the CLI user's privileges.
//! The host checks an exact program grant before calling this module.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use marqdo_value::{ErrorCategory, RuntimeError, Value};

use crate::context::ExecutionContext;

fn process_error(code: &str, message: &str) -> Value {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(false));
    map.insert("code".to_string(), Value::text(code));
    map.insert("message".to_string(), Value::text(message));
    Value::Map(map)
}

pub(crate) fn run_process(
    root: &Path,
    program: &str,
    args: &[String],
    ctx: &ExecutionContext,
) -> Result<Value, RuntimeError> {
    const LIMIT: usize = 1024 * 1024;
    let child = Command::new(program)
        .args(args)
        .current_dir(root)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(process_error(
                "process.not_found",
                &format!("program `{program}` was not found"),
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(RuntimeError::new(
                ErrorCategory::PermissionDenied,
                ctx.span,
                format!("cannot execute `{program}`: {error}"),
            ))
        }
        Err(error) => {
            return Err(RuntimeError::new(
                ErrorCategory::HostError,
                ctx.span,
                format!("cannot start `{program}`: {error}"),
            ))
        }
    };
    let mut child = child;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                timed_out = true;
                let _ = child.kill();
                break;
            }
            Err(error) => {
                return Err(RuntimeError::new(
                    ErrorCategory::HostError,
                    ctx.span,
                    format!("cannot wait for `{program}`: {error}"),
                ))
            }
        }
    }
    let output = child.wait_with_output().map_err(|error| {
        RuntimeError::new(
            ErrorCategory::HostError,
            ctx.span,
            format!("cannot collect `{program}` output: {error}"),
        )
    })?;
    if timed_out {
        return Ok(process_error(
            "process.timeout",
            &format!("program `{program}` exceeded the 5 second limit"),
        ));
    }
    if output.stdout.len() > LIMIT || output.stderr.len() > LIMIT {
        return Ok(process_error(
            "process.output_too_large",
            &format!("program `{program}` exceeded the {LIMIT} byte output limit"),
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|_| {
        RuntimeError::new(
            ErrorCategory::Type,
            ctx.span,
            format!("program `{program}` stdout is not valid UTF-8"),
        )
    })?;
    let stderr = String::from_utf8(output.stderr).map_err(|_| {
        RuntimeError::new(
            ErrorCategory::Type,
            ctx.span,
            format!("program `{program}` stderr is not valid UTF-8"),
        )
    })?;
    let mut value = BTreeMap::new();
    value.insert(
        "status".to_string(),
        Value::int(output.status.code().unwrap_or(-1) as i64),
    );
    value.insert("stdout".to_string(), Value::text(stdout));
    value.insert("stderr".to_string(), Value::text(stderr));
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(true));
    map.insert("value".to_string(), Value::Map(value));
    Ok(Value::Map(map))
}
