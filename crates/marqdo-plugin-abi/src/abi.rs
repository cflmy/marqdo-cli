//! Stable-layout Rust mirrors of the canonical C ABI v1 header
//! (`include/marqdo_plugin_abi_v1.h`) plus version negotiation.
//!
//! The structs in this module are the *only* types allowed to cross the
//! native plugin boundary. Layout is asserted at compile time for both 32-bit
//! and 64-bit targets; do not reorder, resize or add fields to an existing
//! struct outside of a new ABI minor that appends fields after `struct_size`.

use std::ffi::c_void;
use std::ptr;

use serde::{Deserialize, Serialize};

use crate::error::PluginError;

/// Canonical C header version this crate implements.
pub mod version {
    /// Plugin ABI major version. A major mismatch always fails the load.
    pub const ABI_MAJOR: u32 = 1;
    /// Plugin ABI minor version implemented by this crate.
    pub const ABI_MINOR: u32 = 0;
    /// Human-readable ABI version.
    pub const ABI_VERSION_STRING: &str = "1.0";
    /// Required exported entry symbol.
    pub const ENTRY_SYMBOL: &[u8] = b"mq_plugin_entry_v1";
    /// Required exported entry symbol (UTF-8 form).
    pub const ENTRY_SYMBOL_STR: &str = "mq_plugin_entry_v1";
}

pub use version::{ABI_MAJOR, ABI_MINOR, ABI_VERSION_STRING, ENTRY_SYMBOL};

/// ABI result codes returned by the entry point and invocation callbacks.
pub mod result {
    /// Success.
    pub const OK: i32 = 0;
    /// Malformed argument (NULL pointer, bad pointer/length pair, ...).
    pub const INVALID_ARGUMENT: i32 = 1;
    /// ABI major mismatch or incompatible `struct_size`.
    pub const ABI_MISMATCH: i32 = 2;
    /// A required export could not be registered.
    pub const REGISTRATION: i32 = 3;
    /// Operation/type/capability not supported by this version.
    pub const UNSUPPORTED: i32 = 4;
    /// Host allocation failed.
    pub const OUT_OF_MEMORY: i32 = 5;
    /// Payload is not valid wire_version 1 JSON.
    pub const BAD_WIRE: i32 = 6;
    /// Unexpected internal plugin failure (including caught panics).
    pub const INTERNAL: i32 = 7;
    /// Capability request failed or was denied.
    pub const CAPABILITY: i32 = 8;

    /// Human-readable name for a result code.
    pub const fn name(code: i32) -> &'static str {
        match code {
            OK => "ok",
            INVALID_ARGUMENT => "invalid_argument",
            ABI_MISMATCH => "abi_mismatch",
            REGISTRATION => "registration",
            UNSUPPORTED => "unsupported",
            OUT_OF_MEMORY => "out_of_memory",
            BAD_WIRE => "bad_wire",
            INTERNAL => "internal",
            CAPABILITY => "capability",
            _ => "unknown",
        }
    }
}

/// Maximum export-name length in bytes.
pub const MAX_EXPORT_NAME_BYTES: usize = 256;
/// Maximum `signature_wire` length in bytes.
pub const MAX_SIGNATURE_WIRE_BYTES: usize = 4096;
/// Maximum plugin metadata wire length in bytes.
pub const MAX_METADATA_WIRE_BYTES: usize = 65_536;
/// Maximum request wire length in bytes (8 MiB frame minus envelope reserve).
pub const MAX_REQUEST_WIRE_BYTES: usize = 8 * 1024 * 1024 - 4096;
/// Maximum result wire length in bytes (8 MiB frame minus envelope reserve).
pub const MAX_RESULT_WIRE_BYTES: usize = 8 * 1024 * 1024 - 4096;
/// Maximum error wire length in bytes (1 MiB).
pub const MAX_ERROR_WIRE_BYTES: usize = 1024 * 1024;
/// Maximum capability reply length in bytes (8 MiB frame minus envelope reserve).
pub const MAX_CAPABILITY_REPLY_BYTES: usize = 8 * 1024 * 1024 - 4096;

/// Borrowed, immutable byte view (`MqBytes`).
///
/// The producer keeps ownership; the pointer is only valid for the duration
/// of the call that received it. `{NULL, 0}` denotes the empty buffer.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MqBytes {
    /// Pointer to the first byte, or NULL when `len == 0`.
    pub data: *const u8,
    /// Length in bytes.
    pub len: usize,
}

impl MqBytes {
    /// The empty buffer `{NULL, 0}`.
    pub const EMPTY: Self = Self {
        data: ptr::null(),
        len: 0,
    };

    /// Construct a borrowed byte view from a raw pointer and length.
    pub const fn new(data: *const u8, len: usize) -> Self {
        Self { data, len }
    }

    /// Construct a borrowed view over a slice.
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self {
            data: bytes.as_ptr(),
            len: bytes.len(),
        }
    }

    /// Whether the view is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Validate the pointer/length invariant.
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.len == 0 {
            return Ok(());
        }
        if self.data.is_null() {
            return Err(PluginError::wire(
                "invalid_bytes",
                "non-empty MqBytes has a NULL data pointer",
            ));
        }
        Ok(())
    }

    /// View the bytes as a slice.
    ///
    /// # Safety
    ///
    /// If `len > 0`, `data` must point to `len` initialized bytes that stay
    /// alive for `'a`. For ABI borrows, `'a` must not outlive the call that
    /// produced the pointer.
    pub unsafe fn as_slice<'a>(self) -> Result<&'a [u8], PluginError> {
        self.validate()?;
        if self.len == 0 {
            return Ok(&[]);
        }
        // SAFETY: guaranteed by the caller's contract above.
        Ok(unsafe { std::slice::from_raw_parts(self.data, self.len) })
    }
}

impl std::fmt::Debug for MqBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MqBytes")
            .field("data", &self.data)
            .field("len", &self.len)
            .finish()
    }
}

/// Mutable, owned byte buffer (`MqOwnedBytes`).
///
/// The producer allocates with `MqHostApi::alloc` and transfers ownership to
/// the consumer, which copies and then calls `MqHostApi::free`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MqOwnedBytes {
    /// Pointer to the first byte, or NULL when `len == 0`.
    pub data: *mut u8,
    /// Length in bytes.
    pub len: usize,
}

impl MqOwnedBytes {
    /// The empty buffer `{NULL, 0}`.
    pub const EMPTY: Self = Self {
        data: ptr::null_mut(),
        len: 0,
    };

    /// Construct an owner-transfer buffer from a raw pointer and length.
    pub const fn new(data: *mut u8, len: usize) -> Self {
        Self { data, len }
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Validate the pointer/length invariant.
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.len == 0 {
            return Ok(());
        }
        if self.data.is_null() {
            return Err(PluginError::wire(
                "invalid_bytes",
                "non-empty MqOwnedBytes has a NULL data pointer",
            ));
        }
        Ok(())
    }

    /// Copy the buffer into a `Vec`.
    ///
    /// # Safety
    ///
    /// If `len > 0`, `data` must point to `len` initialized bytes.
    pub unsafe fn to_vec(&self) -> Result<Vec<u8>, PluginError> {
        self.validate()?;
        if self.len == 0 {
            return Ok(Vec::new());
        }
        // SAFETY: guaranteed by the caller's contract above.
        Ok(unsafe { std::slice::from_raw_parts(self.data, self.len) }.to_vec())
    }

    /// View the bytes as a slice.
    ///
    /// # Safety
    ///
    /// If `len > 0`, `data` must point to `len` initialized bytes that stay
    /// alive for `'a`.
    pub unsafe fn as_slice<'a>(self) -> Result<&'a [u8], PluginError> {
        self.validate()?;
        if self.len == 0 {
            return Ok(&[]);
        }
        // SAFETY: guaranteed by the caller's contract above.
        Ok(unsafe { std::slice::from_raw_parts(self.data, self.len) })
    }
}

impl std::fmt::Debug for MqOwnedBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MqOwnedBytes")
            .field("data", &self.data)
            .field("len", &self.len)
            .finish()
    }
}

/// Opaque, host-owned invocation context.
///
/// Plugins only ever hold a pointer to this type; its layout is private to
/// the host and may change in a later ABI minor. It is valid only during the
/// invocation it was minted for.
#[repr(C)]
pub struct MqCallContext {
    host_data: *mut c_void,
}

impl MqCallContext {
    /// Mint a context for a host invocation.
    ///
    /// Host-side execution API: the native host owns the invocation lifetime
    /// and passes an opaque pointer the plugin never dereferences.
    pub fn new(host_data: *mut c_void) -> Self {
        Self { host_data }
    }

    /// Opaque host data pointer carried by this context.
    ///
    /// Host-side execution API; not part of the C layout exposed to plugins.
    pub fn host_data(&self) -> *mut c_void {
        self.host_data
    }
}

impl std::fmt::Debug for MqCallContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MqCallContext(..)")
    }
}

/// Host allocator callback (`MqHostApi::alloc`).
pub type MqAllocFn = unsafe extern "C" fn(host_data: *mut c_void, size: usize) -> *mut c_void;
/// Host deallocator callback (`MqHostApi::free`).
pub type MqFreeFn = unsafe extern "C" fn(host_data: *mut c_void, ptr: *mut c_void);
/// Export registration callback (`MqHostApi::register_function`).
pub type MqRegisterFunctionFn = unsafe extern "C" fn(
    host_data: *mut c_void,
    export_name: MqBytes,
    signature_wire: MqBytes,
    invoke: MqPluginInvoke,
) -> i32;
/// Capability request callback (`MqHostApi::request_capability`).
pub type MqRequestCapabilityFn = unsafe extern "C" fn(
    ctx: *mut MqCallContext,
    name: MqBytes,
    request_wire: MqBytes,
    reply_wire: *mut MqOwnedBytes,
) -> i32;

/// Plugin invocation callback (`MqPluginInvoke`).
pub type MqPluginInvoke = unsafe extern "C" fn(
    ctx: *mut MqCallContext,
    args_wire: MqBytes,
    result_wire: *mut MqOwnedBytes,
    error_wire: *mut MqOwnedBytes,
) -> i32;

/// Plugin entry point (`mq_plugin_entry_v1`).
pub type MqPluginEntryV1 =
    unsafe extern "C" fn(host: *const MqHostApi, plugin: *mut MqPluginApi) -> i32;

/// Host-provided function table (`MqHostApi`).
///
/// Mirrors the C struct exactly. `struct_size` must be `sizeof(MqHostApi)` as
/// built by the host; the v1.0 contract requires all function pointers to be
/// non-NULL.
#[repr(C)]
pub struct MqHostApi {
    /// `sizeof(MqHostApi)` as seen by the host.
    pub struct_size: u32,
    /// Host-supported ABI major (always [`ABI_MAJOR`] for this version).
    pub abi_major: u32,
    /// Host-supported ABI minor.
    pub abi_minor: u32,
    /// Opaque host state; passed back to every callback.
    pub host_data: *mut c_void,
    /// Host allocator; NULL on failure and for `size == 0`.
    pub alloc: MqAllocFn,
    /// Host deallocator for pointers returned by `alloc`.
    pub free: MqFreeFn,
    /// Export registration callback.
    pub register_function: MqRegisterFunctionFn,
    /// Capability request callback.
    pub request_capability: MqRequestCapabilityFn,
}

/// Plugin version/metadata table (`MqPluginApi`).
#[repr(C)]
pub struct MqPluginApi {
    /// `sizeof(MqPluginApi)` as initialized by the host.
    pub struct_size: u32,
    /// Plugin ABI major; must equal [`ABI_MAJOR`].
    pub abi_major: u32,
    /// Plugin ABI minor; the host clamps to its own supported minor.
    pub abi_minor: u32,
    /// Plugin metadata JSON, valid only during the entry call.
    pub plugin_metadata_wire: MqBytes,
}

impl MqPluginApi {
    /// Host-side initialization before calling the entry point.
    pub fn host_initialized() -> Self {
        Self {
            struct_size: size_of::<MqPluginApi>() as u32,
            abi_major: 0,
            abi_minor: 0,
            plugin_metadata_wire: MqBytes::EMPTY,
        }
    }
}

/// A negotiated ABI version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AbiVersion {
    /// Negotiated (host) major; always [`ABI_MAJOR`].
    pub major: u32,
    /// Negotiated minor, clamped to the host's supported minor.
    pub minor: u32,
}

impl AbiVersion {
    /// The ABI version implemented by this crate.
    pub const fn current() -> Self {
        Self {
            major: ABI_MAJOR,
            minor: ABI_MINOR,
        }
    }

    /// Construct a version pair.
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl std::fmt::Display for AbiVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Negotiate the ABI version with a plugin reporting `major`/`minor`.
///
/// Rules (see the header's "VERSION NEGOTIATION" section):
/// - a different major version is never negotiable: the load fails;
/// - a plugin minor greater than the host minor is accepted but clamped to
///   the host's supported minor; only features at that minor may be used.
pub fn negotiate(plugin_major: u32, plugin_minor: u32) -> Result<AbiVersion, PluginError> {
    if plugin_major != ABI_MAJOR {
        return Err(PluginError::abi(
            "abi_major_mismatch",
            format!(
                "plugin ABI major {plugin_major} is incompatible with host ABI major {ABI_MAJOR}"
            ),
        )
        .with_detail(format!(
            "plugin abi {plugin_major}.{plugin_minor}, host abi {ABI_MAJOR}.{ABI_MINOR}"
        )));
    }
    // Clamp to the host's supported minor. `ABI_MINOR` is 0 today, so clippy
    // sees the clamp as a no-op; it is required for future minor bumps.
    #[allow(clippy::unnecessary_min_or_max)]
    let negotiated_minor = plugin_minor.min(ABI_MINOR);
    Ok(AbiVersion::new(plugin_major, negotiated_minor))
}

/// Check a host API table handed to a plugin (plugin-side validation).
///
/// A conforming v1.0 host provides `struct_size == sizeof(MqHostApi)`,
/// `abi_major == 1` and non-NULL alloc/free/register_function/
/// request_capability pointers.
///
/// # Safety
///
/// `host` must be a readable pointer to an `MqHostApi`.
pub unsafe fn check_host_api(host: *const MqHostApi) -> Result<AbiVersion, PluginError> {
    if host.is_null() {
        return Err(PluginError::abi(
            "host_api_null",
            "host passed a NULL MqHostApi pointer",
        ));
    }
    // SAFETY: non-NULL and readable per the caller contract.
    let host = unsafe { &*host };
    let min_size = size_of::<MqHostApi>() as u32;
    if host.struct_size < min_size {
        return Err(PluginError::abi(
            "host_struct_size_too_small",
            format!(
                "host MqHostApi struct_size {} is smaller than the required {min_size}",
                host.struct_size
            ),
        ));
    }
    let negotiated = negotiate(host.abi_major, host.abi_minor)?;
    if host.alloc as usize == 0
        || host.free as usize == 0
        || host.register_function as usize == 0
        || host.request_capability as usize == 0
    {
        return Err(PluginError::abi(
            "host_api_incomplete",
            "host MqHostApi has NULL required function pointers",
        ));
    }
    Ok(negotiated)
}

/// Plugin metadata reported through `MqPluginApi::plugin_metadata_wire`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin package name (matches `plugin.yaml`).
    pub name: String,
    /// Plugin package semver (informational; manifests are authoritative).
    pub version: String,
}

impl PluginMetadata {
    /// Encode as `plugin_metadata_wire` JSON.
    pub fn to_wire(&self) -> Result<Vec<u8>, PluginError> {
        serde_json::to_vec(self).map_err(|error| {
            PluginError::wire("metadata_encode_failed", "could not encode plugin metadata")
                .with_detail(error.to_string())
        })
    }

    /// Decode and validate `plugin_metadata_wire` bytes.
    pub fn from_wire(bytes: &[u8]) -> Result<Self, PluginError> {
        if bytes.is_empty() {
            return Err(PluginError::wire(
                "metadata_missing",
                "plugin did not provide plugin_metadata_wire",
            ));
        }
        if bytes.len() > MAX_METADATA_WIRE_BYTES {
            return Err(PluginError::wire(
                "metadata_too_large",
                format!(
                    "plugin metadata is {} bytes, limit is {MAX_METADATA_WIRE_BYTES}",
                    bytes.len()
                ),
            ));
        }
        let metadata: Self = serde_json::from_slice(bytes).map_err(|error| {
            PluginError::wire(
                "metadata_invalid",
                "plugin_metadata_wire is not a valid metadata object",
            )
            .with_detail(error.to_string())
        })?;
        if metadata.name.trim().is_empty() {
            return Err(PluginError::wire(
                "metadata_invalid",
                "plugin metadata name must not be empty",
            ));
        }
        Ok(metadata)
    }
}

// Compile-time layout assertions. These are the frozen ABI contract.
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<MqBytes>() == 16, "MqBytes must be 16 bytes");
    assert!(align_of::<MqBytes>() == 8, "MqBytes must align to 8");
    assert!(
        size_of::<MqOwnedBytes>() == 16,
        "MqOwnedBytes must be 16 bytes"
    );
    assert!(
        align_of::<MqOwnedBytes>() == 8,
        "MqOwnedBytes must align to 8"
    );
    assert!(
        size_of::<MqPluginApi>() == 32,
        "MqPluginApi must be 32 bytes"
    );
    assert!(
        align_of::<MqPluginApi>() == 8,
        "MqPluginApi must align to 8"
    );
    assert!(size_of::<MqHostApi>() == 56, "MqHostApi must be 56 bytes");
    assert!(align_of::<MqHostApi>() == 8, "MqHostApi must align to 8");
    assert!(
        size_of::<MqPluginInvoke>() == 8,
        "MqPluginInvoke must be 8 bytes"
    );
};

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(size_of::<MqBytes>() == 8, "MqBytes must be 8 bytes");
    assert!(
        size_of::<MqOwnedBytes>() == 8,
        "MqOwnedBytes must be 8 bytes"
    );
    assert!(
        size_of::<MqPluginApi>() == 20,
        "MqPluginApi must be 20 bytes"
    );
    assert!(size_of::<MqHostApi>() == 28, "MqHostApi must be 28 bytes");
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_c_header_on_64_bit() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(size_of::<MqBytes>(), 16);
            assert_eq!(align_of::<MqBytes>(), 8);
            assert_eq!(size_of::<MqOwnedBytes>(), 16);
            assert_eq!(size_of::<MqPluginApi>(), 32);
            assert_eq!(size_of::<MqHostApi>(), 56);
            assert_eq!(align_of::<MqHostApi>(), 8);
        }
    }

    #[test]
    fn negotiation_accepts_same_major_and_clamps_minor() {
        assert_eq!(negotiate(1, 0).expect("v1.0"), AbiVersion::new(1, 0));
        assert_eq!(negotiate(1, 1).expect("v1.1"), AbiVersion::new(1, 0));
        assert_eq!(negotiate(1, 99).expect("v1.99"), AbiVersion::new(1, 0));
    }

    #[test]
    fn negotiation_rejects_other_majors() {
        for major in [0, 2, 3, u32::MAX] {
            let error = negotiate(major, 0).expect_err("major must be rejected");
            assert_eq!(error.kind, crate::error::ErrorKind::Abi);
            assert_eq!(error.code, "abi_major_mismatch");
        }
    }

    #[test]
    fn metadata_wire_roundtrip() {
        let metadata = PluginMetadata {
            name: "demo.solver".into(),
            version: "1.2.3".into(),
        };
        let bytes = metadata.to_wire().expect("encode");
        assert_eq!(PluginMetadata::from_wire(&bytes).expect("decode"), metadata);
    }

    #[test]
    fn metadata_rejects_empty_and_invalid() {
        assert_eq!(
            PluginMetadata::from_wire(b"").expect_err("empty").code,
            "metadata_missing"
        );
        assert_eq!(
            PluginMetadata::from_wire(b"not-json")
                .expect_err("invalid")
                .code,
            "metadata_invalid"
        );
        assert_eq!(
            PluginMetadata::from_wire(br#"{"name":"","version":"1.0.0"}"#)
                .expect_err("empty name")
                .code,
            "metadata_invalid"
        );
    }
}
