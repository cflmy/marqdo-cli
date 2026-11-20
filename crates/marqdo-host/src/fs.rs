//! Project-scoped filesystem access.
//!
//! Paths are always project-relative. Absolute paths, `..` components and
//! symlink escapes outside the project root are rejected before the capability
//! check, and the capability check happens before any I/O.

use std::path::{Component, Path, PathBuf};

use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError};

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
        let bytes = std::fs::read(&canonical).map_err(|error| {
            RuntimeError::new(
                ErrorCategory::HostError,
                span,
                format!("cannot read `{relative}`: {error}"),
            )
        })?;
        String::from_utf8(bytes).map_err(|_| {
            RuntimeError::new(
                ErrorCategory::Type,
                span,
                format!("`{relative}` is not valid UTF-8"),
            )
        })
    }

    /// Write a file inside the project root, creating parent directories.
    pub fn write(
        &self,
        user_path: &str,
        contents: &str,
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
        std::fs::write(&absolute, contents).map_err(|error| {
            RuntimeError::new(
                ErrorCategory::HostError,
                span,
                format!("cannot write `{relative}`: {error}"),
            )
        })
    }
}

fn invalid_path(span: Option<Span>, message: impl Into<String>) -> RuntimeError {
    RuntimeError::new(ErrorCategory::PermissionDenied, span, message)
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
