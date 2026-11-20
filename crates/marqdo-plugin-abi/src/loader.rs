//! Native dynamic-library loader and minimal invocation bridge.
//!
//! Lifecycle implemented here (blueprint section 10.5):
//! `validate manifest -> resolve artifact -> verify hash -> negotiate ABI ->
//! register exports -> load library -> verify metadata/exports -> invoke`.
//!
//! Capability *authorization* happens in the host layer before/around
//! [`LoadedPlugin::open`]; this module only exposes the `request_capability`
//! bridge so an already-authorized capability can be consumed.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::ptr;

use libloading::{Library, Symbol};
use serde::Deserialize;

use crate::abi::{
    check_host_api, negotiate, result, AbiVersion, MqBytes, MqCallContext, MqHostApi, MqOwnedBytes,
    MqPluginApi, MqPluginEntryV1, MqPluginInvoke, PluginMetadata, ABI_MAJOR, ABI_MINOR,
    ENTRY_SYMBOL, MAX_CAPABILITY_REPLY_BYTES, MAX_ERROR_WIRE_BYTES, MAX_EXPORT_NAME_BYTES,
    MAX_RESULT_WIRE_BYTES, MAX_SIGNATURE_WIRE_BYTES,
};
use crate::artifact::HostTarget;
use crate::error::PluginError;
use crate::manifest::{PluginKind, PluginManifest};
use crate::wire::{WireRequest, WireResponse};

/// A callback the host installs to answer capability requests from a plugin.
///
/// The handler is invoked on the host invocation thread, one call at a time.
/// It must consult the host's authorization layer and must not perform
/// effects that the loaded plugin was not granted.
pub type CapabilityHandler = Box<dyn FnMut(&str, &[u8]) -> Result<Vec<u8>, PluginError> + 'static>;

/// Options for loading a native plugin.
pub struct LoadOptions {
    /// Host target used to select the manifest artifact.
    pub host_target: HostTarget,
    /// Optional capability bridge; without one every request is denied.
    pub capability_handler: Option<CapabilityHandler>,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            host_target: HostTarget::current(),
            capability_handler: None,
        }
    }
}

impl std::fmt::Debug for LoadOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadOptions")
            .field("host_target", &self.host_target)
            .field(
                "capability_handler",
                &self.capability_handler.as_ref().map(|_| "<handler>"),
            )
            .finish()
    }
}

/// One export registered by a loaded plugin.
#[derive(Debug, Clone)]
pub struct RegisteredExport {
    /// Fully qualified export name.
    pub name: String,
    /// Signature string reported through `signature_wire`.
    pub signature: String,
    invoke: MqPluginInvoke,
}

impl RegisteredExport {
    /// The registered export name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The registered signature string.
    pub fn signature(&self) -> &str {
        &self.signature
    }
}

/// Host state shared with the loaded plugin through `MqHostApi::host_data`
/// and each invocation's `MqCallContext`.
struct HostState {
    registrations: Vec<RegisteredExport>,
    capability_handler: Option<CapabilityHandler>,
}

/// A loaded, validated native plugin instance.
///
/// # Threading and unloading
///
/// A [`LoadedPlugin`] is single-threaded: every invocation must happen on the
/// thread that loaded it, one at a time. Dropping it unloads the native
/// library, so it must only be dropped after all calls have returned (the
/// `&mut self` receiver of [`LoadedPlugin::invoke`] enforces this within Rust).
/// ABI v1 has no hot-reload and no shutdown callback; dropping is a shutdown
/// operation, not a reload mechanism.
///
/// Because ABI v1 plugins may keep the host table in a process-global (the
/// single-threaded contract), do not load the same artifact path twice in one
/// process; use one instance per artifact instead.
pub struct LoadedPlugin {
    /// Keeps the dynamic library mapped for as long as the plugin is alive.
    _library: Library,
    host_state: Box<HostState>,
    /// Heap-allocated so the pointer handed to the plugin at entry time stays
    /// valid for the plugin lifetime (the plugin may retain it, see header).
    host_api: Box<MqHostApi>,
    exports: BTreeMap<String, RegisteredExport>,
    metadata: PluginMetadata,
    abi: AbiVersion,
}

impl std::fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("name", &self.metadata.name)
            .field("version", &self.metadata.version)
            .field("abi", &self.abi)
            .field("exports", &self.exports.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Signature wire decoded from `MqHostApi::register_function`.
#[derive(Deserialize)]
struct SignatureWire {
    signature: String,
}

impl LoadedPlugin {
    /// Validate a manifest and load the matching native artifact from
    /// `base_dir` (the directory containing `plugin.yaml`).
    ///
    /// This runs the full local lifecycle: manifest validation, artifact
    /// resolution/hash verification, dynamic library loading, ABI
    /// negotiation, export registration and metadata cross-checking.
    pub fn open(manifest: &PluginManifest, base_dir: &Path) -> Result<Self, PluginError> {
        Self::open_with(manifest, base_dir, LoadOptions::default())
    }

    /// Like [`LoadedPlugin::open`] with explicit target and capability bridge.
    pub fn open_with(
        manifest: &PluginManifest,
        base_dir: &Path,
        options: LoadOptions,
    ) -> Result<Self, PluginError> {
        manifest.validate()?;
        if manifest.plugin.kind != PluginKind::Native {
            return Err(PluginError::unsupported(
                "plugin_kind_not_loadable_in_process",
                format!(
                    "plugin kind `{}` cannot be loaded natively; use the isolated worker path",
                    manifest.plugin.kind.as_str()
                ),
            ));
        }

        let (path, _artifact) = manifest.verify_artifact(base_dir, &options.host_target)?;

        // SAFETY: `Library::new` runs the platform loader against a path whose
        // content hash was just verified. Loading native code is a trust
        // decision made by the caller, not a sandbox.
        let library = unsafe { Library::new(&path) }.map_err(|error| {
            PluginError::load(
                "library_load_failed",
                format!("dynamic library `{}` could not be loaded", path.display()),
            )
            .with_detail(error.to_string())
        })?;

        let mut plugin_api = MqPluginApi::host_initialized();
        let mut host_state = Box::new(HostState {
            registrations: Vec::new(),
            capability_handler: options.capability_handler,
        });
        let host_api = Box::new(MqHostApi {
            struct_size: size_of::<MqHostApi>() as u32,
            abi_major: ABI_MAJOR,
            abi_minor: ABI_MINOR,
            // The HostState Box is heap-allocated, so this pointer survives
            // moves of the LoadedPlugin value.
            host_data: (&mut *host_state) as *mut HostState as *mut c_void,
            alloc: host_alloc,
            free: host_free,
            register_function: host_register_function,
            request_capability: host_request_capability,
        });

        let entry_code = {
            // SAFETY: symbol lookup in a library we just loaded; the symbol
            // type is the documented ABI v1 entry signature.
            let entry: Symbol<MqPluginEntryV1> =
                unsafe { library.get(ENTRY_SYMBOL) }.map_err(|error| {
                    PluginError::load(
                        "missing_entry_symbol",
                        "plugin does not export `mq_plugin_entry_v1`",
                    )
                    .with_detail(error.to_string())
                })?;
            // SAFETY: entry point contract: one call, same thread, host table
            // and plugin table valid for the duration of the call.
            unsafe { entry(&*host_api as *const MqHostApi, &mut plugin_api) }
        };

        if entry_code != result::OK {
            return Err(entry_failure(entry_code));
        }

        let required_size = size_of::<MqPluginApi>() as u32;
        if plugin_api.struct_size < required_size {
            return Err(PluginError::abi(
                "plugin_api_struct_size_too_small",
                format!(
                    "plugin reported MqPluginApi struct_size {}, host requires at least {required_size}",
                    plugin_api.struct_size
                ),
            ));
        }
        let abi = negotiate(plugin_api.abi_major, plugin_api.abi_minor)?;

        // SAFETY: plugin-owned metadata is documented as valid for the entry
        // call only; we copy it here, immediately after the call.
        let metadata_bytes = unsafe { plugin_api.plugin_metadata_wire.as_slice() }
            .map_err(|error| {
                PluginError::wire(
                    "metadata_invalid",
                    "plugin metadata buffer violates the pointer/length contract",
                )
                .with_detail(error.to_string())
            })?
            .to_vec();
        let metadata = PluginMetadata::from_wire(&metadata_bytes)?;
        if metadata.name != manifest.plugin.name {
            return Err(PluginError::manifest(
                "metadata_name_mismatch",
                format!(
                    "plugin reports name `{}` but manifest declares `{}`",
                    metadata.name, manifest.plugin.name
                ),
            ));
        }
        let package_version = manifest.package_version()?;
        if metadata.version != package_version.raw {
            return Err(PluginError::manifest(
                "metadata_version_mismatch",
                format!(
                    "plugin reports version `{}` but manifest declares `{}`",
                    metadata.version, package_version.raw
                ),
            ));
        }

        let registrations = std::mem::take(&mut host_state.registrations);
        let exports = cross_check_exports(manifest, registrations)?;

        Ok(Self {
            _library: library,
            host_state,
            host_api,
            exports,
            metadata,
            abi,
        })
    }

    /// Negotiated ABI version.
    pub fn abi_version(&self) -> AbiVersion {
        self.abi
    }

    /// Plugin metadata reported by the plugin and cross-checked with the
    /// manifest name/version.
    pub fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    /// Registered exports in name order.
    pub fn exports(&self) -> impl Iterator<Item = &RegisteredExport> {
        self.exports.values()
    }

    /// Invoke `request.function` with the request envelope.
    ///
    /// The plugin must return exactly one wire response envelope. Plugin-side
    /// failures are surfaced as structured [`PluginError`]s.
    pub fn invoke(&mut self, request: &WireRequest) -> Result<WireResponse, PluginError> {
        request.validate()?;
        let args_wire = request.to_json_bytes()?;
        let response_wire = self.invoke_raw(&request.function, &args_wire)?;
        WireResponse::from_json_bytes(&response_wire)?.into_result()
    }

    /// Raw invocation bridge: call `function` with already-encoded request
    /// bytes and return the raw response bytes on success.
    ///
    /// This is primarily for ABI conformance tests; prefer
    /// [`LoadedPlugin::invoke`] for normal use.
    pub fn invoke_raw(&mut self, function: &str, args_wire: &[u8]) -> Result<Vec<u8>, PluginError> {
        if args_wire.len() > crate::abi::MAX_REQUEST_WIRE_BYTES {
            return Err(PluginError::wire(
                "request_too_large",
                format!(
                    "request wire is {} bytes, limit is {}",
                    args_wire.len(),
                    crate::abi::MAX_REQUEST_WIRE_BYTES
                ),
            ));
        }
        let invoke = self
            .exports
            .get(function)
            .map(|export| export.invoke)
            .ok_or_else(|| {
                PluginError::invoke(
                    "unknown_export",
                    format!("plugin does not export `{function}`"),
                )
            })?;

        let mut context =
            MqCallContext::new((&mut *self.host_state) as *mut HostState as *mut c_void);
        let mut result_wire = MqOwnedBytes::EMPTY;
        let mut error_wire = MqOwnedBytes::EMPTY;

        // SAFETY: invoking the plugin callback with valid buffers for the
        // duration of the call; the plugin owns output allocation via the
        // host allocator, which we free below after copying.
        let code = unsafe {
            invoke(
                &mut context as *mut MqCallContext,
                MqBytes::from_slice(args_wire),
                &mut result_wire,
                &mut error_wire,
            )
        };

        let host_api: &MqHostApi = &self.host_api;
        // SAFETY: result/error buffers are the plugin's outputs; copy and
        // release them exactly once.
        let (result_bytes, error_bytes) = unsafe {
            (
                take_host_buffer(host_api, &mut result_wire, MAX_RESULT_WIRE_BYTES, "result")?,
                take_host_buffer(host_api, &mut error_wire, MAX_ERROR_WIRE_BYTES, "error")?,
            )
        };

        if code != result::OK {
            if error_bytes.is_empty() {
                return Err(PluginError::invoke(
                    "plugin_call_failed",
                    format!(
                        "plugin returned result code {} without an error envelope",
                        result::name(code)
                    ),
                ));
            }
            return match WireResponse::from_error_json_bytes(&error_bytes) {
                Ok(response) => match response.into_result() {
                    Err(error) => Err(error),
                    Ok(_) => Err(PluginError::invoke(
                        "plugin_call_failed",
                        "plugin returned a success envelope with a failure code",
                    )),
                },
                Err(error) => Err(PluginError::invoke(
                    "plugin_call_failed",
                    format!(
                        "plugin returned result code {} with an invalid error envelope",
                        result::name(code)
                    ),
                )
                .with_detail(error.to_string())),
            };
        }

        if !error_bytes.is_empty() {
            return Err(PluginError::invoke(
                "unexpected_error_buffer",
                "plugin returned success but filled error_wire",
            ));
        }
        if result_bytes.is_empty() {
            return Err(PluginError::invoke(
                "missing_result_buffer",
                "plugin returned success but did not fill result_wire",
            ));
        }
        Ok(result_bytes)
    }
}

fn entry_failure(code: i32) -> PluginError {
    if code == result::ABI_MISMATCH {
        PluginError::abi(
            "plugin_rejected_host_abi",
            "plugin rejected the host ABI table during the entry call",
        )
        .with_detail(format!("plugin returned `{}`", result::name(code)))
    } else {
        PluginError::load(
            "entry_point_failed",
            "plugin entry point returned a failure code",
        )
        .with_detail(format!(
            "returned `{}` ({code}); {}",
            result::name(code),
            if code == result::REGISTRATION {
                "an export could not be registered"
            } else {
                "see the plugin SDK for result-code semantics"
            }
        ))
    }
}

fn cross_check_exports(
    manifest: &PluginManifest,
    registrations: Vec<RegisteredExport>,
) -> Result<BTreeMap<String, RegisteredExport>, PluginError> {
    let mut registered: BTreeMap<String, RegisteredExport> = BTreeMap::new();
    for export in registrations {
        if registered.insert(export.name.clone(), export).is_some() {
            return Err(PluginError::load(
                "duplicate_registered_export",
                "plugin registered the same export more than once",
            ));
        }
    }
    let declared: BTreeMap<&str, &str> = manifest
        .plugin
        .exports
        .iter()
        .map(|export| (export.name.as_str(), export.signature.as_str()))
        .collect();
    for (name, signature) in &declared {
        let Some(registered_export) = registered.get(*name) else {
            return Err(PluginError::load(
                "export_not_registered",
                format!("plugin did not register declared export `{name}`"),
            ));
        };
        if registered_export.signature.trim() != signature.trim() {
            return Err(PluginError::load(
                "export_signature_mismatch",
                format!("export `{name}` signature differs between manifest and registration"),
            )
            .with_detail(format!(
                "manifest `{signature}`, registered `{}`",
                registered_export.signature
            )));
        }
    }
    for name in registered.keys() {
        if !declared.contains_key(name.as_str()) {
            return Err(PluginError::load(
                "unexpected_export",
                format!("plugin registered undeclared export `{name}`"),
            ));
        }
    }
    Ok(registered)
}

/// Host allocator: `malloc`-like, with an internal size header so the matching
/// `free` needs no length argument. Returns NULL for `size == 0`.
unsafe extern "C" fn host_alloc(_host_data: *mut c_void, size: usize) -> *mut c_void {
    const HEADER: usize = 16;
    const ALIGN: usize = 16;
    if size == 0 {
        return ptr::null_mut();
    }
    let Some(total) = size.checked_add(HEADER) else {
        return ptr::null_mut();
    };
    let Ok(layout) = std::alloc::Layout::from_size_align(total, ALIGN) else {
        return ptr::null_mut();
    };
    // SAFETY: layout has non-zero size (size > 0), checked for overflow.
    let base = unsafe { std::alloc::alloc(layout) };
    if base.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: base has at least HEADER bytes and is 16-byte aligned.
    unsafe { (base as *mut usize).write(size) };
    // SAFETY: offset stays within the allocation.
    unsafe { base.add(HEADER) as *mut c_void }
}

/// Host deallocator paired with [`host_alloc`].
unsafe extern "C" fn host_free(_host_data: *mut c_void, pointer: *mut c_void) {
    const HEADER: usize = 16;
    const ALIGN: usize = 16;
    if pointer.is_null() {
        return;
    }
    // SAFETY: only pointers returned by host_alloc are freed here (ABI
    // contract); the header stores the original requested size.
    unsafe {
        let base = (pointer as *mut u8).sub(HEADER);
        let size = (base as *const usize).read();
        let layout = std::alloc::Layout::from_size_align_unchecked(size + HEADER, ALIGN);
        std::alloc::dealloc(base, layout);
    }
}

/// Host export-registration callback.
unsafe extern "C" fn host_register_function(
    host_data: *mut c_void,
    export_name: MqBytes,
    signature_wire: MqBytes,
    invoke: MqPluginInvoke,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: host_data points at the HostState owned by the loading
        // LoadedPlugin for the duration of the entry call.
        unsafe { register_function_impl(host_data, export_name, signature_wire, invoke) }
    }));
    result.unwrap_or(result::INTERNAL)
}

unsafe fn register_function_impl(
    host_data: *mut c_void,
    export_name: MqBytes,
    signature_wire: MqBytes,
    invoke: MqPluginInvoke,
) -> i32 {
    if host_data.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: non-NULL host_data per the ABI contract.
    let state = unsafe { &mut *(host_data as *mut HostState) };

    // SAFETY: borrowed buffers are valid for the duration of this callback.
    let name_bytes = match unsafe { export_name.as_slice() } {
        Ok(bytes) => bytes,
        Err(_) => return result::INVALID_ARGUMENT,
    };
    if name_bytes.is_empty() || name_bytes.len() > MAX_EXPORT_NAME_BYTES {
        return result::INVALID_ARGUMENT;
    }
    let Ok(name) = std::str::from_utf8(name_bytes) else {
        return result::INVALID_ARGUMENT;
    };

    // SAFETY: borrowed buffers are valid for the duration of this callback.
    let signature_bytes = match unsafe { signature_wire.as_slice() } {
        Ok(bytes) => bytes,
        Err(_) => return result::INVALID_ARGUMENT,
    };
    if signature_bytes.len() > MAX_SIGNATURE_WIRE_BYTES {
        return result::INVALID_ARGUMENT;
    }
    let Ok(signature) = serde_json::from_slice::<SignatureWire>(signature_bytes) else {
        return result::BAD_WIRE;
    };
    if signature.signature.trim().is_empty() {
        return result::BAD_WIRE;
    }

    if state
        .registrations
        .iter()
        .any(|registered| registered.name == name)
    {
        return result::REGISTRATION;
    }
    state.registrations.push(RegisteredExport {
        name: name.to_string(),
        signature: signature.signature,
        invoke,
    });
    result::OK
}

/// Host capability-request callback.
unsafe extern "C" fn host_request_capability(
    ctx: *mut MqCallContext,
    name: MqBytes,
    request_wire: MqBytes,
    reply_wire: *mut MqOwnedBytes,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: pointers follow the invocation ABI contract; buffers are
        // borrowed for the duration of this callback.
        unsafe { request_capability_impl(ctx, name, request_wire, reply_wire) }
    }));
    result.unwrap_or(result::INTERNAL)
}

unsafe fn request_capability_impl(
    ctx: *mut MqCallContext,
    name: MqBytes,
    request_wire: MqBytes,
    reply_wire: *mut MqOwnedBytes,
) -> i32 {
    if ctx.is_null() || reply_wire.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: output pointer is non-NULL and host-provided.
    unsafe { *reply_wire = MqOwnedBytes::EMPTY };
    // SAFETY: ctx was minted by this host for the current invocation.
    let state_pointer = unsafe { (*ctx).host_data() } as *mut HostState;
    if state_pointer.is_null() {
        return result::INVALID_ARGUMENT;
    }

    // SAFETY: borrowed buffers are valid for the duration of this callback.
    let name_bytes = match unsafe { name.as_slice() } {
        Ok(bytes) => bytes,
        Err(_) => return result::INVALID_ARGUMENT,
    };
    let Ok(capability) = std::str::from_utf8(name_bytes) else {
        return result::INVALID_ARGUMENT;
    };
    // SAFETY: borrowed buffers are valid for the duration of this callback.
    let request = match unsafe { request_wire.as_slice() } {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => return result::INVALID_ARGUMENT,
    };

    // SAFETY: state is owned by the live LoadedPlugin whose invocation is in
    // progress; no other Rust reference is held across this callback.
    let state = unsafe { &mut *state_pointer };
    let Some(handler) = state.capability_handler.as_mut() else {
        return result::CAPABILITY;
    };
    let reply = match handler(capability, &request) {
        Ok(reply) => reply,
        Err(_) => return result::CAPABILITY,
    };
    if reply.len() > MAX_CAPABILITY_REPLY_BYTES {
        return result::BAD_WIRE;
    }
    if reply.is_empty() {
        return result::OK;
    }
    // SAFETY: host_alloc is the allocator plugin code frees with host_free.
    let pointer = unsafe { host_alloc(state_pointer as *mut c_void, reply.len()) };
    if pointer.is_null() {
        return result::OUT_OF_MEMORY;
    }
    // SAFETY: pointer has reply.len() bytes; reply is a distinct allocation.
    unsafe {
        std::ptr::copy_nonoverlapping(reply.as_ptr(), pointer as *mut u8, reply.len());
        *reply_wire = MqOwnedBytes::new(pointer as *mut u8, reply.len());
    }
    result::OK
}

/// Copy a plugin output buffer into a `Vec`, enforce its size limit, and release
/// the buffer with the host allocator.
///
/// # Safety
///
/// `buffer` must be a plugin output buffer whose pointer (if non-NULL) was
/// allocated by the host allocator, and it must not have been freed yet.
unsafe fn take_host_buffer(
    host_api: &MqHostApi,
    buffer: &mut MqOwnedBytes,
    limit: usize,
    what: &str,
) -> Result<Vec<u8>, PluginError> {
    // SAFETY: output pointer is host-owned and non-NULL.
    let copy = unsafe { buffer.to_vec() }?;
    let mut taken = MqOwnedBytes::EMPTY;
    std::mem::swap(&mut taken, buffer);
    if taken.len > limit {
        // Free the oversized buffer before reporting the protocol violation.
        // SAFETY: taken.data came from the host allocator.
        unsafe { free_host_buffer(host_api, &taken) };
        return Err(PluginError::wire(
            "output_too_large",
            format!(
                "plugin {what} wire is {} bytes, limit is {limit}",
                taken.len
            ),
        ));
    }
    // SAFETY: taken.data came from the host allocator.
    unsafe { free_host_buffer(host_api, &taken) };
    Ok(copy)
}

/// Release a host-allocated buffer.
///
/// # Safety
///
/// `buffer.data` must be NULL or a pointer previously returned by the host
/// allocator and not yet freed.
unsafe fn free_host_buffer(host_api: &MqHostApi, buffer: &MqOwnedBytes) {
    if buffer.data.is_null() {
        return;
    }
    // SAFETY: alloc/free pair and non-NULL pointer per the caller contract.
    unsafe { (host_api.free)(host_api.host_data, buffer.data as *mut c_void) };
}

/// Validate a host API table (useful for tests and SDK conformance checks).
pub fn validate_host_table(host: &MqHostApi) -> Result<AbiVersion, PluginError> {
    // SAFETY: we pass a valid reference converted to a pointer.
    unsafe { check_host_api(host as *const MqHostApi) }
}
