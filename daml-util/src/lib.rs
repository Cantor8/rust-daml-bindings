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
    clippy::return_self_not_must_use
)]
#![forbid(unsafe_code)]
#![doc(html_favicon_url = "https://docs.daml.com/_static/images/favicon/favicon-32x32.png")]
#![doc(html_logo_url = "https://docs.daml.com/_static/images/DAML_Logo_Blue.svg")]
#![doc(html_root_url = "https://docs.rs/daml-util/0.3.0")]

// The v1-shaped sandbox JWT token builder was removed in the v2
// migration: Canton v2 uses a different claim shape
// (`aud` / `scope` / `sub` style audience-scoped tokens) that the
// v1 `https://daml.com/ledger-api` claim doesn't model. A v2 token
// helper is a planned follow-up.

/// Conveniences for working with a collection of [`DamlPackage`](daml_grpc::data::package::DamlPackage).
pub mod package;
