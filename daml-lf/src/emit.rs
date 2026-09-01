//! Building Daml-LF packages.
//!
//! The counterpart of the decoding the rest of this crate does: given a
//! description of some templates, produce the archive a participant needs in
//! order to type, store and project their contracts.

mod build;
mod dar;
mod schema;

pub use build::build_archive;
pub use dar::build_dar;
pub use schema::{
    Choice, Ctor, DataBody, DataType, Field, FieldType, Module, Package, ResultType, Template,
    TemplateKey, TypeRef,
};
