//! Marqdo frontend: Markdown AST → unit index → executable IR.
//!
//! The frontend is the semantic mapper described by
//! `spec/mapping.md`. It owns heading scopes, executable-role assignment,
//! payload parsing, signature/parameter analysis and static call validation.
//! It never executes user code and never touches the host boundary.

pub mod analyze;
pub mod calls;
pub mod infer;
pub mod map;
pub mod payload;
pub mod resources;
pub mod signature;
pub mod units;

pub use analyze::{analyze_source, analyze_source_for_program, AnalysisResult};
pub use units::{build_units, github_slug, FunctionScope, HeadingInfo, UnitIndex};
