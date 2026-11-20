//! Versioned plugin C ABI v1 bindings, manifest validation and artifact
//! loading (host side).
//!
//! This crate is the host-side counterpart of
//! `include/marqdo_plugin_abi_v1.h` (the frozen C ABI) and of the
//! `marqdo-plugin-sdk` crate (the plugin-developer side). It implements the
//! blueprint section 10 contract:
//!
//! - stable `#[repr(C)]` Rust mirrors of the C types, with compile-time layout
//!   assertions and major/minor negotiation ([`abi`]),
//! - `plugin.yaml` parsing and validation ([`manifest`]),
//! - target-triple artifact selection and sha256 verification ([`artifact`]),
//! - `wire_version` 1 JSON request/response envelopes with typed value tags
//!   ([`wire`]),
//! - a native loader/invocation bridge built on `libloading` ([`loader`]),
//! - structured, serializable [`PluginError`]s ([`error`]).
//!
//! A C ABI is not a sandbox: native plugins run in-process and require an
//! explicit trust decision. This crate only validates and loads what the
//! caller already decided to trust; it never grants capabilities.

pub mod abi;
pub mod artifact;
pub mod error;
pub mod loader;
pub mod manifest;
pub mod wire;
pub mod worker;

pub use abi::result as result_code;
pub use abi::{
    check_host_api, negotiate, AbiVersion, MqAllocFn, MqBytes, MqCallContext, MqFreeFn, MqHostApi,
    MqOwnedBytes, MqPluginApi, MqPluginEntryV1, MqPluginInvoke, MqRegisterFunctionFn,
    MqRequestCapabilityFn, PluginMetadata, ABI_MAJOR, ABI_MINOR, ABI_VERSION_STRING, ENTRY_SYMBOL,
};
pub use artifact::{
    current_target_triple, is_valid_sha256_hex, sha256_bytes, sha256_file, verify_sha256_file,
    HostTarget, TargetTriple,
};
pub use error::{ErrorKind, PluginError};
pub use loader::{CapabilityHandler, LoadOptions, LoadedPlugin, RegisteredExport};
pub use manifest::{
    AbiRequirement, Artifact, Export, PackageVersion, PluginInfo, PluginKind, PluginManifest,
    KNOWN_CAPABILITIES,
};
pub use marqdo_value::Value;
pub use wire::{
    bool_from_value, int_from_value, text_from_value, to_wire_json, WireContext, WireErrorBody,
    WireRequest, WireResponse, WIRE_VERSION,
};
