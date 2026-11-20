//! Safe wrappers around the raw `MqBytes` / `MqOwnedBytes` ABI buffers.
//!
//! Ownership rules (see the canonical header):
//! - [`BytesRef`] is a borrowed, validated view of host input; it is only
//!   valid for the call that produced it.
//! - [`HostBuffer`] is plugin output allocated with the host allocator and
//!   handed back to the host, which frees it after copying.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr;

use marqdo_plugin_abi::abi::{MqBytes, MqHostApi, MqOwnedBytes};

use crate::PluginError;

/// Validated borrowed view of an ABI byte buffer (`MqBytes`).
pub struct BytesRef<'a> {
    raw: MqBytes,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> BytesRef<'a> {
    /// Validate a raw borrowed buffer.
    ///
    /// # Safety
    ///
    /// If `raw.len > 0`, `raw.data` must point to `raw.len` initialized bytes
    /// that stay alive for `'a`. Within the ABI, `'a` must not outlive the
    /// call that produced the buffer.
    pub unsafe fn from_raw(raw: MqBytes) -> Result<Self, PluginError> {
        raw.validate()?;
        Ok(Self {
            raw,
            _marker: PhantomData,
        })
    }

    /// The underlying raw ABI view.
    pub fn as_raw(&self) -> MqBytes {
        self.raw
    }

    /// The bytes as a slice (valid for `'a`).
    pub fn as_slice(&self) -> &'a [u8] {
        if self.raw.len == 0 {
            return &[];
        }
        // SAFETY: `from_raw` validated the pointer/length pair and `'a` is
        // tied to the caller's borrow guarantee.
        unsafe { std::slice::from_raw_parts(self.raw.data, self.raw.len) }
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.raw.len
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.raw.len == 0
    }
}

/// A byte buffer allocated with the host allocator and owned by the host.
///
/// The plugin writes its response/error wire into a [`HostBuffer`] and
/// transfers ownership through the ABI output parameter. The plugin must
/// never free this buffer itself; the host frees it after copying.
pub struct HostBuffer {
    raw: MqOwnedBytes,
}

impl HostBuffer {
    /// The empty buffer `{NULL, 0}`.
    pub fn empty() -> Self {
        Self {
            raw: MqOwnedBytes::EMPTY,
        }
    }

    /// Allocate a host buffer and copy `bytes` into it.
    ///
    /// `host` must be the host API table provided to the entry point (the SDK
    /// stores it for the plugin lifetime). Returns an out-of-memory error if
    /// the host allocator returns NULL.
    pub fn copy_from(host: &MqHostApi, bytes: &[u8]) -> Result<Self, PluginError> {
        if bytes.is_empty() {
            return Ok(Self::empty());
        }
        // SAFETY: host alloc/free pointers are non-NULL and valid for the
        // plugin lifetime (validated during the entry call).
        let pointer = unsafe { (host.alloc)(host.host_data, bytes.len()) };
        if pointer.is_null() {
            return Err(PluginError::invoke(
                "out_of_memory",
                "host allocator returned NULL for a plugin output buffer",
            ));
        }
        // SAFETY: pointer has bytes.len() writable bytes and does not overlap
        // the source slice.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), pointer as *mut u8, bytes.len());
        }
        Ok(Self {
            raw: MqOwnedBytes::new(pointer as *mut u8, bytes.len()),
        })
    }

    /// The raw owner-transfer buffer.
    pub fn into_raw(self) -> MqOwnedBytes {
        self.raw
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.raw.len
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.raw.len == 0
    }
}

/// Borrowed view of a capability reply or other host-allocated buffer.
///
/// The plugin copies what it needs and then must release the buffer with the
/// host allocator; use [`release_host_buffer`].
///
/// # Safety
///
/// `buffer` must be a host-allocated buffer that is still alive; the returned
/// slice must not outlive the release of that buffer.
pub unsafe fn reply_slice<'a>(buffer: MqOwnedBytes) -> &'a [u8] {
    if buffer.len == 0 {
        return &[];
    }
    // SAFETY: host-allocated buffer is initialized for `buffer.len` bytes; the
    // caller must not retain the slice beyond the release point.
    unsafe { std::slice::from_raw_parts(buffer.data, buffer.len) }
}

/// Release a host-allocated buffer through the host allocator.
///
/// # Safety
///
/// `buffer.data` must be NULL or a pointer returned by the host allocator
/// (`MqHostApi::alloc`) that has not been freed yet.
pub unsafe fn release_host_buffer(host: &MqHostApi, buffer: MqOwnedBytes) {
    if buffer.data.is_null() {
        return;
    }
    // SAFETY: alloc/free pair and non-NULL pointer per the caller contract.
    unsafe { (host.free)(host.host_data, buffer.data as *mut c_void) };
}
