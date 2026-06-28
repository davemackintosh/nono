//! Nono — a spite-driven static site generator.
//!
//! Library root. The `nono` binary (src/main.rs) and the integration tests
//! both depend on these modules.

pub mod ast;
pub mod build;
pub mod config;
pub mod eval;
pub mod html;
pub mod parser;
pub mod scaffold;
pub mod serve;
pub mod sources;
pub mod value;
