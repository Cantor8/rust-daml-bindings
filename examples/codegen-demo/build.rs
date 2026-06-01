//! Placeholder build script for the codegen-demo stub.
//!
//! The original build.rs invoked `daml::codegen::generator::daml_codegen`
//! against `resources/rental/archive/rental-0_1_0-sdk_1_18_1-lf_1_14.dar`,
//! but that fixture is LF 1.14 and the v2 `daml-lf` crate doesn't
//! load LF1. Phase 7 will check in an LF2-compiled replacement DAR
//! (with an interface implementation so Phase 4b / 4c codegen has
//! something to exercise); the body of this build.rs will then
//! re-invoke `daml_codegen` against that fixture.

fn main() {}
