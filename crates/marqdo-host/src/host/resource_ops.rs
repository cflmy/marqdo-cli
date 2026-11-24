//! Resource/stream open dispatch plus provider and TCP endpoints.
//!
//! Child module of `host`: it may use the parent's private fields, and keeps
//! the open-path policy out of the generic `Host::invoke` dispatch.

use super::*;

fn parse_host_port(descriptor: &str, span: Option<Span>) -> Result<(String, u16), RuntimeError> {
    let invalid = || {
        RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            span,
            format!("tcp descriptor `{descriptor}` must be `host:port` or `[ipv6]:port`"),
        )
    };
    if let Some(rest) = descriptor.strip_prefix('[') {
        let (host, tail) = rest.split_once(']').ok_or_else(invalid)?;
        let port = tail.strip_prefix(':').ok_or_else(invalid)?;
        let port = port.parse::<u16>().map_err(|_| invalid())?;
        if host.is_empty() {
            return Err(invalid());
        }
        return Ok((host.to_string(), port));
    }
    let (host, port) = descriptor.rsplit_once(':').ok_or_else(invalid)?;
    if host.is_empty() {
        return Err(invalid());
    }
    let port = port.parse::<u16>().map_err(|_| invalid())?;
    Ok((host.to_string(), port))
}

impl StdHost {
    pub(crate) fn open_resource_stream(
        &mut self,
        kind: &str,
        descriptor: &str,
        rights: &[String],
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
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
                self.observe_effect("resource.open", "ok", ctx);
                Ok(Value::Resource(handle))
            }
            "file" => {
                if requested == BTreeSet::from(["read"]) {
                    self.capabilities.check_read(descriptor, ctx.span)?;
                    let (_, relative) = self.fs.resolve(descriptor, ctx.span)?;
                    let bytes = self.fs.read_bytes(descriptor, ctx.span)?;
                    let handle = self.resources.open_file(relative, bytes);
                    self.observe_effect("resource.open", "ok", ctx);
                    return Ok(Value::Resource(handle));
                }
                if requested == BTreeSet::from(["write"]) {
                    self.capabilities.check_write(descriptor, ctx.span)?;
                    let (_, relative) = self.fs.resolve(descriptor, ctx.span)?;
                    let handle = self.resources.open_file_writer(relative);
                    self.observe_effect("resource.open", "ok", ctx);
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
                let (host, port) = parse_host_port(descriptor, ctx.span)?;
                self.capabilities.check_net(&host, ctx.span)?;
                let addresses = (host.as_str(), port).to_socket_addrs().map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("cannot resolve `{host}:{port}`: {error}"),
                    )
                })?;
                let mut stream = None;
                let mut last_error = None;
                for address in addresses {
                    match TcpStream::connect_timeout(&address, Duration::from_secs(5)) {
                        Ok(connected) => {
                            stream = Some(connected);
                            break;
                        }
                        Err(error) => last_error = Some((address, error)),
                    }
                }
                let Some(stream) = stream else {
                    let detail = match last_error {
                        Some((address, error)) => format!("{address}: {error}"),
                        None => format!("`{host}:{port}` resolved to no addresses"),
                    };
                    return Err(RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("cannot connect to `{host}:{port}`: {detail}"),
                    ));
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                let handle = self.resources.open_tcp(stream);
                self.observe_effect("resource.open", "ok", ctx);
                Ok(Value::Resource(handle))
            }
            "tls" => {
                if self.mode != SandboxMode::Project {
                    return Err(RuntimeError::new(
                        ErrorCategory::PermissionDenied,
                        ctx.span,
                        "tls resources are available only in project mode",
                    ));
                }
                if requested != BTreeSet::from(["read", "write"]) {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceUnsupported,
                        ctx.span,
                        "tls resources require exactly [read, write] in this build",
                    ));
                }
                let (host, port) = parse_host_port(descriptor, ctx.span)?;
                self.capabilities.check_net(&host, ctx.span)?;
                let addresses = (host.as_str(), port).to_socket_addrs().map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("cannot resolve `{host}:{port}`: {error}"),
                    )
                })?;
                let mut tcp = None;
                let mut last_error = None;
                for address in addresses {
                    match TcpStream::connect_timeout(&address, Duration::from_secs(5)) {
                        Ok(connected) => {
                            tcp = Some(connected);
                            break;
                        }
                        Err(error) => last_error = Some((address, error)),
                    }
                }
                let Some(tcp) = tcp else {
                    let detail = match last_error {
                        Some((address, error)) => format!("{address}: {error}"),
                        None => format!("`{host}:{port}` resolved to no addresses"),
                    };
                    return Err(RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("cannot connect tls to `{host}:{port}`: {detail}"),
                    ));
                };
                let _ = tcp.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = tcp.set_write_timeout(Some(Duration::from_secs(5)));
                let mut roots = rustls::RootCertStore::empty();
                for certificate in rustls_native_certs::load_native_certs().certs {
                    let _ = roots.add(certificate);
                }
                for certificate in &self.tls_extra_roots {
                    roots.add(certificate.clone()).map_err(|error| {
                        RuntimeError::new(
                            ErrorCategory::HostError,
                            ctx.span,
                            format!("cannot add TLS root: {error}"),
                        )
                    })?;
                }
                if roots.is_empty() {
                    return Err(RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        "no trusted TLS roots are available",
                    ));
                }
                let config = rustls::ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth();
                let server_name =
                    rustls::pki_types::ServerName::try_from(host.clone()).map_err(|_| {
                        RuntimeError::new(
                            ErrorCategory::ResourceUnsupported,
                            ctx.span,
                            format!("`{host}` is not a valid TLS server name"),
                        )
                    })?;
                let connection =
                    rustls::ClientConnection::new(std::sync::Arc::new(config), server_name)
                        .map_err(|error| {
                            RuntimeError::new(
                                ErrorCategory::HostError,
                                ctx.span,
                                format!("cannot start TLS: {error}"),
                            )
                        })?;
                let stream = rustls::StreamOwned::new(connection, tcp);
                let handle = self.resources.open_tls(stream);
                self.observe_effect("resource.open", "ok", ctx);
                Ok(Value::Resource(handle))
            }
            "tcp-listener" => {
                if self.mode != SandboxMode::Project {
                    return Err(RuntimeError::new(
                        ErrorCategory::PermissionDenied,
                        ctx.span,
                        "tcp-listener resources are available only in project mode",
                    ));
                }
                if requested != BTreeSet::from(["accept"]) {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceUnsupported,
                        ctx.span,
                        "tcp-listener requires exactly [accept] in this build",
                    ));
                }
                let (host, port) = parse_host_port(descriptor, ctx.span)?;
                self.capabilities.check_net(&host, ctx.span)?;
                let addresses = (host.as_str(), port).to_socket_addrs().map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("cannot resolve `{host}:{port}`: {error}"),
                    )
                })?;
                let mut listener = None;
                let mut last_error = None;
                for address in addresses {
                    match TcpListener::bind(address) {
                        Ok(bound) => {
                            listener = Some(bound);
                            break;
                        }
                        Err(error) => last_error = Some((address, error)),
                    }
                }
                let Some(listener) = listener else {
                    let detail = match last_error {
                        Some((address, error)) => format!("{address}: {error}"),
                        None => format!("`{host}:{port}` resolved to no addresses"),
                    };
                    return Err(RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("cannot listen on `{host}:{port}`: {detail}"),
                    ));
                };
                listener.set_nonblocking(true).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("cannot configure tcp-listener: {error}"),
                    )
                })?;
                let handle = self.resources.open_tcp_listener(listener);
                self.observe_effect("resource.open", "ok", ctx);
                Ok(Value::Resource(handle))
            }
            "udp" => {
                if self.mode != SandboxMode::Project {
                    return Err(RuntimeError::new(
                        ErrorCategory::PermissionDenied,
                        ctx.span,
                        "udp resources are available only in project mode",
                    ));
                }
                if requested != BTreeSet::from(["read", "write"]) {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceUnsupported,
                        ctx.span,
                        "udp resources require exactly [read, write] in this build",
                    ));
                }
                let (host, port) = parse_host_port(descriptor, ctx.span)?;
                self.capabilities.check_net(&host, ctx.span)?;
                let addresses = (host.as_str(), port).to_socket_addrs().map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("cannot resolve `{host}:{port}`: {error}"),
                    )
                })?;
                let mut socket = None;
                let mut last_error = None;
                for address in addresses {
                    let bind = if address.is_ipv4() {
                        "0.0.0.0:0"
                    } else {
                        "[::]:0"
                    };
                    let candidate = UdpSocket::bind(bind).and_then(|socket| {
                        socket.connect(address)?;
                        Ok(socket)
                    });
                    match candidate {
                        Ok(connected) => {
                            socket = Some(connected);
                            break;
                        }
                        Err(error) => last_error = Some((address, error)),
                    }
                }
                let Some(socket) = socket else {
                    let detail = match last_error {
                        Some((address, error)) => format!("{address}: {error}"),
                        None => format!("`{host}:{port}` resolved to no addresses"),
                    };
                    return Err(RuntimeError::new(
                        ErrorCategory::HostError,
                        ctx.span,
                        format!("cannot connect udp to `{host}:{port}`: {detail}"),
                    ));
                };
                let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = socket.set_write_timeout(Some(Duration::from_secs(5)));
                let handle = self.resources.open_udp(socket);
                self.observe_effect("resource.open", "ok", ctx);
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
                self.observe_effect("resource.open", "ok", ctx);
                Ok(Value::Resource(handle))
            }
        }
    }
}
