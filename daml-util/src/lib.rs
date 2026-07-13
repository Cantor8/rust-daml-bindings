//! Daml ledger utilities.
//!
//! This provides utilities which depends on both [`daml-grpc`](daml_grpc) and [`daml-lf`](daml_lf) crates.

#![warn(clippy::all, clippy::pedantic, clippy::nursery, rust_2018_idioms)]
#![allow(
    clippy::missing_errors_doc,
    clippy::used_underscore_binding,
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::missing_const_for_fn,
    clippy::return_self_not_must_use,
    // Style-only pedantic / nursery lints intentionally allowed
    // workspace-wide — none affect correctness.
    clippy::large_enum_variant,
    clippy::too_many_lines,
    clippy::non_canonical_partial_ord_impl,
    clippy::too_long_first_doc_paragraph,
    clippy::option_if_let_else,
    clippy::trivially_copy_pass_by_ref,
)]
#![forbid(unsafe_code)]
#![doc(html_favicon_url = "https://docs.daml.com/_static/images/favicon/favicon-32x32.png")]
#![doc(html_logo_url = "https://docs.daml.com/_static/images/DAML_Logo_Blue.svg")]
#![doc(html_root_url = "https://docs.rs/daml-util/0.3.0")]

/// JWT token builder for Canton v2 participants
/// (`aud`/`sub`/`scope`-shape claims).
pub mod canton_auth;
pub use canton_auth::{DamlCantonClaims, DamlCantonTokenBuilder, DamlCantonTokenError, DamlCantonTokenResult};

/// Conveniences for working with a collection of [`DamlPackage`](daml_grpc::data::package::DamlPackage).
pub mod package;
