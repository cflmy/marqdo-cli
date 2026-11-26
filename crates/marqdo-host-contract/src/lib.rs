//! Platform-neutral Host contract.
//!
//! This crate owns the only side-effect interface the language engine knows:
//! the [`Host`] trait, its [`HostOp`] surface, the per-call
//! [`ExecutionContext`], the opaque resource read result and the runtime
//! observation notifications. It is a Rust compile-time interface inside one
//! codebase, not a wire protocol: there is no HTTP, RPC, C ABI or dynamic
//! loading boundary here.
//!
//! The crate is deliberately neutral. It depends only on the language value
//! and span types; it must never link the native host, the CLI, the sandbox,
//! foreign kernels, the plugin loader or a project filesystem. Native and
//! embedded hosts implement [`Host`] without re-implementing any language
//! semantics. Authorization, budgets, resource identity and OS isolation stay
//! owned by the trusted host implementation that consumes this contract.

pub mod context;
pub mod host;
pub mod observe;
pub mod resource;

pub use context::ExecutionContext;
pub use host::{Host, HostOp};
pub use observe::{FrameId, LoopTransfer, ParameterSource, RuntimeObservation};
pub use resource::ResourceRead;
