//! Marqdo language values, diagnostics and errors.
//!
//! This crate is the shared data contract between the frontend, the IR, the
//! runtime and the host boundary. It has no dependency on Markdown parsing,
//! process execution or the CLI so that every layer can agree on values and
//! structured failures without creating dependency cycles.

pub mod diagnostic;
pub mod error;
pub mod value;
pub mod wire;

pub use diagnostic::{codes, Diagnostic, Related, Severity};
pub use error::{CallFrame, ErrorCategory, RuntimeError};
pub use marqdo_span::{LineCol, Span};
pub use value::{
    bigint_to_exact_f64, float_to_bigint, format_float, CodeValue, FormulaValue, FunctionRef,
    MemoryHandle, PluginHandle, ResourceHandle, Table, Value,
};
pub use wire::{from_wire, to_wire, WireError, WIRE_VERSION};
