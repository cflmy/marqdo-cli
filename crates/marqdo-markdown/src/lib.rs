//! Canonical Markdown AST for Marqdo.
//!
//! This crate is the only place that talks to the Markdown parser. It freezes
//! the CommonMark/GFM/Math profile (see [`profile`]), converts the parser tree
//! into an owned [`ast::MdNode`] tree with byte spans, and extracts YAML front
//! matter. It performs **no** semantic mapping and **no** execution: deciding
//! which AST nodes are executable is the frontend's job and is based on the
//! node kind and structural position only.

pub mod ast;
pub mod frontmatter;
pub mod profile;
pub mod text;

pub use ast::{MdKind, MdNode};
pub use frontmatter::FrontMatter;
pub use marqdo_span::{LineCol, Span};
pub use profile::{parse, MarkdownDocument, COMRAK_VERSION, PROFILE_ID};
pub use text::{executable_payload, normalize_name, plain_text, PayloadError};
