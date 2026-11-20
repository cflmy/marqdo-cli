//! Host-owned resource/stream table (ADR-0025).
//!
//! The stream table owns run-scoped stdio endpoints, bounded project-file
//! read/write streams, explicitly registered provider tokens and IP-literal TCP
//! streams. Handles are opaque and the table validates run identity, slot
//! identity and generation on every call. Bytes move only through `Memory`.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use marqdo_span::Span;
use marqdo_value::{ErrorCategory, ResourceHandle, RuntimeError};

static NEXT_RUN_ID: AtomicU64 = AtomicU64::new(1);

/// A versioned trusted provider of additional resource kinds (ADR-0033).
///
/// In-process providers share the host address space and are **not** a
/// sandbox; only explicitly trusted code may be registered.
pub trait ResourceProvider: Send {
    /// Stable grant name.
    fn name(&self) -> &str;
    /// Protocol version; must be 1 in this build.
    fn protocol_version(&self) -> u32 {
        1
    }
    /// Resource kinds served by this provider.
    fn kinds(&self) -> &[&str];

    /// Owned kind list used by registration. Providers with dynamically owned
    /// kind strings override this.
    fn kind_strings(&self) -> Vec<String> {
        self.kinds()
            .iter()
            .map(|kind| (*kind).to_string())
            .collect()
    }
    /// Open one descriptor, returning a provider-private token.
    fn open(
        &mut self,
        kind: &str,
        descriptor: &str,
        rights: &[String],
        span: Option<Span>,
    ) -> Result<u64, RuntimeError>;
    /// Read up to `max_bytes` from a token.
    fn read(
        &mut self,
        token: u64,
        max_bytes: usize,
        span: Option<Span>,
    ) -> Result<ResourceRead, RuntimeError>;
    /// Write bytes to a token; return the accepted count.
    fn write(
        &mut self,
        token: u64,
        bytes: &[u8],
        span: Option<Span>,
    ) -> Result<usize, RuntimeError>;
    /// Close a token.
    fn close(&mut self, token: u64, span: Option<Span>) -> Result<(), RuntimeError>;
}

/// Shared provider handle registered on the host.
pub type SharedResourceProvider = Arc<Mutex<Box<dyn ResourceProvider>>>;

/// Bytes returned by one stream read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRead {
    /// Bytes actually read (may be empty at EOF).
    pub bytes: Vec<u8>,
    /// Whether the stream has reached EOF.
    pub eof: bool,
}

enum Stream {
    Stdin {
        bytes: Vec<u8>,
        position: usize,
        closed: bool,
    },
    Stdout {
        closed: bool,
    },
    Stderr {
        closed: bool,
    },
    File {
        relative: String,
        bytes: Vec<u8>,
        position: usize,
        closed: bool,
    },
    FileWriter {
        relative: String,
        bytes: Vec<u8>,
        closed: bool,
    },
    Provider {
        kind: String,
        provider: SharedResourceProvider,
        token: u64,
        closed: bool,
    },
    Tcp {
        stream: TcpStream,
        closed: bool,
    },
}

struct Slot {
    generation: u64,
    stream: Stream,
}

impl Slot {
    fn closed(&self) -> bool {
        match &self.stream {
            Stream::Stdin { closed, .. } => *closed,
            Stream::Stdout { closed } => *closed,
            Stream::Stderr { closed } => *closed,
            Stream::File { closed, .. } => *closed,
            Stream::FileWriter { closed, .. } => *closed,
            Stream::Provider { closed, .. } => *closed,
            Stream::Tcp { closed, .. } => *closed,
        }
    }
}

/// Host resource table for one run.
pub struct ResourceTable {
    run_id: u64,
    next_resource: u64,
    slots: BTreeMap<u64, Slot>,
}

impl Default for ResourceTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceTable {
    /// Create an empty table with a fresh run identity.
    pub fn new() -> Self {
        Self {
            run_id: NEXT_RUN_ID.fetch_add(1, Ordering::Relaxed),
            next_resource: 1,
            slots: BTreeMap::new(),
        }
    }

    /// Number of live (not necessarily open) slots.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Register the pre-opened stdout stream.
    pub fn open_stdout(&mut self) -> ResourceHandle {
        self.insert(Stream::Stdout { closed: false })
    }

    /// Register the pre-opened stderr stream.
    pub fn open_stderr(&mut self) -> ResourceHandle {
        self.insert(Stream::Stderr { closed: false })
    }

    /// Register a run-scoped stdin stream over already supplied bytes.
    ///
    /// The host supplies bytes at assembly time; `read` chunks them with EOF.
    /// There is no blocking OS read and no hidden second input channel here.
    pub fn open_stdin(&mut self, bytes: Vec<u8>) -> ResourceHandle {
        self.insert(Stream::Stdin {
            bytes,
            position: 0,
            closed: false,
        })
    }

    /// Return the stdio descriptor name for a handle, if it is a stdio stream.
    pub fn stdio_name(
        &self,
        handle: ResourceHandle,
        span: Option<Span>,
    ) -> Result<Option<&'static str>, RuntimeError> {
        let slot = self.slot(handle, span)?;
        Ok(match &slot.stream {
            Stream::Stdin { .. } => Some("stdin"),
            Stream::Stdout { .. } => Some("stdout"),
            Stream::Stderr { .. } => Some("stderr"),
            Stream::File { .. } | Stream::FileWriter { .. } => None,
            Stream::Provider { .. } | Stream::Tcp { .. } => None,
        })
    }

    /// Register a project-file read stream opened from already-bounded bytes.
    pub fn open_file(&mut self, relative: impl Into<String>, bytes: Vec<u8>) -> ResourceHandle {
        self.insert(Stream::File {
            relative: relative.into(),
            bytes,
            position: 0,
            closed: false,
        })
    }

    /// Register an empty buffered project-file write stream.
    pub fn open_file_writer(&mut self, relative: impl Into<String>) -> ResourceHandle {
        self.insert(Stream::FileWriter {
            relative: relative.into(),
            bytes: Vec::new(),
            closed: false,
        })
    }

    /// Register a connected TCP stream.
    pub fn open_tcp(&mut self, stream: TcpStream) -> ResourceHandle {
        self.insert(Stream::Tcp {
            stream,
            closed: false,
        })
    }

    /// Open a provider token and wrap it in a host-issued handle.
    pub fn open_provider(
        &mut self,
        provider: SharedResourceProvider,
        kind: &str,
        descriptor: &str,
        rights: &[String],
        span: Option<Span>,
    ) -> Result<ResourceHandle, RuntimeError> {
        let token = {
            let mut provider = provider.lock().map_err(|_| provider_lock_error(span))?;
            provider.open(kind, descriptor, rights, span)?
        };
        Ok(self.insert(Stream::Provider {
            kind: kind.to_string(),
            provider,
            token,
            closed: false,
        }))
    }

    fn insert(&mut self, stream: Stream) -> ResourceHandle {
        let id = self.next_resource;
        self.next_resource += 1;
        self.slots.insert(
            id,
            Slot {
                generation: 1,
                stream,
            },
        );
        ResourceHandle::from_host(self.run_id, id, 1)
    }

    fn slot(&self, handle: ResourceHandle, span: Option<Span>) -> Result<&Slot, RuntimeError> {
        if handle.run_id() != self.run_id {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceFault,
                span,
                "Resource handle belongs to a different host run",
            ));
        }
        let Some(slot) = self.slots.get(&handle.resource_id()) else {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceFault,
                span,
                "unknown Resource handle",
            ));
        };
        if slot.generation != handle.generation() {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceFault,
                span,
                "stale Resource handle",
            ));
        }
        Ok(slot)
    }

    fn slot_mut(
        &mut self,
        handle: ResourceHandle,
        span: Option<Span>,
    ) -> Result<&mut Slot, RuntimeError> {
        self.slot(handle, span)?;
        self.slots
            .get_mut(&handle.resource_id())
            .ok_or_else(|| unknown_handle(span))
    }

    /// Read up to `max_bytes` from a file stream.
    ///
    /// Reads at EOF return `count=0, eof=true`. A read on a closed stream is
    /// `resource.closed`; stdout is not readable.
    pub fn read(
        &mut self,
        handle: ResourceHandle,
        max_bytes: usize,
        span: Option<Span>,
    ) -> Result<ResourceRead, RuntimeError> {
        let slot = self.slot_mut(handle, span)?;
        if slot.closed() {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceClosed,
                span,
                "Resource handle is closed",
            ));
        }
        match &mut slot.stream {
            Stream::Stdin {
                bytes, position, ..
            }
            | Stream::File {
                bytes, position, ..
            } => {
                let start = *position;
                let end = start.saturating_add(max_bytes).min(bytes.len());
                let chunk = bytes[start..end].to_vec();
                *position = end;
                Ok(ResourceRead {
                    bytes: chunk,
                    eof: end >= bytes.len(),
                })
            }
            Stream::Stdout { .. } => Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                span,
                "stdout does not support resource.read",
            )),
            Stream::Stderr { .. } => Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                span,
                "stderr does not support resource.read",
            )),
            Stream::FileWriter { .. } => Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                span,
                "a file write stream does not support resource.read",
            )),
            Stream::Provider {
                provider, token, ..
            } => {
                let mut provider = provider.lock().map_err(|_| provider_lock_error(span))?;
                provider.read(*token, max_bytes, span)
            }
            Stream::Tcp { stream, .. } => {
                let mut buffer = vec![0u8; max_bytes];
                let count = stream.read(&mut buffer).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("tcp read failed: {error}"),
                    )
                })?;
                buffer.truncate(count);
                Ok(ResourceRead {
                    bytes: buffer,
                    eof: count == 0,
                })
            }
        }
    }

    /// Validate a stdout write and return the accepted byte count.
    ///
    /// The caller performs the output-budget accounting and buffer append; this
    /// method only checks handle identity, closed state and stream kind.
    pub fn write(
        &mut self,
        handle: ResourceHandle,
        bytes: &[u8],
        span: Option<Span>,
    ) -> Result<usize, RuntimeError> {
        let slot = self.slot_mut(handle, span)?;
        if slot.closed() {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceClosed,
                span,
                "Resource handle is closed",
            ));
        }
        match &mut slot.stream {
            Stream::Stdout { .. } | Stream::Stderr { .. } => Ok(bytes.len()),
            Stream::Stdin { .. } => Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                span,
                "stdin is a read-only resource stream",
            )),
            Stream::File { relative, .. } => Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                span,
                format!("project file `{relative}` was opened read-only"),
            )),
            Stream::FileWriter {
                relative,
                bytes: buffer,
                ..
            } => {
                let next = buffer.len().checked_add(bytes.len());
                let Some(next) = next else {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceExhausted,
                        span,
                        format!("file write buffer for `{relative}` overflows"),
                    ));
                };
                if next > crate::fs::MAX_READ_BYTES as usize {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceExhausted,
                        span,
                        format!(
                            "file write buffer for `{relative}` exceeds the {} byte limit",
                            crate::fs::MAX_READ_BYTES
                        ),
                    ));
                }
                buffer.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            Stream::Provider {
                provider, token, ..
            } => {
                let mut provider = provider.lock().map_err(|_| provider_lock_error(span))?;
                provider.write(*token, bytes, span)
            }
            Stream::Tcp { stream, .. } => stream.write(bytes).map_err(|error| {
                RuntimeError::new(
                    ErrorCategory::HostError,
                    span,
                    format!("tcp write failed: {error}"),
                )
            }),
        }
    }

    /// Close a stream. A second close is `resource.closed`.
    pub fn close(
        &mut self,
        handle: ResourceHandle,
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        let slot = self.slot_mut(handle, span)?;
        if slot.closed() {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceClosed,
                span,
                "Resource handle is already closed",
            ));
        }
        match &mut slot.stream {
            Stream::Stdin { closed, .. } => *closed = true,
            Stream::Stdout { closed } => *closed = true,
            Stream::Stderr { closed } => *closed = true,
            Stream::File { closed, .. } => *closed = true,
            Stream::FileWriter { closed, .. } => *closed = true,
            Stream::Provider {
                provider,
                token,
                closed,
                ..
            } => {
                let mut provider = provider.lock().map_err(|_| provider_lock_error(span))?;
                provider.close(*token, span)?;
                *closed = true;
            }
            Stream::Tcp { stream, closed } => {
                let _ = stream.shutdown(std::net::Shutdown::Both);
                *closed = true;
            }
        }
        Ok(())
    }

    /// Return the buffered bytes of a file write stream without closing it.
    ///
    /// `None` means the handle is not a file writer (stdout or a reader). The
    /// caller must call [`Self::close`] only after the flush succeeds, so a
    /// failed flush leaves the writer open with its buffer intact.
    pub fn pending_file_write(
        &self,
        handle: ResourceHandle,
        span: Option<Span>,
    ) -> Result<Option<(String, Vec<u8>)>, RuntimeError> {
        let slot = self.slot(handle, span)?;
        if slot.closed() {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceClosed,
                span,
                "Resource handle is closed",
            ));
        }
        match &slot.stream {
            Stream::FileWriter {
                relative, bytes, ..
            } => Ok(Some((relative.clone(), bytes.clone()))),
            _ => Ok(None),
        }
    }

    /// Descriptor kind for diagnostics/trace, if the handle is valid.
    pub fn kind(&self, handle: ResourceHandle, span: Option<Span>) -> Result<String, RuntimeError> {
        let slot = self.slot(handle, span)?;
        Ok(match &slot.stream {
            Stream::Stdin { .. } | Stream::Stdout { .. } | Stream::Stderr { .. } => {
                "stdio".to_string()
            }
            Stream::File { .. } | Stream::FileWriter { .. } => "file".to_string(),
            Stream::Provider { kind, .. } => kind.clone(),
            Stream::Tcp { .. } => "tcp".to_string(),
        })
    }
}

fn provider_lock_error(span: Option<Span>) -> RuntimeError {
    RuntimeError::new(
        ErrorCategory::HostError,
        span,
        "resource provider lock is poisoned",
    )
}

fn unknown_handle(span: Option<Span>) -> RuntimeError {
    RuntimeError::new(
        ErrorCategory::ResourceFault,
        span,
        "unknown Resource handle",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_reads_are_chunked_with_eof() {
        let mut table = ResourceTable::new();
        let handle = table.open_file("data.txt", b"abcdef".to_vec());
        assert_eq!(
            table.read(handle, 2, None).unwrap(),
            ResourceRead {
                bytes: b"ab".to_vec(),
                eof: false
            }
        );
        assert_eq!(
            table.read(handle, 8, None).unwrap(),
            ResourceRead {
                bytes: b"cdef".to_vec(),
                eof: true
            }
        );
        assert_eq!(
            table.read(handle, 1, None).unwrap(),
            ResourceRead {
                bytes: Vec::new(),
                eof: true
            }
        );
    }

    #[test]
    fn forged_and_closed_handles_are_rejected() {
        let mut table = ResourceTable::new();
        let handle = table.open_stdout();
        let forged = ResourceHandle::from_host(handle.run_id() + 1, handle.resource_id(), 1);
        assert_eq!(
            table.write(forged, b"x", None).unwrap_err().code(),
            "runtime.resource"
        );
        table.close(handle, None).unwrap();
        assert_eq!(
            table.write(handle, b"x", None).unwrap_err().code(),
            "resource.closed"
        );
        assert_eq!(
            table.close(handle, None).unwrap_err().code(),
            "resource.closed"
        );
    }

    #[test]
    fn stdout_is_not_readable_and_file_is_not_writable() {
        let mut table = ResourceTable::new();
        let stdout = table.open_stdout();
        assert_eq!(
            table.read(stdout, 1, None).unwrap_err().code(),
            "resource.unsupported"
        );
        let file = table.open_file("x.txt", Vec::new());
        assert_eq!(
            table.write(file, b"x", None).unwrap_err().code(),
            "resource.unsupported"
        );
    }

    #[test]
    fn stdin_reads_are_chunked_and_end_at_eof() {
        let mut table = ResourceTable::new();
        let stdin = table.open_stdin(b"ab".to_vec());
        assert_eq!(
            table.read(stdin, 1, None).unwrap(),
            ResourceRead {
                bytes: b"a".to_vec(),
                eof: false
            }
        );
        assert_eq!(
            table.read(stdin, 8, None).unwrap(),
            ResourceRead {
                bytes: b"b".to_vec(),
                eof: true
            }
        );
        assert_eq!(
            table.read(stdin, 1, None).unwrap(),
            ResourceRead {
                bytes: Vec::new(),
                eof: true
            }
        );
        assert_eq!(
            table.write(stdin, b"x", None).unwrap_err().code(),
            "resource.unsupported"
        );
        table.close(stdin, None).unwrap();
        assert_eq!(
            table.read(stdin, 1, None).unwrap_err().code(),
            "resource.closed"
        );
    }

    #[test]
    fn stderr_is_write_only() {
        let mut table = ResourceTable::new();
        let stderr = table.open_stderr();
        assert_eq!(table.write(stderr, b"diagnostic", None).unwrap(), 10);
        assert_eq!(
            table.read(stderr, 1, None).unwrap_err().code(),
            "resource.unsupported"
        );
        assert_eq!(table.stdio_name(stderr, None).unwrap(), Some("stderr"));
        table.close(stderr, None).unwrap();
        assert_eq!(
            table.write(stderr, b"x", None).unwrap_err().code(),
            "resource.closed"
        );
    }
}
