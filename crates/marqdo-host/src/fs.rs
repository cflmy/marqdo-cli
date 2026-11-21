//! Project-scoped filesystem access.
//!
//! Paths are always project-relative. Absolute paths, `..` components and
//! symlink escapes outside the project root are rejected before the capability
//! check, and the capability check happens before any I/O.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError, Value};
use std::collections::BTreeMap;

/// Maximum size for a single file read.
pub const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;

/// A filesystem view rooted at the project directory.
#[derive(Debug, Clone)]
pub struct ProjectFs {
    root: PathBuf,
}

impl ProjectFs {
    /// Create a filesystem view. The root is canonicalised when possible.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let root = root.canonicalize().unwrap_or(root);
        Self { root }
    }

    /// Project root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Normalise a user supplied project-relative path.
    ///
    /// Returns the normalised relative string (with `/` separators) used for
    /// capability matching and the absolute path inside the project root.
    pub fn resolve(
        &self,
        user_path: &str,
        span: Option<Span>,
    ) -> Result<(PathBuf, String), RuntimeError> {
        if user_path.is_empty() {
            return Err(invalid_path(span, "path must not be empty"));
        }
        let path = Path::new(user_path);
        if path.is_absolute() {
            return Err(invalid_path(
                span,
                format!("absolute paths are not allowed: `{user_path}`"),
            ));
        }
        let mut normalised = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => normalised.push(part),
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(invalid_path(
                        span,
                        format!("path traversal is not allowed: `{user_path}`"),
                    ));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(invalid_path(
                        span,
                        format!("absolute paths are not allowed: `{user_path}`"),
                    ));
                }
            }
        }
        if normalised.as_os_str().is_empty() {
            return Err(invalid_path(span, "path must name a file"));
        }
        let relative = normalised
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        let absolute = self.root.join(&normalised);
        Ok((absolute, relative))
    }

    /// Read a UTF-8 file, bounded by [`MAX_READ_BYTES`].
    pub fn read(&self, user_path: &str, span: Option<Span>) -> Result<String, RuntimeError> {
        let bytes = self.read_bytes(user_path, span)?;
        String::from_utf8(bytes).map_err(|_| {
            RuntimeError::new(
                ErrorCategory::Type,
                span,
                format!("`{user_path}` is not valid UTF-8"),
            )
        })
    }

    /// Read a file as raw bytes, bounded by [`MAX_READ_BYTES`].
    ///
    /// Used by the resource stream provider; path/capability checks are the
    /// caller's responsibility for grants, while this method still rejects
    /// traversal and symlink escapes.
    pub fn read_bytes(&self, user_path: &str, span: Option<Span>) -> Result<Vec<u8>, RuntimeError> {
        let (absolute, relative) = self.resolve(user_path, span)?;
        let metadata = std::fs::metadata(&absolute).map_err(|error| {
            RuntimeError::new(
                ErrorCategory::HostError,
                span,
                format!("cannot read `{relative}`: {error}"),
            )
        })?;
        if !metadata.is_file() {
            return Err(RuntimeError::new(
                ErrorCategory::HostError,
                span,
                format!("`{relative}` is not a regular file"),
            ));
        }
        if metadata.len() > MAX_READ_BYTES {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!(
                    "`{relative}` exceeds the {} byte read limit",
                    MAX_READ_BYTES
                ),
            ));
        }
        let canonical = absolute.canonicalize().map_err(|error| {
            RuntimeError::new(
                ErrorCategory::HostError,
                span,
                format!("cannot resolve `{relative}`: {error}"),
            )
        })?;
        if !canonical.starts_with(&self.root) {
            return Err(invalid_path(span, "path escapes the project root"));
        }
        std::fs::read(&canonical).map_err(|error| {
            RuntimeError::new(
                ErrorCategory::HostError,
                span,
                format!("cannot read `{relative}`: {error}"),
            )
        })
    }

    /// Execute a path operation on the project filesystem.
    ///
    /// This is the host side of `resource.request`; it validates every path
    /// against the project root, then returns the recoverable `Result` map.
    /// Capability checks are performed by the host before this method is
    /// called. Unknown operations are a programming error and stay fatal.
    pub fn request(
        &self,
        operation: &str,
        args: &[String],
        span: Option<Span>,
    ) -> Result<Value, RuntimeError> {
        match operation {
            "exists" => {
                let (absolute, relative) = self.resolve(one_arg(operation, args, span)?, span)?;
                if !absolute.exists() {
                    return Ok(fs_ok(Value::Bool(false)));
                }
                self.ensure_inside(&absolute, &relative, span)?;
                Ok(fs_ok(Value::Bool(true)))
            }
            "metadata" => {
                let (absolute, relative) = self.resolve(one_arg(operation, args, span)?, span)?;
                if !absolute.exists() {
                    return Ok(fs_error(
                        "fs.not_found",
                        &format!("`{relative}` does not exist"),
                    ));
                }
                let absolute = self.ensure_inside(&absolute, &relative, span)?;
                let metadata = std::fs::metadata(&absolute).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot stat `{relative}`: {error}"),
                    )
                })?;
                let kind = if metadata.is_file() {
                    "file"
                } else if metadata.is_dir() {
                    "directory"
                } else {
                    "other"
                };
                let size = if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                };
                let mut value = BTreeMap::new();
                value.insert("kind".to_string(), Value::text(kind));
                value.insert(
                    "size".to_string(),
                    Value::int(size.min(i64::MAX as u64) as i64),
                );
                Ok(fs_ok(Value::Map(value)))
            }
            "list" => {
                let (absolute, relative) = self.resolve(one_arg(operation, args, span)?, span)?;
                if !absolute.exists() {
                    return Ok(fs_error(
                        "fs.not_found",
                        &format!("`{relative}` does not exist"),
                    ));
                }
                let absolute = self.ensure_inside(&absolute, &relative, span)?;
                let metadata = std::fs::metadata(&absolute).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot stat `{relative}`: {error}"),
                    )
                })?;
                if !metadata.is_dir() {
                    return Ok(fs_error(
                        "fs.not_directory",
                        &format!("`{relative}` is not a directory"),
                    ));
                }
                let mut names = Vec::new();
                let entries = std::fs::read_dir(&absolute).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot list `{relative}`: {error}"),
                    )
                })?;
                for entry in entries {
                    let entry = entry.map_err(|error| {
                        RuntimeError::new(
                            ErrorCategory::HostError,
                            span,
                            format!("cannot list `{relative}`: {error}"),
                        )
                    })?;
                    names.push(entry.file_name().to_string_lossy().to_string());
                }
                names.sort();
                Ok(fs_ok(Value::List(
                    names.into_iter().map(Value::text).collect(),
                )))
            }
            "mkdir" => {
                let (absolute, relative) = self.resolve(one_arg(operation, args, span)?, span)?;
                self.ensure_parent_inside(&absolute, &relative, span)?;
                if absolute.exists() {
                    return Ok(fs_error(
                        "fs.exists",
                        &format!("`{relative}` already exists"),
                    ));
                }
                match std::fs::create_dir(&absolute) {
                    Ok(()) => Ok(fs_ok(Value::None)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(fs_error(
                        "fs.not_found",
                        &format!("parent directory of `{relative}` does not exist"),
                    )),
                    Err(error) => Err(RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot create directory `{relative}`: {error}"),
                    )),
                }
            }
            "remove" => {
                let (absolute, relative) = self.resolve(one_arg(operation, args, span)?, span)?;
                if !absolute.exists() {
                    return Ok(fs_error(
                        "fs.not_found",
                        &format!("`{relative}` does not exist"),
                    ));
                }
                let absolute = self.ensure_inside(&absolute, &relative, span)?;
                let metadata = std::fs::metadata(&absolute).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot stat `{relative}`: {error}"),
                    )
                })?;
                if metadata.is_dir() {
                    let mut entries = std::fs::read_dir(&absolute).map_err(|error| {
                        RuntimeError::new(
                            ErrorCategory::HostError,
                            span,
                            format!("cannot list `{relative}`: {error}"),
                        )
                    })?;
                    if entries.next().is_some() {
                        return Ok(fs_error(
                            "fs.not_empty",
                            &format!("`{relative}` is not empty"),
                        ));
                    }
                    std::fs::remove_dir(&absolute).map_err(|error| {
                        RuntimeError::new(
                            ErrorCategory::HostError,
                            span,
                            format!("cannot remove `{relative}`: {error}"),
                        )
                    })?;
                } else {
                    std::fs::remove_file(&absolute).map_err(|error| {
                        let category = if error.kind() == std::io::ErrorKind::PermissionDenied {
                            ErrorCategory::PermissionDenied
                        } else {
                            ErrorCategory::HostError
                        };
                        RuntimeError::new(
                            category,
                            span,
                            format!("cannot remove `{relative}`: {error}"),
                        )
                    })?;
                }
                Ok(fs_ok(Value::None))
            }
            "append" => {
                if args.len() != 2 {
                    return Err(fs_arity(operation, 2, args.len(), span));
                }
                let (absolute, relative) = self.resolve(&args[0], span)?;
                self.ensure_parent_inside(&absolute, &relative, span)?;
                let bytes = args[1].as_bytes();
                if bytes.len() > MAX_READ_BYTES as usize {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceExhausted,
                        span,
                        format!(
                            "append to `{relative}` exceeds the {} byte limit",
                            MAX_READ_BYTES
                        ),
                    ));
                }
                let mut file = std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&absolute)
                    .map_err(|error| {
                        RuntimeError::new(
                            ErrorCategory::HostError,
                            span,
                            format!("cannot append to `{relative}`: {error}"),
                        )
                    })?;
                file.write_all(bytes).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot append to `{relative}`: {error}"),
                    )
                })?;
                Ok(fs_ok(Value::int(bytes.len() as i64)))
            }
            "read_at" => {
                if args.len() != 3 {
                    return Err(fs_arity(operation, 3, args.len(), span));
                }
                let (absolute, relative) = self.resolve(&args[0], span)?;
                let offset = parse_offset(&args[1], span)?;
                let length = parse_offset(&args[2], span)?;
                if length > MAX_READ_BYTES {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceExhausted,
                        span,
                        format!("read_at length exceeds the {} byte limit", MAX_READ_BYTES),
                    ));
                }
                let mut file = std::fs::File::open(&absolute).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot read `{relative}`: {error}"),
                    )
                })?;
                file.seek(SeekFrom::Start(offset)).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot seek `{relative}`: {error}"),
                    )
                })?;
                let mut buffer = Vec::new();
                file.take(length)
                    .read_to_end(&mut buffer)
                    .map_err(|error| {
                        RuntimeError::new(
                            ErrorCategory::HostError,
                            span,
                            format!("cannot read `{relative}`: {error}"),
                        )
                    })?;
                let text = String::from_utf8(buffer).map_err(|_| {
                    RuntimeError::new(
                        ErrorCategory::Type,
                        span,
                        format!("`{relative}` slice is not valid UTF-8"),
                    )
                })?;
                Ok(fs_ok(Value::text(text)))
            }
            "walk" => {
                let (absolute, relative) = self.resolve(one_arg(operation, args, span)?, span)?;
                if !absolute.exists() {
                    return Ok(fs_error(
                        "fs.not_found",
                        &format!("`{relative}` does not exist"),
                    ));
                }
                let absolute = self.ensure_inside(&absolute, &relative, span)?;
                let metadata = std::fs::metadata(&absolute).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot stat `{relative}`: {error}"),
                    )
                })?;
                if !metadata.is_dir() {
                    return Ok(fs_error(
                        "fs.not_directory",
                        &format!("`{relative}` is not a directory"),
                    ));
                }
                let mut entries = Vec::new();
                walk_directory(&absolute, "", &mut entries, 0).map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot walk `{relative}`: {error}"),
                    )
                })?;
                if entries.len() > 4096 {
                    return Err(RuntimeError::new(
                        ErrorCategory::ResourceExhausted,
                        span,
                        "walk exceeds the 4096 entry limit",
                    ));
                }
                Ok(fs_ok(Value::List(
                    entries.into_iter().map(Value::text).collect(),
                )))
            }
            "rename" => {
                if args.len() != 2 {
                    return Err(fs_arity(operation, 2, args.len(), span));
                }
                let (from_absolute, from) = self.resolve(&args[0], span)?;
                let (to_absolute, to) = self.resolve(&args[1], span)?;
                if !from_absolute.exists() {
                    return Ok(fs_error(
                        "fs.not_found",
                        &format!("`{from}` does not exist"),
                    ));
                }
                let from_absolute = self.ensure_inside(&from_absolute, &from, span)?;
                self.ensure_parent_inside(&to_absolute, &to, span)?;
                if to_absolute.exists() {
                    return Ok(fs_error("fs.exists", &format!("`{to}` already exists")));
                }
                match std::fs::rename(&from_absolute, &to_absolute) {
                    Ok(()) => Ok(fs_ok(Value::None)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(fs_error(
                        "fs.not_found",
                        &format!("parent directory of `{to}` does not exist"),
                    )),
                    Err(error) => Err(RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot rename `{from}` to `{to}`: {error}"),
                    )),
                }
            }
            other => Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                span,
                format!("unsupported filesystem operation `{other}`"),
            )),
        }
    }

    fn ensure_inside(
        &self,
        absolute: &Path,
        relative: &str,
        span: Option<Span>,
    ) -> Result<PathBuf, RuntimeError> {
        let canonical = absolute.canonicalize().map_err(|error| {
            RuntimeError::new(
                ErrorCategory::HostError,
                span,
                format!("cannot resolve `{relative}`: {error}"),
            )
        })?;
        if !canonical.starts_with(&self.root) {
            return Err(invalid_path(span, "path escapes the project root"));
        }
        Ok(canonical)
    }

    fn ensure_parent_inside(
        &self,
        absolute: &Path,
        relative: &str,
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        let mut ancestor = absolute.parent();
        while let Some(current) = ancestor {
            if current.exists() {
                let canonical = current.canonicalize().map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot resolve `{relative}`: {error}"),
                    )
                })?;
                if !canonical.starts_with(&self.root) {
                    return Err(invalid_path(span, "path escapes the project root"));
                }
                return Ok(());
            }
            ancestor = current.parent();
        }
        Ok(())
    }

    /// Write a file inside the project root, creating parent directories.
    pub fn write(
        &self,
        user_path: &str,
        contents: &str,
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        self.write_bytes(user_path, contents.as_bytes(), span)
    }

    /// Write raw bytes inside the project root, creating parent directories.
    ///
    /// This is a single validated write, not a transactional replace.
    pub fn write_bytes(
        &self,
        user_path: &str,
        bytes: &[u8],
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        let (absolute, relative) = self.resolve(user_path, span)?;
        // Validate the closest existing ancestor stays inside the root.
        let mut ancestor = absolute.parent();
        while let Some(current) = ancestor {
            if current.exists() {
                let canonical = current.canonicalize().map_err(|error| {
                    RuntimeError::new(
                        ErrorCategory::HostError,
                        span,
                        format!("cannot resolve `{relative}`: {error}"),
                    )
                })?;
                if !canonical.starts_with(&self.root) {
                    return Err(invalid_path(span, "path escapes the project root"));
                }
                break;
            }
            ancestor = current.parent();
        }
        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                RuntimeError::new(
                    ErrorCategory::HostError,
                    span,
                    format!("cannot create parent directory for `{relative}`: {error}"),
                )
            })?;
        }
        std::fs::write(&absolute, bytes).map_err(|error| {
            RuntimeError::new(
                ErrorCategory::HostError,
                span,
                format!("cannot write `{relative}`: {error}"),
            )
        })
    }
}

fn parse_offset(raw: &str, span: Option<Span>) -> Result<u64, RuntimeError> {
    raw.parse::<u64>().map_err(|_| {
        RuntimeError::new(
            ErrorCategory::Type,
            span,
            format!("`{raw}` is not a non-negative integer offset"),
        )
    })
}

fn walk_directory(
    dir: &Path,
    prefix: &str,
    out: &mut Vec<String>,
    depth: usize,
) -> std::io::Result<()> {
    if depth > 64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "directory depth exceeds 64",
        ));
    }
    let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if file_type.is_dir() {
            walk_directory(&entry.path(), &relative, out, depth + 1)?;
        } else {
            out.push(relative);
        }
    }
    Ok(())
}

fn invalid_path(span: Option<Span>, message: impl Into<String>) -> RuntimeError {
    RuntimeError::new(ErrorCategory::PermissionDenied, span, message)
}

fn one_arg<'a>(
    operation: &str,
    args: &'a [String],
    span: Option<Span>,
) -> Result<&'a str, RuntimeError> {
    if args.len() != 1 {
        return Err(fs_arity(operation, 1, args.len(), span));
    }
    Ok(&args[0])
}

fn fs_arity(operation: &str, expected: usize, actual: usize, span: Option<Span>) -> RuntimeError {
    RuntimeError::new(
        ErrorCategory::Arity,
        span,
        format!("filesystem operation `{operation}` expects {expected} argument(s), got {actual}"),
    )
}

fn fs_ok(value: Value) -> Value {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(true));
    map.insert("value".to_string(), value);
    Value::Map(map)
}

fn fs_error(code: &str, message: &str) -> Value {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(false));
    map.insert("code".to_string(), Value::text(code));
    map.insert("message".to_string(), Value::text(message));
    Value::Map(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        let dir = tempfile::tempdir().unwrap();
        let fs = ProjectFs::new(dir.path());
        assert!(fs.resolve("../etc/passwd", None).is_err());
        assert!(fs.resolve("/etc/passwd", None).is_err());
        assert!(fs.resolve("data/../x.txt", None).is_err());
        assert!(fs.resolve("", None).is_err());
        assert_eq!(fs.resolve("./data/a.txt", None).unwrap().1, "data/a.txt");
    }

    #[test]
    fn reads_and_writes_inside_root() {
        let dir = tempfile::tempdir().unwrap();
        let fs = ProjectFs::new(dir.path());
        fs.write("output/result.txt", "hello", None).unwrap();
        assert_eq!(fs.read("output/result.txt", None).unwrap(), "hello");
    }
}
