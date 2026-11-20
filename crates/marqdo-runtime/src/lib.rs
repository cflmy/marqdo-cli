//! Rust tree-walk interpreter for Marqdo IR.
//!
//! The interpreter is Marqdo's own execution engine: expressions, scoping,
//! calls, branches, loops and returns are evaluated here, never by translating
//! to Python or another language. Every side effect goes through
//! [`marqdo_host::Host`]; prose never reaches this layer because it is absent
//! from the IR.

mod builtins_impl;
mod interpreter;
mod value_ops;

pub use interpreter::{run, EvaluatedArg, Interpreter, RunOutcome};

pub use marqdo_value::{RuntimeError, Value};
