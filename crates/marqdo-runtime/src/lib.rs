//! Rust tree-walk interpreter for Marqdo IR.
//!
//! The interpreter is Marqdo's own execution engine: expressions, scoping,
//! calls, branches, loops and returns are evaluated here, never by translating
//! to Python or another language. Every side effect goes through
//! [`marqdo_host_contract::Host`]; prose never reaches this layer because it is absent
//! from the IR.

mod builtins_impl;
mod interpreter;
mod memory;
mod value_ops;

pub use interpreter::{run, run_with_limits, EvaluatedArg, Interpreter, RunOutcome};
pub use memory::{
    MemoryLimits, MemoryStore, DEFAULT_MAX_REGION_BYTES, DEFAULT_MAX_TOTAL_BYTES, PAGE_SIZE,
};

pub use marqdo_value::{RuntimeError, Value};
