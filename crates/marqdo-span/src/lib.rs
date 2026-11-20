//! Byte source spans and line/column mapping shared by the Marqdo toolchain.
//!
//! Every syntax node and every diagnostic carries a [`Span`] expressed as byte
//! offsets into the original UTF-8 source file. Byte offsets are stable and
//! cheap to compare; human readable positions are derived on demand with a
//! [`LineIndex`]. This crate deliberately has no Marqdo dependencies so that
//! the Markdown layer can stay independent of the semantic layers.

use serde::{Deserialize, Serialize};

/// Half-open byte range `[start, end)` into the original source text.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct Span {
    /// Byte offset of the first byte of the span.
    pub start: usize,
    /// Byte offset one past the last byte of the span.
    pub end: usize,
}

impl Span {
    /// Create a span from half-open byte offsets.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The empty span at `offset`.
    pub const fn empty(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    /// Number of bytes covered by the span.
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Whether the span covers no bytes.
    pub const fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Smallest span covering both `self` and `other`.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Shift the span by `offset` bytes (used to re-base onto the original file
    /// after a BOM has been stripped).
    pub fn shift(self, offset: usize) -> Span {
        Span {
            start: self.start + offset,
            end: self.end + offset,
        }
    }

    /// Whether `self` contains `offset`.
    pub fn contains(&self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// 1-based line and column (column counted in bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LineCol {
    /// 1-based line number.
    pub line: usize,
    /// 1-based byte column.
    pub column: usize,
}

impl std::fmt::Display for LineCol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Precomputed line-start offsets for a source text.
#[derive(Debug, Clone, Default)]
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Build a line index for `source`.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }
        Self { line_starts }
    }

    /// Number of lines in the indexed source (a trailing newline does not add a
    /// phantom line).
    pub fn line_count(&self) -> usize {
        if self.line_starts.is_empty() {
            0
        } else {
            self.line_starts.len()
        }
    }

    /// Convert a byte offset into a 1-based line/column pair. Offsets past the
    /// end of the source clamp to the last line.
    pub fn line_col(&self, offset: usize) -> LineCol {
        let idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts.get(idx).copied().unwrap_or(0);
        LineCol {
            line: idx + 1,
            column: offset.saturating_sub(line_start) + 1,
        }
    }

    /// Byte offset of the first byte of a 1-based line.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.line_starts.get(line.checked_sub(1)?).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_maps_multibyte_text() {
        let src = "ab\n数学\nc";
        let idx = LineIndex::new(src);
        assert_eq!(idx.line_count(), 3);
        assert_eq!(idx.line_col(0), LineCol { line: 1, column: 1 });
        assert_eq!(idx.line_col(2), LineCol { line: 1, column: 3 });
        // '数' starts at byte 3.
        assert_eq!(idx.line_col(3), LineCol { line: 2, column: 1 });
        // '学' is 3 bytes later.
        assert_eq!(idx.line_col(6), LineCol { line: 2, column: 4 });
        // byte 7 is inside line 2 (the newline is byte 9).
        assert_eq!(idx.line_col(7), LineCol { line: 2, column: 5 });
        assert_eq!(idx.line_col(9), LineCol { line: 2, column: 7 });
        assert_eq!(idx.line_col(10), LineCol { line: 3, column: 1 });
        assert_eq!(idx.line_start(2), Some(3));
        assert_eq!(idx.line_start(0), None);
    }

    #[test]
    fn span_merge_and_shift() {
        let a = Span::new(4, 7);
        let b = Span::new(2, 5);
        assert_eq!(a.merge(b), Span::new(2, 7));
        assert_eq!(a.shift(3), Span::new(7, 10));
        assert!(a.contains(4));
        assert!(!a.contains(7));
        assert_eq!(Span::new(4, 7).len(), 3);
    }
}
