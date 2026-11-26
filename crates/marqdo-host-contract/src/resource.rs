//! Values crossing the resource stream methods of [`crate::Host`].

/// Bytes returned by one stream read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRead {
    /// Bytes actually read (may be empty at EOF).
    pub bytes: Vec<u8>,
    /// Whether the stream has reached EOF.
    pub eof: bool,
}
