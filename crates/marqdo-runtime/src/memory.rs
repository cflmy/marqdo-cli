//! Runtime-owned addressable byte regions (RFC-CM-0001 / ADR-0022).
//!
//! A [`MemoryStore`] owns every region created during one run. Language values
//! carry only an opaque [`MemoryHandle`]; offsets are region-relative integers
//! and are never host pointers. Region identity is issued here and cannot be
//! forged from a document, a number or wire data.
//!
//! Semantics frozen by ADR-0022:
//!
//! * a region is a byte array with a 64 KiB logical page size; new bytes are
//!   zero-filled;
//! * regions grow only, never shrink; a failed grow leaves the region and the
//!   committed budget unchanged;
//! * copies of a `Memory` value share one region, while different regions
//!   never alias;
//! * every access is bounds-checked against the region and the run budget, and
//!   overflow in offset/page arithmetic is a structured error, never a panic;
//! * regions live for the whole run and are dropped with the store. There is
//!   no user-visible `free(region)`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use marqdo_span::Span;
use marqdo_value::{ErrorCategory, MemoryHandle, RuntimeError};
use num_bigint::BigInt;
use num_traits::ToPrimitive;

/// Logical page size for every region.
pub const PAGE_SIZE: usize = 64 * 1024;

/// Default maximum committed bytes across all regions in one run (64 MiB).
pub const DEFAULT_MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// Default maximum committed bytes for one region (16 MiB).
pub const DEFAULT_MAX_REGION_BYTES: usize = 16 * 1024 * 1024;

static NEXT_RUN_ID: AtomicU64 = AtomicU64::new(1);

/// Bounds for addressable memory in one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryLimits {
    /// Maximum bytes committed across all live regions.
    pub max_total_bytes: usize,
    /// Maximum bytes committed by a single region (`max_pages` is further
    /// capped by this value).
    pub max_region_bytes: usize,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            max_region_bytes: DEFAULT_MAX_REGION_BYTES,
        }
    }
}

impl MemoryLimits {
    /// Maximum page count a single region may declare or reach.
    pub fn max_region_pages(&self) -> u64 {
        (self.max_region_bytes / PAGE_SIZE) as u64
    }
}

struct Region {
    bytes: Vec<u8>,
    /// Declared maximum size for this region.
    max_bytes: usize,
    generation: u64,
}

/// All addressable regions created by one run.
pub struct MemoryStore {
    run_id: u64,
    next_region: u64,
    regions: BTreeMap<u64, Region>,
    committed_bytes: usize,
    limits: MemoryLimits,
}

impl MemoryStore {
    /// Create an empty store with a fresh, unguessable run identity.
    pub fn new(limits: MemoryLimits) -> Self {
        Self {
            run_id: NEXT_RUN_ID.fetch_add(1, Ordering::Relaxed),
            next_region: 1,
            regions: BTreeMap::new(),
            committed_bytes: 0,
            limits,
        }
    }

    /// Limits in force for this store.
    pub fn limits(&self) -> MemoryLimits {
        self.limits
    }

    /// Bytes currently committed across all regions.
    pub fn committed_bytes(&self) -> usize {
        self.committed_bytes
    }

    /// Number of live regions.
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Create a zero-filled region.
    ///
    /// `initial_pages` is committed immediately; `max_pages` is the declared
    /// capacity that `grow` may not exceed. Both are checked against the
    /// region and total budgets before anything is allocated, so a rejected
    /// create leaves the store unchanged.
    pub fn create(
        &mut self,
        initial_pages: u64,
        max_pages: u64,
        span: Option<Span>,
    ) -> Result<MemoryHandle, RuntimeError> {
        if initial_pages > max_pages {
            return Err(memory_error(
                span,
                format!(
                    "memory.new initial_pages ({initial_pages}) must not exceed max_pages \
                     ({max_pages})"
                ),
            ));
        }
        if max_pages > self.limits.max_region_pages() {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!(
                    "memory.new max_pages ({max_pages}) exceeds the per-region limit of {} \
                     pages ({PAGE_SIZE} bytes each)",
                    self.limits.max_region_pages()
                ),
            ));
        }
        let max_bytes = checked_bytes(max_pages, span, "region capacity")?;
        if max_bytes > self.limits.max_region_bytes {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!(
                    "memory.new max_pages ({max_pages}) exceeds the per-region byte budget of {}",
                    self.limits.max_region_bytes
                ),
            ));
        }
        let initial_bytes = checked_bytes(initial_pages, span, "initial region size")?;
        let committed = self
            .committed_bytes
            .checked_add(initial_bytes)
            .ok_or_else(|| budget_error(span, "memory budget overflow"))?;
        if committed > self.limits.max_total_bytes {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!(
                    "memory budget exceeded: committing {initial_bytes} bytes would reach \
                     {committed} of {}",
                    self.limits.max_total_bytes
                ),
            ));
        }

        let mut bytes = Vec::new();
        bytes.try_reserve_exact(initial_bytes).map_err(|_| {
            RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!("cannot allocate {initial_bytes} bytes of addressable memory"),
            )
        })?;
        bytes.resize(initial_bytes, 0);

        let region_id = self.next_region;
        self.next_region = self
            .next_region
            .checked_add(1)
            .ok_or_else(|| memory_error(span, "memory region identity space exhausted"))?;
        self.regions.insert(
            region_id,
            Region {
                bytes,
                max_bytes,
                generation: 1,
            },
        );
        self.committed_bytes = committed;
        Ok(MemoryHandle::from_runtime(self.run_id, region_id, 1))
    }

    /// Current length of a region in pages.
    pub fn size(&self, handle: MemoryHandle, span: Option<Span>) -> Result<u64, RuntimeError> {
        let region = self.region(handle, span)?;
        Ok((region.bytes.len() / PAGE_SIZE) as u64)
    }

    /// Grow a region by whole pages and return its previous page count.
    ///
    /// A failed grow is atomic: the region keeps its old length and bytes and
    /// the committed budget is unchanged.
    pub fn grow(
        &mut self,
        handle: MemoryHandle,
        pages: u64,
        span: Option<Span>,
    ) -> Result<u64, RuntimeError> {
        if pages == 0 {
            return self.size(handle, span);
        }
        let region = self.region(handle, span)?;
        let old_bytes = region.bytes.len();
        let old_pages = (old_bytes / PAGE_SIZE) as u64;
        let additional = checked_bytes(pages, span, "memory.grow pages")?;
        let new_bytes = old_bytes
            .checked_add(additional)
            .ok_or_else(|| memory_error(span, "memory.grow size overflows the address space"))?;
        if new_bytes > region.max_bytes {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!(
                    "memory.grow would grow the region to {new_bytes} bytes, beyond its \
                     declared maximum of {} bytes",
                    region.max_bytes
                ),
            ));
        }
        let committed = self
            .committed_bytes
            .checked_add(additional)
            .ok_or_else(|| budget_error(span, "memory budget overflow"))?;
        if committed > self.limits.max_total_bytes {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!(
                    "memory budget exceeded: growing by {additional} bytes would reach \
                     {committed} of {}",
                    self.limits.max_total_bytes
                ),
            ));
        }

        let region = self
            .regions
            .get_mut(&handle.region_id())
            .ok_or_else(|| memory_error(span, "unknown Memory region handle"))?;
        region.bytes.try_reserve(additional).map_err(|_| {
            RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!("cannot grow addressable memory by {additional} bytes"),
            )
        })?;
        region.bytes.resize(new_bytes, 0);
        self.committed_bytes = committed;
        Ok(old_pages)
    }

    /// Total byte length of a region.
    pub fn byte_len(
        &self,
        handle: MemoryHandle,
        span: Option<Span>,
    ) -> Result<usize, RuntimeError> {
        Ok(self.region(handle, span)?.bytes.len())
    }

    /// Read a contiguous byte range. Used by resource I/O adapters.
    pub fn read_bytes(
        &self,
        handle: MemoryHandle,
        offset: &BigInt,
        length: usize,
        span: Option<Span>,
    ) -> Result<Vec<u8>, RuntimeError> {
        let region = self.region(handle, span)?;
        let start = range_offset(offset, region.bytes.len(), length, span)?;
        Ok(region.bytes[start..start + length].to_vec())
    }

    /// Write a contiguous byte range atomically after a full bounds check.
    pub fn write_bytes(
        &mut self,
        handle: MemoryHandle,
        offset: &BigInt,
        bytes: &[u8],
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        let len = self.region(handle, span)?.bytes.len();
        let start = range_offset(offset, len, bytes.len(), span)?;
        let region = self
            .regions
            .get_mut(&handle.region_id())
            .ok_or_else(|| memory_error(span, "unknown Memory region handle"))?;
        region.bytes[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// Read one byte. Offsets are non-negative and region-relative.
    pub fn read(
        &self,
        handle: MemoryHandle,
        offset: &BigInt,
        span: Option<Span>,
    ) -> Result<u8, RuntimeError> {
        let region = self.region(handle, span)?;
        let position = checked_offset(offset, region.bytes.len(), span)?;
        Ok(region.bytes[position])
    }

    /// Write one byte. The caller must have validated the byte range.
    pub fn write(
        &mut self,
        handle: MemoryHandle,
        offset: &BigInt,
        byte: u8,
        span: Option<Span>,
    ) -> Result<(), RuntimeError> {
        let len = self.region(handle, span)?.bytes.len();
        let position = checked_offset(offset, len, span)?;
        let region = self
            .regions
            .get_mut(&handle.region_id())
            .ok_or_else(|| memory_error(span, "unknown Memory region handle"))?;
        region.bytes[position] = byte;
        Ok(())
    }

    fn region(&self, handle: MemoryHandle, span: Option<Span>) -> Result<&Region, RuntimeError> {
        if handle.run_id() != self.run_id {
            return Err(memory_error(
                span,
                "Memory handle belongs to a different execution run",
            ));
        }
        let Some(region) = self.regions.get(&handle.region_id()) else {
            return Err(memory_error(span, "unknown Memory region handle"));
        };
        if region.generation != handle.generation() {
            return Err(memory_error(span, "stale Memory region handle"));
        }
        Ok(region)
    }
}

fn checked_bytes(pages: u64, span: Option<Span>, what: &str) -> Result<usize, RuntimeError> {
    let bytes = pages
        .checked_mul(PAGE_SIZE as u64)
        .ok_or_else(|| memory_error(span, format!("{what} overflows the address space")))?;
    usize::try_from(bytes).map_err(|_| {
        RuntimeError::new(
            ErrorCategory::ResourceExhausted,
            span,
            format!("{what} ({pages} pages) does not fit this platform's address space"),
        )
    })
}

/// Validate an offset plus a byte count against the region length.
fn range_offset(
    offset: &BigInt,
    len: usize,
    length: usize,
    span: Option<Span>,
) -> Result<usize, RuntimeError> {
    if offset.sign() == num_bigint::Sign::Minus {
        return Err(RuntimeError::new(
            ErrorCategory::IndexOutOfBounds,
            span,
            "Memory offset must be a non-negative integer",
        ));
    }
    let Some(start) = offset.to_usize() else {
        return Err(RuntimeError::new(
            ErrorCategory::IndexOutOfBounds,
            span,
            format!("Memory offset {offset} is out of bounds for region length {len}"),
        ));
    };
    let end = start.checked_add(length).ok_or_else(|| {
        memory_error(
            span,
            "Memory offset plus length overflows the address space",
        )
    })?;
    if end > len {
        return Err(RuntimeError::new(
            ErrorCategory::IndexOutOfBounds,
            span,
            format!("Memory range {start}..{end} is out of bounds for region length {len}"),
        ));
    }
    Ok(start)
}

fn checked_offset(offset: &BigInt, len: usize, span: Option<Span>) -> Result<usize, RuntimeError> {
    if offset.sign() == num_bigint::Sign::Minus {
        return Err(RuntimeError::new(
            ErrorCategory::IndexOutOfBounds,
            span,
            "Memory offset must be a non-negative integer",
        ));
    }
    let Some(position) = offset.to_usize() else {
        return Err(RuntimeError::new(
            ErrorCategory::IndexOutOfBounds,
            span,
            format!("Memory offset {offset} is out of bounds for region length {len}"),
        ));
    };
    if position >= len {
        return Err(RuntimeError::new(
            ErrorCategory::IndexOutOfBounds,
            span,
            format!("Memory offset {offset} is out of bounds for region length {len}"),
        ));
    }
    Ok(position)
}

fn memory_error(span: Option<Span>, message: impl Into<String>) -> RuntimeError {
    RuntimeError::new(ErrorCategory::MemoryFault, span, message)
}

fn budget_error(span: Option<Span>, message: impl Into<String>) -> RuntimeError {
    RuntimeError::new(ErrorCategory::ResourceExhausted, span, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(value: impl Into<BigInt>) -> BigInt {
        value.into()
    }

    fn store() -> MemoryStore {
        MemoryStore::new(MemoryLimits::default())
    }

    #[test]
    fn regions_are_isolated_and_handles_share_identity() {
        let mut store = store();
        let first = store.create(1, 2, None).unwrap();
        let second = store.create(1, 2, None).unwrap();
        let alias = first;
        store.write(first, &int(0), 7, None).unwrap();
        store.write(second, &int(0), 9, None).unwrap();
        assert_eq!(store.read(first, &int(0), None).unwrap(), 7);
        assert_eq!(store.read(alias, &int(0), None).unwrap(), 7);
        assert_eq!(store.read(second, &int(0), None).unwrap(), 9);
        assert_eq!(first, alias);
        assert_ne!(first, second);
    }

    #[test]
    fn create_is_zero_filled_and_reports_pages() {
        let mut store = store();
        let memory = store.create(1, 4, None).unwrap();
        assert_eq!(store.size(memory, None).unwrap(), 1);
        assert_eq!(PAGE_SIZE, 64 * 1024);
        for offset in [0usize, 1, PAGE_SIZE - 1] {
            assert_eq!(store.read(memory, &int(offset as u64), None).unwrap(), 0);
        }
        // Zero-page creation is legal but every access is out of bounds.
        let empty = store.create(0, 1, None).unwrap();
        assert_eq!(store.size(empty, None).unwrap(), 0);
        assert_eq!(
            store.read(empty, &int(0), None).unwrap_err().category,
            ErrorCategory::IndexOutOfBounds
        );
    }

    #[test]
    fn grow_keeps_old_bytes_and_zero_fills_new_pages() {
        let mut store = store();
        let memory = store.create(1, 2, None).unwrap();
        store.write(memory, &int(3), 200, None).unwrap();
        assert_eq!(store.grow(memory, 1, None).unwrap(), 1);
        assert_eq!(store.size(memory, None).unwrap(), 2);
        assert_eq!(store.read(memory, &int(3), None).unwrap(), 200);
        assert_eq!(store.read(memory, &int(PAGE_SIZE as u64), None).unwrap(), 0);
        store
            .write(memory, &int(PAGE_SIZE as u64 + 5), 42, None)
            .unwrap();
        assert_eq!(
            store
                .read(memory, &int(PAGE_SIZE as u64 + 5), None)
                .unwrap(),
            42
        );
        // Zero-page growth is a no-op returning the old count.
        assert_eq!(store.grow(memory, 0, None).unwrap(), 2);
    }

    #[test]
    fn failed_grow_does_not_change_region_or_budget() {
        let mut store = MemoryStore::new(MemoryLimits {
            max_total_bytes: PAGE_SIZE * 2,
            max_region_bytes: PAGE_SIZE * 4,
        });
        let memory = store.create(1, 2, None).unwrap();
        store.write(memory, &int(0), 1, None).unwrap();
        let before = store.committed_bytes();
        let error = store.grow(memory, 2, None).unwrap_err();
        assert_eq!(error.category, ErrorCategory::ResourceExhausted);
        assert_eq!(store.size(memory, None).unwrap(), 1);
        assert_eq!(store.read(memory, &int(0), None).unwrap(), 1);
        assert_eq!(store.committed_bytes(), before);
    }

    #[test]
    fn invalid_offsets_are_structured_errors() {
        let mut store = store();
        let memory = store.create(1, 1, None).unwrap();
        for offset in [int(-1), int(PAGE_SIZE as u64)] {
            let error = store.read(memory, &offset, None).unwrap_err();
            assert_eq!(error.category, ErrorCategory::IndexOutOfBounds);
        }
        let huge: BigInt = "999999999999999999999999999999999999999".parse().unwrap();
        assert_eq!(
            store.read(memory, &huge, None).unwrap_err().category,
            ErrorCategory::IndexOutOfBounds
        );
        assert_eq!(
            store.write(memory, &huge, 1, None).unwrap_err().category,
            ErrorCategory::IndexOutOfBounds
        );
        assert_eq!(
            store
                .write(memory, &int(PAGE_SIZE as u64), 1, None)
                .unwrap_err()
                .category,
            ErrorCategory::IndexOutOfBounds
        );
    }

    #[test]
    fn create_validates_page_counts_and_budget() {
        let mut store = MemoryStore::new(MemoryLimits {
            max_total_bytes: PAGE_SIZE * 3,
            max_region_bytes: PAGE_SIZE * 2,
        });
        assert_eq!(
            store.create(2, 1, None).unwrap_err().category,
            ErrorCategory::MemoryFault
        );
        assert_eq!(
            store.create(1, 3, None).unwrap_err().category,
            ErrorCategory::ResourceExhausted
        );
        let _first = store.create(2, 2, None).unwrap();
        let _second = store.create(1, 1, None).unwrap();
        // Total budget is now exactly three pages.
        assert_eq!(
            store.create(1, 1, None).unwrap_err().category,
            ErrorCategory::ResourceExhausted
        );
        assert_eq!(store.committed_bytes(), PAGE_SIZE * 3);
    }

    #[test]
    fn forged_and_stale_handles_are_rejected() {
        let mut store = store();
        let memory = store.create(1, 1, None).unwrap();
        let forged = MemoryHandle::from_runtime(memory.run_id() + 1, memory.region_id(), 1);
        assert_eq!(
            store.read(forged, &int(0), None).unwrap_err().category,
            ErrorCategory::MemoryFault
        );
        let stale = MemoryHandle::from_runtime(memory.run_id(), memory.region_id(), 2);
        assert_eq!(
            store.read(stale, &int(0), None).unwrap_err().category,
            ErrorCategory::MemoryFault
        );
        let unknown = MemoryHandle::from_runtime(memory.run_id(), 999, 1);
        assert_eq!(
            store.write(unknown, &int(0), 1, None).unwrap_err().category,
            ErrorCategory::MemoryFault
        );
    }

    #[test]
    fn page_arithmetic_never_overflows() {
        let mut store = MemoryStore::new(MemoryLimits {
            max_total_bytes: usize::MAX,
            max_region_bytes: usize::MAX,
        });
        assert_eq!(
            store.create(u64::MAX, u64::MAX, None).unwrap_err().category,
            ErrorCategory::ResourceExhausted
        );
    }
}
