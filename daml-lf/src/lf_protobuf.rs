#![allow(clippy::all, clippy::pedantic)]

// Daml-LF 2.x archive protos, generated from
// resources/protobuf/com/digitalasset/daml/lf/archive/{daml_lf.proto, daml_lf2.proto}.
//
// `daml_lf` carries the `Archive` / `ArchivePayload` envelope (proto package `daml_lf`),
// `daml_lf_2` carries the LF2 `Package` AST (proto package `daml_lf_2`).
pub mod daml_lf {
    include!(concat!(env!("OUT_DIR"), "/daml_lf.rs"));
}

pub mod daml_lf_2 {
    include!(concat!(env!("OUT_DIR"), "/daml_lf_2.rs"));
}
