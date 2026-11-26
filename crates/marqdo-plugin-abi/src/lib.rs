//! Public contract for the versioned plugin C ABI v1.
//!
//! This crate is the single normative Rust side of
//! `include/marqdo_plugin_abi_v1.h`: the stable `#[repr(C)]` types with layout
//! assertions and major/minor negotiation ([`abi`]), the `wire_version` 1
//! JSON request/response envelope with typed value tags ([`wire`]), and the
//! structured protocol [`PluginError`].
//!
//! It is a contract crate, not a loader. Native loading (`dlopen`), artifact
//! selection on disk, manifest file validation, worker process management and
//! capability dispatch live in the native host (`marqdo-host::plugin`). The
//! SDK consumes only this crate. There is no second wire encoding here: the
//! wire envelope reuses the single [`marqdo_value`] encoding.
//!
//! A C ABI is not a sandbox. A host that loads native code must separately make
//! and enforce an explicit trust decision; this crate grants no capability.

pub mod abi;
pub mod error;
pub mod wire;

pub use abi::result as result_code;
pub use abi::{
    check_host_api, negotiate, AbiVersion, MqAllocFn, MqBytes, MqCallContext, MqFreeFn, MqHostApi,
    MqOwnedBytes, MqPluginApi, MqPluginEntryV1, MqPluginInvoke, MqRegisterFunctionFn,
    MqRequestCapabilityFn, PluginMetadata, ABI_MAJOR, ABI_MINOR, ABI_VERSION_STRING, ENTRY_SYMBOL,
};
pub use error::{ErrorKind, PluginError};
pub use marqdo_value::Value;
pub use wire::{
    bool_from_value, int_from_value, text_from_value, to_wire_json, WireContext, WireErrorBody,
    WireRequest, WireResponse, WIRE_VERSION,
};
