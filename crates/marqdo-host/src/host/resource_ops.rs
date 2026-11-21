//! Resource/stream open dispatch plus provider and TCP endpoints.
//!
//! Child module of `host`: it may use the parent's private fields, and keeps
//! the open-path policy out of the generic `Host::invoke` dispatch.

use super::*;

impl StdHost {
    pub(crate) fn open_resource_stream(
        &mut self,
        kind: &str,
        descriptor: &str,
        rights: &[String],
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        let started = Instant::now();
        let requested: BTreeSet<&str> = rights.iter().map(String::as_str).collect();
        if self.mode == SandboxMode::Strict {
            // stdio/stdout is the existing console channel already permitted by
            // strict L1 (CM-A11); stdin, stderr, file/network/process providers
            // are not, because strict isolation has not been proven for them.
            let stdio_console =
                kind == "stdio" && descriptor == "stdout" && requested == BTreeSet::from(["write"]);
            if !stdio_console {
                return Err(RuntimeError::new(
                    ErrorCategory::PermissionDenied,
                    ctx.span,
                    "strict mode permits only the existing stdio/stdout console channel; \
                     other resource streams are refused",
                ));
            }
        }
        if self.mode == SandboxMode::Pure && kind == "file" {
            return Err(RuntimeError::new(
                ErrorCategory::PermissionDenied,
                ctx.span,
                "pure mode does not provide project file resources",
            ));
        }
        match kind {
            "stdio" => {
                let allowed = match descriptor {
                    "stdout" | "stderr" => requested == BTreeSet::from(["write"]),
                    "stdin" => requested == BTreeSet::from(["read"]),
                    _ => false,
                };
                if !allowed {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceUnsupported,
                        ctx.span,
                        "stdio supports stdout/stderr with rights [write] and stdin with rights [read]",
                    ));
                }
                if !self.capabilities.console {
                    return Err(RuntimeError::new(
                        ErrorCategory::PermissionDenied,
                        ctx.span,
                        "the console capability is required for stdio resources",
                    ));
                }
                let handle = match descriptor {
                    "stdin" => self.resources.open_stdin(),
                    "stderr" => self.resources.open_stderr(),
                    "stdout" => self.resources.open_stdout(),
                    _ => unreachable!(),
                };
                self.record_semantic(
                    "resource.open",
                    None,
                    Some(&ctx.module_id),
                    ctx.span,
                    Some(&format!("kind=stdio descriptor={descriptor}")),
                );
                self.record_resource_effect("resource.open", ctx, started, "ok");
                Ok(Value::Resource(handle))
            }
            "file" => {
                if requested == BTreeSet::from(["read"]) {
                    self.capabilities.check_read(descriptor, ctx.span)?;
                    let (_, relative) = self.fs.resolve(descriptor, ctx.span)?;
                    let bytes = self.fs.read_bytes(descriptor, ctx.span)?;
                    let handle = self.resources.open_file(relative, bytes);
                    self.record_semantic(
                        "resource.open",
                        None,
                        Some(&ctx.module_id),
                        ctx.span,
                        Some("kind=file rights=read"),
                    );
                    self.record_resource_effect("resource.open", ctx, started, "ok");
                    return Ok(Value::Resource(handle));
                }
                if requested == BTreeSet::from(["write"]) {
                    self.capabilities.check_write(descriptor, ctx.span)?;
                    let (_, relative) = self.fs.resolve(descriptor, ctx.span)?;
                    let handle = self.resources.open_file_writer(relative);
                    self.record_semantic(
                        "resource.open",
                        None,
                        Some(&ctx.module_id),
                        ctx.span,
                        Some("kind=file rights=write"),
                    );
                    self.record_resource_effect("resource.open", ctx, started, "ok");
                    return Ok(Value::Resource(handle));
                }
                Err(RuntimeError::new(
                    ErrorCategory::ResourceUnsupported,
                    ctx.span,
                    "project file streams support exactly [read] or [write] in this build",
                ))
            }
            "tcp" => {
                if self.mode != SandboxMode::Project {
                    return Err(RuntimeError::new(
                        ErrorCategory::PermissionDenied,
                        ctx.span,
                        "tcp resources are available only in project mode",
                    ));
                }
                if requested != BTreeSet::from(["read", "write"]) {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceUnsupported,
                        ctx.span,
                        "tcp resources require exactly [read, write] in this build",
                    ));
                }
                let address: SocketAddr = descriptor.parse().map_err(|_| {
                    RuntimeError::new(
                        ErrorCategory::ResourceUnsupported,
                        ctx.span,
                        format!(
                            "tcp descriptor `{descriptor}` must be an IP literal address:port                              (DNS names are not resolved in this build)"
                        ),
                    )
                })?;
                self.capabilities
                    .check_net(&address.ip().to_string(), ctx.span)?;
                let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).map_err(
                    |error| {
                        RuntimeError::new(
                            ErrorCategory::HostError,
                            ctx.span,
                            format!("cannot connect to {address}: {error}"),
                        )
                    },
                )?;
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                let handle = self.resources.open_tcp(stream);
                self.record_semantic(
                    "resource.open",
                    None,
                    Some(&ctx.module_id),
                    ctx.span,
                    Some(&format!("kind=tcp address={address}")),
                );
                self.record_resource_effect("resource.open", ctx, started, "ok");
                Ok(Value::Resource(handle))
            }
            other => {
                if self.mode != SandboxMode::Project {
                    return Err(RuntimeError::new(
                        ErrorCategory::PermissionDenied,
                        ctx.span,
                        "resource providers are available only in project mode",
                    ));
                }
                let Some((provider_name, provider)) = self.resource_providers.get(other).cloned()
                else {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceUnsupported,
                        ctx.span,
                        format!("resource kind `{other}` is not registered"),
                    ));
                };
                self.capabilities
                    .check_resource_provider(&provider_name, ctx.span)?;
                let handle = self
                    .resources
                    .open_provider(provider, other, descriptor, rights, ctx.span)?;
                self.record_semantic(
                    "resource.open",
                    None,
                    Some(&ctx.module_id),
                    ctx.span,
                    Some(&format!("kind={other} provider={provider_name}")),
                );
                self.record_resource_effect("resource.open", ctx, started, "ok");
                Ok(Value::Resource(handle))
            }
        }
    }
}
