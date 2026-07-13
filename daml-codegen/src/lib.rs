#![warn(clippy::all, clippy::pedantic, clippy::nursery, rust_2018_idioms)]
#![allow(
    clippy::module_name_repetitions,
    clippy::needless_pass_by_value,
    clippy::use_self,
    clippy::cast_sign_loss,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    // Style-only pedantic / nursery lints intentionally allowed
    // workspace-wide — none affect correctness.
    clippy::too_long_first_doc_paragraph,
    clippy::doc_lazy_continuation,
    clippy::doc_markdown,
    clippy::large_enum_variant,
    clippy::trivially_copy_pass_by_ref,
    clippy::too_many_lines,
    clippy::non_canonical_partial_ord_impl,
    clippy::derive_partial_eq_without_eq,
    clippy::option_if_let_else
)]
#![forbid(unsafe_code)]
#![doc(html_favicon_url = "https://docs.daml.com/_static/images/favicon/favicon-32x32.png")]
#![doc(html_logo_url = "https://docs.daml.com/_static/images/DAML_Logo_Blue.svg")]
#![doc(html_root_url = "https://docs.rs/daml-codegen/0.3.0")]
#![recursion_limit = "128"]
// importing the crate README as the rust doc breaks the link to the LICENCE file.
#![allow(rustdoc::broken_intra_doc_links)]
#![doc = include_str!("../README.md")]

mod error;

// The renderer is exposed so that it may be used by `daml-derive` only, it is not part of the public interface.
#[doc(hidden)]
pub mod renderer;

/// Code generators for producing Rust implementations of Daml types.
pub mod generator;
