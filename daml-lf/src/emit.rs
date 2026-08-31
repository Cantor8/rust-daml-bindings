//! Building Daml-LF packages.
//!
//! The counterpart of the decoding the rest of this crate does: given a
//! description of some templates, produce the archive a participant needs in
//! order to type, store and project their contracts.

mod build;
mod schema;

pub use build::{build_archive, encode_archive};
pub use schema::{Field, FieldType, Module, Package, Template};
