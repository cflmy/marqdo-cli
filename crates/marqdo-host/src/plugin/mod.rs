//! Execution side of the Callable Plugin ABI: manifest/artifact validation,
//! native loading, invocation dispatch and the worker process.
//!
//! The public contract (C layout, wire envelope and protocol errors) lives in
//! `marqdo-plugin-abi`; this module owns `dlopen`, process spawning, artifact
//! file selection and trusted invocation. The CLI may consume the static
//! descriptor types for `check`/`inspect` preparation, but preparation never
//! loads a library or starts a process.
//!
//! The `include/marqdo_plugin_abi_v1.h` header and the Rust layout assertions
//! in `marqdo_plugin_abi::abi` remain the single ABI authority.

pub mod artifact;
pub mod loader;
pub mod manifest;
pub mod worker;

pub use artifact::{
    current_target_triple, is_valid_sha256_hex, sha256_bytes, sha256_file, verify_sha256_file,
    HostTarget, TargetTriple,
};
pub use loader::{CapabilityHandler, LoadOptions, LoadedPlugin, RegisteredExport};
pub use manifest::{
    AbiRequirement, Artifact, Export, PackageVersion, PluginInfo, PluginKind, PluginManifest,
    KNOWN_CAPABILITIES,
};
